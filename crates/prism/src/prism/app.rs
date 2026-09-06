use std::{net::SocketAddr, path::Path, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::task::JoinSet;

use crate::prism::middleware::MiddlewareProvider;
use crate::prism::{
    admin, config, logging, managed, middleware, net, proxy, router, runtime_paths, telemetry,
    tunnel,
};

pub async fn run(
    config_path: Option<PathBuf>,
    workdir: Option<PathBuf>,
    middleware_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let resolved = config::resolve_config_path(config_path)?;

    let paths = runtime_paths::resolve_runtime_paths(workdir, &resolved.path, middleware_dir)?;

    let created = config::ensure_config_file(&resolved.path)?;

    let bootstrap_cfg = config::load_config(&resolved.path)
        .with_context(|| format!("load config: {}", resolved.path.display()))?;

    let logrt = logging::init(&bootstrap_cfg.logging)?;
    let _logrt_guard = logrt; // keep alive

    if created {
        tracing::warn!(path = %resolved.path.display(), source = %resolved.source, "config: created new config file");
    }

    let created_mws = middleware::materialize_default_middlewares(&paths.middleware_dir)
        .with_context(|| {
            format!(
                "materialize middlewares: {}",
                paths.middleware_dir.display()
            )
        })?;
    if !created_mws.is_empty() {
        tracing::info!(
            middleware_dir = %paths.middleware_dir.display(),
            created = created_mws.len(),
            "middleware: materialized default middlewares"
        );
    }

    let management_plane = match bootstrap_cfg.role {
        config::PrismRole::Management => Some(Arc::new(managed::ManagementPlane::open(
            &paths.workdir,
            bootstrap_cfg
                .managed
                .management
                .as_ref()
                .expect("management role validated in config"),
        )?)),
        _ => None,
    };

    let worker_agent = match bootstrap_cfg.role {
        config::PrismRole::Worker => Some(Arc::new(managed::WorkerAgent::open(
            &paths.workdir,
            bootstrap_cfg
                .managed
                .worker
                .as_ref()
                .expect("worker role validated in config"),
        )?)),
        _ => None,
    };

    if let Some(worker_agent) = &worker_agent
        && worker_agent.connection_mode() == config::ManagedConnectionMode::Active
    {
        if let Err(err) = worker_agent.sync_once().await {
            tracing::warn!(
                node_id = %bootstrap_cfg.managed.worker.as_ref().expect("worker config present").node_id,
                err = %err,
                "managed: initial worker sync failed; starting from persisted state"
            );
        }
    }

    let startup_managed_cfg = if let Some(worker_agent) = &worker_agent {
        worker_agent.startup_config().await.map(|(_, cfg)| cfg)
    } else {
        None
    };

    let cfg = if let Some(startup_managed_cfg) = startup_managed_cfg.as_ref() {
        config::overlay_managed_config_document(&bootstrap_cfg, startup_managed_cfg)?
    } else if bootstrap_cfg.role == config::PrismRole::Worker {
        config::worker_bootstrap_runtime_config(&bootstrap_cfg)
    } else {
        bootstrap_cfg.clone()
    };

    let proxy_enabled = !cfg.listeners.is_empty();
    let tunnel_server_enabled = !cfg.tunnel.endpoints.is_empty();
    let tunnel_connector_enabled =
        cfg.tunnel.connector.is_some() && !cfg.tunnel.services.is_empty();
    let tunnel_client_enabled = cfg.tunnel.client.is_some();

    if !proxy_enabled
        && !tunnel_server_enabled
        && !tunnel_connector_enabled
        && !tunnel_client_enabled
        && !matches!(
            cfg.role,
            config::PrismRole::Management | config::PrismRole::Worker
        )
    {
        anyhow::bail!(
            "config: nothing to run (set listeners and/or routes and/or tunnel.endpoints and/or tunnel.connector+services and/or tunnel.client)"
        );
    }

    tracing::info!(
        config = %resolved.path.display(),
        workdir = %paths.workdir.display(),
        middleware_dir = %paths.middleware_dir.display(),
        role = %cfg.role,
        proxy_enabled,
        tunnel_server_enabled,
        tunnel_connector_enabled,
        tunnel_client_enabled,
        admin_addr = %cfg.admin_addr,
        proxy_listeners = cfg.listeners.len(),
        tunnel_endpoints = cfg.tunnel.endpoints.len(),
        tunnel_services = cfg.tunnel.services.len(),
        "prism: starting"
    );

    // Shared state for admin endpoints.
    let sessions = Arc::new(telemetry::SessionRegistry::new());
    let optimizer = Arc::new(telemetry::OptimizerStatsRegistry::new());
    let tunnel_manager = Arc::new(tunnel::manager::Manager::new());
    let auth_manager = Arc::new(crate::prism::auth::AuthManager::new(
        cfg.auth.clone(),
        Some(&paths.workdir),
    ));

    // Routing stack.
    let routes_with_middlewares = build_routes_with_middlewares(&cfg, &paths.middleware_dir)?;
    let rtr = Arc::new(router::Router::new(routes_with_middlewares));

    let tcp_runtime = Arc::new(tokio::sync::RwLock::new(proxy::TcpRuntimeConfig {
        max_header_bytes: cfg.max_header_bytes,
        handshake_timeout: cfg.timeouts.handshake_timeout,
        idle_timeout: cfg.timeouts.idle_timeout,
        upstream_dial_timeout: cfg.upstream_dial_timeout,
        buffer_size: cfg.buffer_size,
        proxy_protocol_v2: cfg.proxy_protocol_v2,
    }));

    let (reload_tx, reload_rx) = tokio::sync::watch::channel(telemetry::ReloadSignal::new());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut tasks = JoinSet::new();

    // Config reload loop (polling + admin-triggered).
    if cfg.role != config::PrismRole::Worker {
        let config_path = resolved.path.clone();
        let static_cfg = cfg.clone();
        let router = rtr.clone();
        let runtime = tcp_runtime.clone();
        let middleware_dir = paths.middleware_dir.clone();
        let mut reload_rx = reload_rx.clone();
        let mut shutdown = shutdown_rx.clone();
        let mut enabled = cfg.reload.enabled;
        let mut poll = cfg.reload.poll_interval;

        tasks.spawn(async move {
            reload_loop(
                config_path,
                static_cfg,
                middleware_dir,
                router,
                runtime,
                &mut reload_rx,
                &mut shutdown,
                &mut enabled,
                &mut poll,
            )
            .await;
            Ok(())
        });
    }

    let client_controller = Arc::new(tunnel::client::ClientController::new(Some(
        paths.middleware_dir.clone(),
    )));

    // Admin server (either external public/private or loopback ephemeral for internal stream).
    let mut bound_admin_addr: Option<SocketAddr> = None;
    if !cfg.admin_addr.trim().is_empty() {
        let admin_addr = net::normalize_bind_addr(&cfg.admin_addr);
        let addr: SocketAddr = admin_addr
            .parse()
            .with_context(|| format!("invalid admin_addr: {}", cfg.admin_addr))?;

        let admin_state = admin::AdminState {
            sessions: sessions.clone(),
            optimizer: optimizer.clone(),
            config_path: resolved.path.clone(),
            reload_tx: reload_tx.clone(),
            tunnel: Some(tunnel_manager.clone()),
            auth: admin::AdminAuth {
                panel_token: management_plane
                    .as_ref()
                    .map(|plane| plane.panel_token().to_string()),
                worker_token: if let Some(plane) = &management_plane {
                    Some(plane.worker_token().to_string())
                } else {
                    worker_agent
                        .as_ref()
                        .map(|agent| agent.auth_token().to_string())
                },
            },
            management: management_plane.clone(),
            worker: worker_agent.clone(),
            client: Some(client_controller.clone()),
            auth_manager: Some(auth_manager.clone()),
            serve_frontend: true,
        };

        let listener = tokio::net::TcpListener::bind(addr).await?;
        let local_addr = listener.local_addr()?;
        bound_admin_addr = Some(local_addr);
        let shutdown = shutdown_rx.clone();
        tasks.spawn(async move {
            admin::serve_listener_with_shutdown(listener, admin_state, shutdown).await
        });
    } else if tunnel_server_enabled
        || proxy_enabled
        || matches!(
            cfg.role,
            config::PrismRole::Management | config::PrismRole::Worker
        )
    {
        // No explicit admin_addr configured (e.g. to avoid public web exposure / compliance).
        // Bind an internal loopback ephemeral listener for in-band tunnel management streams.
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let local_addr = listener.local_addr()?;
        bound_admin_addr = Some(local_addr);
        tracing::info!(
            internal_admin = %local_addr,
            "admin: internal loopback listener started for in-band tunnel streams"
        );

        let admin_state = admin::AdminState {
            sessions: sessions.clone(),
            optimizer: optimizer.clone(),
            config_path: resolved.path.clone(),
            reload_tx: reload_tx.clone(),
            tunnel: Some(tunnel_manager.clone()),
            auth: admin::AdminAuth {
                panel_token: management_plane
                    .as_ref()
                    .map(|plane| plane.panel_token().to_string()),
                worker_token: if let Some(plane) = &management_plane {
                    Some(plane.worker_token().to_string())
                } else {
                    worker_agent
                        .as_ref()
                        .map(|agent| agent.auth_token().to_string())
                },
            },
            management: management_plane.clone(),
            worker: worker_agent.clone(),
            client: Some(client_controller.clone()),
            auth_manager: Some(auth_manager.clone()),
            serve_frontend: true,
        };

        let shutdown = shutdown_rx.clone();
        tasks.spawn(async move {
            admin::serve_listener_with_shutdown(listener, admin_state, shutdown).await
        });
    }

    // Proxy listeners.
    if proxy_enabled {
        for l in &cfg.listeners {
            match l.protocol.as_str() {
                "tcp" => {
                    let listen_addr = l.listen_addr.clone();
                    let upstream = l.upstream.clone();
                    let shutdown = shutdown_rx.clone();

                    let handler = if upstream.trim().is_empty() {
                        proxy::TcpHandler::routing(proxy::TcpRoutingHandlerOptions {
                            router: rtr.clone(),
                            sessions: sessions.clone(),
                            tunnel_manager: Some(tunnel_manager.clone()),
                            runtime: tcp_runtime.clone(),
                        })
                    } else {
                        proxy::TcpHandler::forward(proxy::TcpForwardHandlerOptions {
                            upstream,
                            sessions: sessions.clone(),
                            tunnel_manager: Some(tunnel_manager.clone()),
                            runtime: tcp_runtime.clone(),
                        })
                    };

                    tasks.spawn(async move {
                        proxy::serve_tcp_with_shutdown(&listen_addr, handler, shutdown).await
                    });
                }
                "udp" => {
                    let listen_addr = l.listen_addr.clone();
                    let upstream = l.upstream.clone();
                    let shutdown = shutdown_rx.clone();

                    if upstream.trim().is_empty() {
                        tracing::warn!(listen_addr = %listen_addr, "udp listener missing upstream; skipping");
                        continue;
                    }

                    let opts = proxy::UdpForwardOptions {
                        upstream,
                        sessions: sessions.clone(),
                        tunnel_manager: Some(tunnel_manager.clone()),
                        idle_timeout: cfg.timeouts.idle_timeout,
                    };

                    tasks.spawn(async move {
                        proxy::serve_udp_with_shutdown(&listen_addr, opts, shutdown).await
                    });
                }
                other => {
                    tracing::warn!(listen_addr = %l.listen_addr, protocol = %other, "unsupported listener protocol");
                }
            }
        }
    }

    // Tunnel server.
    if tunnel_server_enabled {
        for ep in &cfg.tunnel.endpoints {
            let server = tunnel::server::Server::new(tunnel::server::ServerOptions {
                listen_addr: ep.listen_addr.clone(),
                transport: ep.transport.clone(),
                auth_token: cfg.tunnel.auth_token.clone(),
                quic: tunnel::server::QuicServerOptions {
                    cert_file: ep.quic.cert_file.clone(),
                    key_file: ep.quic.key_file.clone(),
                },
                websocket: tunnel::server::WebSocketServerOptions {
                    cert_file: ep.websocket.cert_file.clone(),
                    key_file: ep.websocket.key_file.clone(),
                },
                manager: tunnel_manager.clone(),
                auth_manager: Some(auth_manager.clone()),
                admin_addr: bound_admin_addr,
            })?;

            let shutdown = shutdown_rx.clone();
            tasks.spawn(async move { server.listen_and_serve(shutdown).await });
        }

        if cfg.tunnel.auto_listen_services {
            let al = tunnel::autolisten::AutoListener::new(
                tunnel_manager.clone(),
                tunnel::autolisten::AutoListenOptions::default(),
            );
            let shutdown = shutdown_rx.clone();
            tasks.spawn(async move { al.run(shutdown).await });
        }
    }

    // Tunnel connector (service publisher).
    if tunnel_connector_enabled {
        let conn = cfg.tunnel.connector.as_ref().expect("checked above");
        let services = cfg
            .tunnel
            .services
            .iter()
            .map(|s| tunnel::protocol::RegisteredService {
                name: s.name.clone(),
                proto: s.proto.clone(),
                local_addr: s.local_addr.clone(),
                route_only: s.route_only,
                remote_addr: s.remote_addr.clone(),
                masquerade_host: s.masquerade_host.clone(),
                middleware: s.middleware.clone(),
                optimizer: s.optimizer.clone(),
            })
            .collect::<Vec<_>>();

        let quic_opts = conn
            .quic
            .as_ref()
            .map(|q| tunnel::connector::QuicConnectorOptions {
                server_name: q.server_name.clone(),
                insecure_skip_verify: q.insecure_skip_verify,
            })
            .unwrap_or_else(|| tunnel::connector::QuicConnectorOptions {
                server_name: String::new(),
                insecure_skip_verify: false,
            });

        let ws_opts = conn
            .websocket
            .as_ref()
            .map(|w| tunnel::connector::WebSocketConnectorOptions {
                server_name: w.server_name.clone(),
                insecure_skip_verify: w.insecure_skip_verify,
            })
            .unwrap_or_else(|| tunnel::connector::WebSocketConnectorOptions {
                server_name: String::new(),
                insecure_skip_verify: false,
            });

        let connector = tunnel::connector::Connector::new(tunnel::connector::ConnectorOptions {
            server_addr: conn.server_addr.clone(),
            transport: conn.transport.clone(),
            auth_token: conn.auth_token.clone(),
            services,
            dial_timeout: conn.dial_timeout,
            quic: quic_opts,
            websocket: ws_opts,
            middleware_dir: Some(paths.middleware_dir.clone()),
            optimizer: Some(optimizer.clone()),
        })?;

        let connector = Arc::new(connector);
        let shutdown = shutdown_rx.clone();
        tasks.spawn(async move { connector.run(shutdown).await });

        // mDNS local service discovery + local proxy.
        if cfg.tunnel.mdns.enabled && !cfg.tunnel.mdns.listen_addr.trim().is_empty() {
            // Build service name -> local_addr map for the local proxy.
            let mut svc_map = std::collections::HashMap::new();
            for s in &cfg.tunnel.services {
                let name = s.name.trim().to_ascii_lowercase();
                if !name.is_empty() && !s.local_addr.trim().is_empty() {
                    svc_map.insert(name, s.local_addr.trim().to_string());
                }
            }

            // Determine proxy port for mDNS advertisement.
            let bind_addr = net::normalize_bind_addr(&cfg.tunnel.mdns.listen_addr);
            let port: u16 = bind_addr
                .rsplit_once(':')
                .and_then(|(_, p)| p.parse().ok())
                .unwrap_or(0);

            // Start mDNS responder.
            match tunnel::mdns::MdnsResponder::new(
                &cfg.tunnel.mdns.domain,
                &cfg.tunnel.mdns.subdomain,
                port,
                "", // auto-detect LAN IP
            ) {
                Ok(mut mdns) => {
                    let names: Vec<&str> = svc_map.keys().map(|s| s.as_str()).collect();
                    mdns.reconcile(&names);
                    // Keep mdns alive until shutdown.
                    let mut shutdown = shutdown_rx.clone();
                    tasks.spawn(async move {
                        let _ = shutdown.changed().await;
                        mdns.shutdown();
                        Ok(())
                    });
                }
                Err(err) => {
                    tracing::warn!(err = %err, "mdns: failed to start responder (continuing without mDNS)");
                }
            }

            // Optional Minecraft LAN multicast broadcaster for zero-config client discovery.
            if cfg.tunnel.mdns.minecraft_lan {
                let broadcaster = tunnel::fake_lan::FakeLanBroadcaster::new();
                for s in &cfg.tunnel.services {
                    let name = s.name.trim();
                    if !name.is_empty() && !s.local_addr.trim().is_empty() && !s.route_only {
                        broadcaster
                            .add_service(tunnel::fake_lan::AdvertisedService::new(
                                name,
                                port,
                                &cfg.tunnel.mdns.motd_prefix,
                            ))
                            .await;
                    }
                }
                let shutdown = shutdown_rx.clone();
                tasks.spawn(async move {
                    if let Err(err) = broadcaster.run(shutdown).await {
                        tracing::warn!(err = %err, "mdns: minecraft fake lan broadcaster exited with error");
                    }
                    Ok(())
                });
            }

            // Build middleware chain for hostname extraction.
            if !cfg.tunnel.mdns.middlewares.is_empty() {
                let provider =
                    middleware::FsWasmMiddlewareProvider::new(paths.middleware_dir.clone());
                match provider.chain(&cfg.tunnel.mdns.middlewares) {
                    Ok(chain) => {
                        let proxy = tunnel::local_proxy::LocalProxy::new(
                            tunnel::local_proxy::LocalProxyConfig {
                                listen_addr: cfg.tunnel.mdns.listen_addr.clone(),
                                max_header_bytes: cfg.max_header_bytes,
                                handshake_timeout: cfg.timeouts.handshake_timeout,
                                domain: cfg.tunnel.mdns.domain.clone(),
                                subdomain: cfg.tunnel.mdns.subdomain.clone(),
                            },
                            Arc::new(svc_map),
                            chain,
                        );
                        let shutdown = shutdown_rx.clone();
                        tasks.spawn(async move { proxy.run(shutdown).await });
                    }
                    Err(err) => {
                        tracing::warn!(
                            err = %err,
                            middlewares = ?cfg.tunnel.mdns.middlewares,
                            "mdns: failed to build middleware chain for local proxy"
                        );
                    }
                }
            } else {
                tracing::warn!(
                    "mdns: no middlewares configured; local proxy will not start \
                     (set tunnel.mdns.middlewares)"
                );
            }
        }
    }

    // Tunnel client sidecar.
    if tunnel_client_enabled {
        let tc = cfg.tunnel.client.as_ref().expect("checked above");
        tracing::info!(
            server_addr = %tc.server_addr,
            listen_addr = %tc.listen_addr,
            middleware = ?tc.middleware,
            fake_lan_broadcast = tc.fake_lan_broadcast,
            "tunnel client sidecar enabled"
        );
        let client = tunnel::client::Client::new(tc.clone())?
            .with_middleware_dir(paths.middleware_dir.clone());
        let client = Arc::new(client);
        let (client_shutdown_tx, client_shutdown_rx) = tokio::sync::watch::channel(false);
        client_controller
            .attach(client.clone(), client_shutdown_tx)
            .await;
        let mut shutdown = shutdown_rx.clone();
        tasks.spawn(async move {
            tokio::select! {
                res = client.run(client_shutdown_rx) => res,
                _ = shutdown.changed() => Ok(()),
            }
        });
    }

    if let Some(worker_agent) = &worker_agent {
        worker_agent
            .attach_runtime(managed::RuntimeApplyHandles {
                middleware_dir: paths.middleware_dir.clone(),
                router: rtr.clone(),
                runtime: tcp_runtime.clone(),
            })
            .await;

        if startup_managed_cfg.is_some() {
            worker_agent.mark_started_with_startup_config().await?;
        }

        if worker_agent.connection_mode() == config::ManagedConnectionMode::Active {
            let worker_agent = worker_agent.clone();
            let shutdown = shutdown_rx.clone();
            tasks.spawn(async move {
                worker_agent.run_active_sync_loop(shutdown).await;
                Ok(())
            });
        }
    }

    // Wait for shutdown signal (Ctrl-C / SIGTERM) or unexpected task termination.
    tokio::select! {
        _ = shutdown_signal() => {
            tracing::info!("shutdown: signal");
            let _ = shutdown_tx.send(true);
        }
        res = tasks.join_next() => {
            if let Some(res) = res {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => {
                        let _ = shutdown_tx.send(true);
                        return Err(err);
                    }
                    Err(join_err) => return Err(join_err.into()),
                }
            }
        }
    }

    // Drain tasks: exit as soon as they complete; only enforce a timeout if something hangs.
    let drain = async {
        while let Some(_res) = tasks.join_next().await {
            // Best-effort: tasks are expected to observe shutdown; ignore errors during teardown.
        }
    };

    // Hard cap so `docker stop` doesn't stall indefinitely.
    let drain_timeout = Duration::from_secs(5);
    if tokio::time::timeout(drain_timeout, drain).await.is_err() {
        tasks.abort_all();
        while tasks.join_next().await.is_some() {}
    }

    Ok(())
}

