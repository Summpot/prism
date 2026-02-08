use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::task::JoinSet;

use crate::prism::{admin, config, logging, net, protocol, proxy, router, telemetry, tunnel};

pub async fn run(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let resolved = config::resolve_config_path(config_path)?;

    let created = config::ensure_config_file(&resolved.path)?;

    let cfg = config::load_config(&resolved.path)
        .with_context(|| format!("load config: {}", resolved.path.display()))?;

    let logrt = logging::init(&cfg.logging)?;
    let _logrt_guard = logrt; // keep alive

    if created {
        tracing::warn!(path = %resolved.path.display(), source = %resolved.source, "config: created new config file");
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
    let host_parser = protocol::build_host_parser(&cfg.routing_parsers)?;
    let rtr = Arc::new(router::Router::new(cfg.routes.clone()));

    let (reload_tx, _reload_rx) = tokio::sync::watch::channel(telemetry::ReloadSignal::new());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    let mut tasks = JoinSet::new();

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

        tasks.spawn(async move { admin::serve(addr, admin_state).await });
    }

    // Proxy listeners.
    if proxy_enabled {
        for l in &cfg.listeners {
            match l.protocol.as_str() {
                "tcp" => {
                    let listen_addr = l.listen_addr.clone();
                    let upstream = l.upstream.clone();

                    let handler = if upstream.trim().is_empty() {
                        proxy::TcpHandler::routing(proxy::TcpRoutingHandlerOptions {
                            parser: host_parser.clone(),
                            router: rtr.clone(),
                            sessions: sessions.clone(),
                            tunnel_manager: Some(tunnel_manager.clone()),
                            max_header_bytes: cfg.max_header_bytes,
                            handshake_timeout: cfg.timeouts.handshake_timeout,
                            idle_timeout: cfg.timeouts.idle_timeout,
                            upstream_dial_timeout: cfg.upstream_dial_timeout,
                            buffer_size: cfg.buffer_size,
                        })
                    } else {
                        proxy::TcpHandler::forward(proxy::TcpForwardHandlerOptions {
                            upstream,
                            sessions: sessions.clone(),
                            tunnel_manager: Some(tunnel_manager.clone()),
                            idle_timeout: cfg.timeouts.idle_timeout,
                            upstream_dial_timeout: cfg.upstream_dial_timeout,
                            buffer_size: cfg.buffer_size,
                        })
                    };

                    tasks.spawn(async move { proxy::serve_tcp(&listen_addr, handler).await });
                }
                "udp" => {
                    // TODO: UDP forwarder
                    tracing::warn!(listen_addr = %l.listen_addr, "udp listener configured but UDP is not implemented yet");
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

    // Wait for shutdown signal.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutdown: ctrl-c");
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

    // Give tasks a moment to shut down gracefully.
    let deadline = tokio::time::sleep(Duration::from_secs(2));
    tokio::pin!(deadline);
    loop {
        tokio::select! {
            biased;
            _ = &mut deadline => break,
            res = tasks.join_next() => {
                if res.is_none() {
                    break;
                }
            }
        }
    }

    Ok(())
}
