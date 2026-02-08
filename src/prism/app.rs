use std::{net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use anyhow::Context;
use tokio::task::JoinSet;

use crate::prism::{admin, config, logging, protocol, proxy, router, telemetry};

pub async fn run(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    let resolved = config::resolve_config_path(config_path)?;

    let created = config::ensure_config_file(&resolved.path)?;

    let cfg = config::load_config(&resolved.path)
        .with_context(|| format!("load config: {}", resolved.path.display()))?;

    let logrt = logging::init(&cfg.logging, &cfg.opentelemetry)?;
    let _logrt_guard = logrt; // keep alive

    if created {
        tracing::warn!(path = %resolved.path.display(), source = %resolved.source, "config: created new config file");
    }

    let proxy_enabled = !cfg.listeners.is_empty();
    let tunnel_server_enabled = !cfg.tunnel.endpoints.is_empty();
    let tunnel_client_enabled = cfg.tunnel.client.is_some() && !cfg.tunnel.services.is_empty();
    let admin_enabled = !cfg.admin_addr.trim().is_empty() && (proxy_enabled || tunnel_server_enabled);

    if !proxy_enabled && !tunnel_server_enabled && !tunnel_client_enabled {
        anyhow::bail!("config: nothing to run (set listeners and/or routes and/or tunnel.endpoints and/or tunnel.client+services)");
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
    let metrics = Arc::new(telemetry::MetricsCollector::new());
    let sessions = Arc::new(telemetry::SessionRegistry::new());

    // Routing stack.
    let host_parser = protocol::build_host_parser(&cfg.routing_parsers)?;
    let rtr = Arc::new(router::Router::new(cfg.routes.clone()));

    let (reload_tx, _reload_rx) = tokio::sync::watch::channel(telemetry::ReloadSignal::new());

    let mut tasks = JoinSet::new();

    // Admin server.
    if admin_enabled {
        let addr: SocketAddr = cfg
            .admin_addr
            .parse()
            .with_context(|| format!("invalid admin_addr: {}", cfg.admin_addr))?;

        let admin_state = admin::AdminState {
            metrics: metrics.clone(),
            sessions: sessions.clone(),
            config_path: resolved.path.clone(),
            reload_tx: reload_tx.clone(),
        };

        tasks.spawn(async move {
            admin::serve(addr, admin_state).await
        });
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
                            metrics: metrics.clone(),
                            sessions: sessions.clone(),
                            max_header_bytes: cfg.max_header_bytes,
                            handshake_timeout: cfg.timeouts.handshake_timeout,
                            idle_timeout: cfg.timeouts.idle_timeout,
                            upstream_dial_timeout: cfg.upstream_dial_timeout,
                            buffer_size: cfg.buffer_size,
                        })
                    } else {
                        proxy::TcpHandler::forward(proxy::TcpForwardHandlerOptions {
                            upstream,
                            metrics: metrics.clone(),
                            sessions: sessions.clone(),
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

    // Tunnel roles are not yet implemented in Rust.
    if tunnel_server_enabled || tunnel_client_enabled {
        tracing::warn!("tunnel mode is not implemented yet in Rust (config parsed, but functionality pending)");
    }

    // Wait for shutdown signal.
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("shutdown: ctrl-c");
        }
        res = tasks.join_next() => {
            if let Some(res) = res {
                match res {
                    Ok(Ok(())) => {}
                    Ok(Err(err)) => return Err(err),
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