async fn shutdown_signal() {
    // Ctrl-C works cross-platform.
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};

        let mut term = signal(SignalKind::terminate()).expect("install SIGTERM handler");

        tokio::select! {
            _ = tokio::signal::ctrl_c() => {}
            _ = term.recv() => {}
        }
    }

    #[cfg(not(unix))]
    {
        let _ = tokio::signal::ctrl_c().await;
    }
}

#[allow(clippy::too_many_arguments)]
async fn reload_loop(
    config_path: PathBuf,
    static_cfg: config::Config,
    middleware_dir: PathBuf,
    router: Arc<router::Router>,
    runtime: Arc<tokio::sync::RwLock<proxy::TcpRuntimeConfig>>,
    reload_rx: &mut tokio::sync::watch::Receiver<telemetry::ReloadSignal>,
    shutdown: &mut tokio::sync::watch::Receiver<bool>,
    enabled: &mut bool,
    poll_interval: &mut Duration,
) {
    let mut last_sig = file_sig(&config_path).ok();

    loop {
        let sleep_dur = if *enabled {
            (*poll_interval).max(Duration::from_millis(200))
        } else {
            Duration::from_secs(3600)
        };

        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = reload_rx.changed() => {
                apply_reload(
                    &config_path,
                    &static_cfg,
                    &middleware_dir,
                    &router,
                    &runtime,
                    enabled,
                    poll_interval,
                ).await;
                last_sig = file_sig(&config_path).ok();
            }
            _ = tokio::time::sleep(sleep_dur) => {
                if !*enabled {
                    continue;
                }
                let sig = match file_sig(&config_path) {
                    Ok(s) => s,
                    Err(_) => continue,
                };
                if last_sig.is_some_and(|prev| prev == sig) {
                    continue;
                }
                apply_reload(
                    &config_path,
                    &static_cfg,
                    &middleware_dir,
                    &router,
                    &runtime,
                    enabled,
                    poll_interval,
                ).await;
                last_sig = Some(sig);
            }
        }
    }
}

