use std::{net::SocketAddr, path::Path, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::task::JoinSet;

use crate::prism::middleware::MiddlewareProvider;
use crate::prism::{
    admin, config, logging, middleware, net, proxy, router, runtime_paths, telemetry, tunnel,
};

pub async fn run(
    config_path: Option<PathBuf>,
    workdir: Option<PathBuf>,
    middleware_dir: Option<PathBuf>,
) -> anyhow::Result<()> {
    let resolved = config::resolve_config_path(config_path)?;

    let paths = runtime_paths::resolve_runtime_paths(workdir, &resolved.path, middleware_dir)?;

    let created = config::ensure_config_file(&resolved.path)?;

    let cfg = config::load_config(&resolved.path)
        .with_context(|| format!("load config: {}", resolved.path.display()))?;

    let logrt = logging::init(&cfg.logging)?;
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

    let proxy_enabled = !cfg.listeners.is_empty();
    let tunnel_server_enabled = !cfg.tunnel.endpoints.is_empty();
    let tunnel_client_enabled = cfg.tunnel.client.is_some() && !cfg.tunnel.services.is_empty();
    let admin_enabled = !cfg.admin_addr.trim().is_empty()
        && (proxy_enabled || tunnel_server_enabled || tunnel_client_enabled);

    if !proxy_enabled && !tunnel_server_enabled && !tunnel_client_enabled {
        anyhow::bail!(
            "config: nothing to run (set listeners and/or routes and/or tunnel.endpoints and/or tunnel.client+services)"
        );
    }

    tracing::info!(
        config = %resolved.path.display(),
        workdir = %paths.workdir.display(),
        middleware_dir = %paths.middleware_dir.display(),
        proxy_enabled,
        tunnel_server_enabled,
        tunnel_client_enabled,
        admin_addr = %cfg.admin_addr,
        proxy_listeners = cfg.listeners.len(),
        tunnel_endpoints = cfg.tunnel.endpoints.len(),
        tunnel_services = cfg.tunnel.services.len(),
        "prism: starting"
    );

    // Shared state for admin endpoints.
    let prom = Arc::new(telemetry::init_prometheus()?);
    let sessions = Arc::new(telemetry::SessionRegistry::new());
    let tunnel_manager = Arc::new(tunnel::manager::Manager::new());

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
    {
        let config_path = resolved.path.clone();
        let static_listeners = cfg.listeners.clone();
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
                static_listeners,
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

    // Admin server.
    if admin_enabled {
        let admin_addr = net::normalize_bind_addr(&cfg.admin_addr);
        let addr: SocketAddr = admin_addr
            .parse()
            .with_context(|| format!("invalid admin_addr: {}", cfg.admin_addr))?;

        let admin_state = admin::AdminState {
            prom: prom.clone(),
            sessions: sessions.clone(),
            config_path: resolved.path.clone(),
            reload_tx: reload_tx.clone(),
            tunnel: Some(tunnel_manager.clone()),
        };

        let shutdown = shutdown_rx.clone();
        tasks.spawn(async move { admin::serve_with_shutdown(addr, admin_state, shutdown).await });
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
                manager: tunnel_manager.clone(),
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

    // Tunnel client.
    if tunnel_client_enabled {
        let cc = cfg.tunnel.client.as_ref().expect("checked above");
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
            })
            .collect::<Vec<_>>();

        let client = tunnel::client::Client::new(tunnel::client::ClientOptions {
            server_addr: cc.server_addr.clone(),
            transport: cc.transport.clone(),
            auth_token: cfg.tunnel.auth_token.clone(),
            services,
            dial_timeout: cc.dial_timeout,
            quic: tunnel::client::QuicClientOptions {
                server_name: cc.quic.server_name.clone(),
                insecure_skip_verify: cc.quic.insecure_skip_verify,
            },
        })?;

        let client = Arc::new(client);
        let shutdown = shutdown_rx.clone();
        tasks.spawn(async move { client.run(shutdown).await });
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

async fn reload_loop(
    config_path: PathBuf,
    static_listeners: Vec<config::ProxyListenerConfig>,
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
                    &static_listeners,
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
                    &static_listeners,
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
    config_path: &PathBuf,
    static_listeners: &[config::ProxyListenerConfig],
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

    // Listener topology changes require restart.
    if !listeners_equal(static_listeners, &cfg.listeners) {
        tracing::warn!(
            "reload: listener topology changed; restart required to apply listener changes"
        );
    }

    match build_routes_with_middlewares(&cfg, middleware_dir) {
        Ok(routes_with_middlewares) => {
            router.update(routes_with_middlewares);
        }
        Err(err) => {
            tracing::warn!(err=%err, "reload: rebuild per-route middlewares failed");
            return;
        }
    }

    *runtime.write().await = proxy::TcpRuntimeConfig {
        max_header_bytes: cfg.max_header_bytes,
        handshake_timeout: cfg.timeouts.handshake_timeout,
        idle_timeout: cfg.timeouts.idle_timeout,
        upstream_dial_timeout: cfg.upstream_dial_timeout,
        buffer_size: cfg.buffer_size,
        proxy_protocol_v2: cfg.proxy_protocol_v2,
    };

    *enabled = cfg.reload.enabled;
    *poll_interval = cfg.reload.poll_interval;

    tracing::info!("reload: applied");
}

fn build_routes_with_middlewares(
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

fn listeners_equal(a: &[config::ProxyListenerConfig], b: &[config::ProxyListenerConfig]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (x, y) in a.iter().zip(b.iter()) {
        if x.listen_addr.trim() != y.listen_addr.trim() {
            return false;
        }
        if x.protocol.trim() != y.protocol.trim() {
            return false;
        }
        if x.upstream.trim() != y.upstream.trim() {
            return false;
        }
    }
    true
}

fn file_sig(path: &PathBuf) -> anyhow::Result<(u64, u64)> {
    let meta = std::fs::metadata(path)?;
    let len = meta.len();
    let m = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64;
    Ok((m, len))
}