async fn apply_reload(
    config_path: &Path,
    static_cfg: &config::Config,
    middleware_dir: &Path,
    router: &Arc<router::Router>,
    runtime: &Arc<tokio::sync::RwLock<proxy::TcpRuntimeConfig>>,
    enabled: &mut bool,
    poll_interval: &mut Duration,
) {
    let cfg = match config::load_config(config_path) {
        Ok(c) => c,
        Err(err) => {
            tracing::warn!(path=%config_path.display(), err=%err, "reload: config load failed");
            return;
        }
    };

    if let Err(err) = middleware::materialize_default_middlewares(middleware_dir) {
        tracing::warn!(
            middleware_dir = %middleware_dir.display(),
            err = %err,
            "reload: failed to materialize default middlewares"
        );
    }

    let restart_reasons = config::restart_required_reasons(static_cfg, &cfg);
    if !restart_reasons.is_empty() {
        tracing::warn!(reasons = ?restart_reasons, "reload: restart required for static topology changes");
    }

    if let Err(err) = apply_runtime_config_update(&cfg, middleware_dir, router, runtime).await {
        tracing::warn!(err=%err, "reload: hot-apply failed");
        return;
    }

    *enabled = cfg.reload.enabled;
    *poll_interval = cfg.reload.poll_interval;

    tracing::info!("reload: applied");
}

pub(crate) async fn apply_runtime_config_update(
    cfg: &config::Config,
    middleware_dir: &Path,
    router: &Arc<router::Router>,
    runtime: &Arc<tokio::sync::RwLock<proxy::TcpRuntimeConfig>>,
) -> anyhow::Result<()> {
    let routes_with_middlewares = build_routes_with_middlewares(cfg, middleware_dir)?;
    router.update(routes_with_middlewares);
    *runtime.write().await = proxy::TcpRuntimeConfig {
        max_header_bytes: cfg.max_header_bytes,
        handshake_timeout: cfg.timeouts.handshake_timeout,
        idle_timeout: cfg.timeouts.idle_timeout,
        upstream_dial_timeout: cfg.upstream_dial_timeout,
        buffer_size: cfg.buffer_size,
        proxy_protocol_v2: cfg.proxy_protocol_v2,
    };
    Ok(())
}

pub(crate) fn build_routes_with_middlewares(
    cfg: &config::Config,
    middleware_dir: &Path,
) -> anyhow::Result<Vec<(config::RouteConfig, middleware::SharedMiddlewareChain)>> {
    let provider = middleware::FsWasmMiddlewareProvider::new(middleware_dir.to_path_buf());
    let mut out = Vec::with_capacity(cfg.routes.len());
    for (i, r) in cfg.routes.iter().enumerate() {
        let chain = provider
            .chain(&r.middlewares)
            .with_context(|| format!("route[{}] build middleware chain", i))?;
        out.push((r.clone(), chain));
    }
    Ok(out)
}

fn file_sig(path: &Path) -> anyhow::Result<(u64, u64)> {
    let meta = std::fs::metadata(path)?;
    let len = meta.len();
    let m = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    Ok((m, len))
}
