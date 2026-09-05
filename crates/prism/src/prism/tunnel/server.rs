use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::prism::tunnel::{
    manager::Manager,
    protocol,
    transport::{TransportListenOptions, transport_by_name},
};

#[derive(Debug, Clone)]
pub struct QuicServerOptions {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub listen_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub quic: QuicServerOptions,
    pub manager: Arc<Manager>,
}

pub struct Server {
    opts: ServerOptions,
}

impl Server {
    pub fn new(opts: ServerOptions) -> anyhow::Result<Self> {
        Ok(Self { opts })
    }

    #[allow(dead_code)]
    pub fn manager(&self) -> Arc<Manager> {
        self.opts.manager.clone()
    }

    pub async fn listen_and_serve(
        &self,
        ctx: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let tr = transport_by_name(&self.opts.transport)?;

        let ln = tr
            .listen(
                &self.opts.listen_addr,
                TransportListenOptions {
                    quic: crate::prism::tunnel::transport::QuicListenOptions {
                        cert_file: self.opts.quic.cert_file.clone(),
                        key_file: self.opts.quic.key_file.clone(),
                        next_protos: vec![],
                    },
                },
            )
            .await?;

        tracing::info!(
            addr = %self.opts.listen_addr,
            transport = %tr.name(),
            "tunnel: listening"
        );

        let mut shutdown = ctx.clone();
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                sess = ln.accept() => {
                    let sess = sess?;
                    let mgr = self.opts.manager.clone();
                    let token = self.opts.auth_token.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_session(mgr, sess, token).await {
                            tracing::warn!(err=%err, "tunnel: session ended with error");
                        }
                    });
                }
            }
        }

        ln.close().await?;
        Ok(())
    }
}

async fn handle_session(
    mgr: Arc<Manager>,
    sess: Arc<dyn crate::prism::tunnel::transport::TransportSession>,
    auth_token: String,
) -> anyhow::Result<()> {
    let remote = sess
        .remote_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();

    // First stream must be register.
    let mut reg = sess.accept_stream().await?;
    let req = protocol::read_register_request(&mut reg).await?;

    if !auth_token.trim().is_empty() && req.token != auth_token {
        tracing::warn!(client=%remote, "tunnel: bad token");
        sess.close().await;
        return Ok(());
    }

    if req.is_client() {
        let cid = mgr.next_client_id("cs");
        mgr.register_client_session(cid.clone(), sess.clone())
            .await?;
        tracing::info!(cid=%cid, client=%remote, "tunnel: client sidecar connected");

        // Broadcast loop on reg stream sending active services whenever services change.
        let mgr_broadcast = mgr.clone();
        let broadcast_task = tokio::spawn(async move {
            let mut sub = mgr_broadcast.subscribe();
            let initial = mgr_broadcast.active_services().await;
            if protocol::write_service_catalog(&mut reg, &initial)
                .await
                .is_err()
            {
                return;
            }
            while sub.changed().await.is_ok() {
                let services = mgr_broadcast.active_services().await;
                if protocol::write_service_catalog(&mut reg, &services)
                    .await
                    .is_err()
                {
                    break;
                }
            }
        });

        // Accept streams from connected Client that writes a PRPX header.
        while let Ok(client_stream) = sess.accept_stream().await {
            let mgr = mgr.clone();
            tokio::spawn(async move {
                if let Err(err) = handle_client_stream(mgr, client_stream).await {
                    tracing::debug!(err=%err, "tunnel: client stream relay ended");
                }
            });
        }

        broadcast_task.abort();
        mgr.unregister_client_session(&cid).await;
        tracing::info!(cid=%cid, client=%remote, "tunnel: client sidecar disconnected");
    } else {
        let cid = mgr.next_client_id("c");
        mgr.register_client(cid.clone(), sess.clone(), req.services)
            .await?;
        tracing::info!(cid=%cid, client=%remote, "tunnel: connector connected");

        // Hold an accept loop to detect disconnects and close unexpected streams.
        while let Ok(mut st) = sess.accept_stream().await {
            let _ = tokio::time::timeout(std::time::Duration::from_secs(1), st.shutdown()).await;
        }

        mgr.unregister_client(&cid).await;
        tracing::info!(cid=%cid, client=%remote, "tunnel: connector disconnected");
    }

    Ok(())
}

async fn handle_client_stream(
    mgr: Arc<Manager>,
    mut client_stream: crate::prism::tunnel::transport::BoxedStream,
) -> anyhow::Result<()> {
    let (kind, service_name, flags) =
        protocol::read_proxy_stream_header_with_flags(&mut client_stream).await?;
    match kind {
        protocol::ProxyStreamKind::Tcp => {
            let (mut conn_stream, _meta) = if flags != 0 {
                mgr.dial_service_tcp_with_flags(&service_name, flags)
                    .await?
            } else {
                mgr.dial_service_tcp_with_meta(&service_name).await?
            };
            let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut conn_stream).await;
        }
        protocol::ProxyStreamKind::Udp => {
            let mut conn_stream = mgr.dial_service_udp(&service_name).await?;
            let _ = tokio::io::copy_bidirectional(&mut client_stream, &mut conn_stream).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prism::tunnel::transport::TransportSession;
    use std::net::SocketAddr;
    use tokio::io::AsyncReadExt;
    use tokio::sync::mpsc;

    struct MockSession {
        accept_rx: tokio::sync::Mutex<mpsc::Receiver<crate::prism::tunnel::transport::BoxedStream>>,
        open_tx:
            tokio::sync::Mutex<Option<mpsc::Sender<crate::prism::tunnel::transport::BoxedStream>>>,
        close_tx: tokio::sync::watch::Sender<bool>,
    }

    impl MockSession {
        fn new(
            rx: mpsc::Receiver<crate::prism::tunnel::transport::BoxedStream>,
            tx: Option<mpsc::Sender<crate::prism::tunnel::transport::BoxedStream>>,
        ) -> Self {
            let (close_tx, _) = tokio::sync::watch::channel(false);
            Self {
                accept_rx: tokio::sync::Mutex::new(rx),
                open_tx: tokio::sync::Mutex::new(tx),
                close_tx,
            }
        }
    }

    #[async_trait::async_trait]
    impl crate::prism::tunnel::transport::TransportSession for MockSession {
        async fn open_stream(
            &self,
        ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
            let (client_side, server_side) = tokio::io::duplex(64 * 1024);
            let guard = self.open_tx.lock().await;
            if let Some(tx) = guard.as_ref() {
                tx.send(Box::new(server_side))
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            Ok(Box::new(client_side))
        }

        async fn accept_stream(
            &self,
        ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
            let mut close_rx = self.close_tx.subscribe();
            if *close_rx.borrow() {
                return Err(anyhow::anyhow!("closed"));
            }
            let mut guard = self.accept_rx.lock().await;
            tokio::select! {
                _ = close_rx.changed() => Err(anyhow::anyhow!("closed")),
                res = guard.recv() => res.ok_or_else(|| anyhow::anyhow!("channel closed")),
            }
        }

        async fn close(&self) {
            let _ = self.close_tx.send(true);
        }
        fn remote_addr(&self) -> Option<SocketAddr> {
            None
        }
        fn local_addr(&self) -> Option<SocketAddr> {
            None
        }
    }

    #[tokio::test]
    async fn server_connector_registration_registers_services() {
        let mgr = Arc::new(Manager::new());
        let (accept_tx, accept_rx) = mpsc::channel(16);
        let sess = Arc::new(MockSession::new(accept_rx, None));

        let (mut reg_client, reg_server) = tokio::io::duplex(4096);
        accept_tx.send(Box::new(reg_server)).await.unwrap();

        // Write connector register request
        let req = protocol::RegisterRequest {
            client_type: "connector".into(),
            token: "secret".into(),
            services: vec![protocol::RegisteredService {
                name: "test-svc".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:8080".into(),
                route_only: false,
                remote_addr: "".into(),
                masquerade_host: "".into(),
                middleware: None,
                traffic_optimizer: None,
            }],
        };

        tokio::spawn(async move {
            protocol::write_register_request(&mut reg_client, &req)
                .await
                .unwrap();
        });

        let mgr_clone = mgr.clone();
        let sess_clone = sess.clone();
        let handle = tokio::spawn(async move {
            let _ = handle_session(mgr_clone, sess_clone, "secret".into()).await;
        });

        // Wait briefly for registration
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(mgr.has_service("test-svc").await);

        let active = mgr.active_services().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].name, "test-svc");

        drop(accept_tx);
        let _ = handle.await;
    }

    #[tokio::test]
    async fn server_client_sidecar_receives_catalog_broadcast() {
        let mgr = Arc::new(Manager::new());
        let (accept_tx, accept_rx) = mpsc::channel(16);
        let sess = Arc::new(MockSession::new(accept_rx, None));

        let (mut reg_client, reg_server) = tokio::io::duplex(4096);
        accept_tx.send(Box::new(reg_server)).await.unwrap();

        // Write client sidecar register request
        let req = protocol::RegisterRequest {
            client_type: "client".into(),
            token: "".into(),
            services: vec![],
        };

        let w_handle = tokio::spawn(async move {
            protocol::write_register_request(&mut reg_client, &req)
                .await
                .unwrap();
            // Read initial service catalog
            let catalog1 = protocol::read_service_catalog(&mut reg_client)
                .await
                .unwrap();
            // Wait for next catalog after service added
            let catalog2 = protocol::read_service_catalog(&mut reg_client)
                .await
                .unwrap();
            (catalog1, catalog2)
        });

        let mgr_clone = mgr.clone();
        let sess_clone = sess.clone();
        tokio::spawn(async move {
            let _ = handle_session(mgr_clone, sess_clone, "".into()).await;
        });

        // Initially empty
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // Register a service on connector
        let (conn_tx, conn_rx) = mpsc::channel(16);
        let conn_sess = Arc::new(MockSession::new(conn_rx, None));
        let (mut conn_reg_c, conn_reg_s) = tokio::io::duplex(4096);
        conn_tx.send(Box::new(conn_reg_s)).await.unwrap();

        let conn_req = protocol::RegisterRequest {
            client_type: "connector".into(),
            token: "".into(),
            services: vec![protocol::RegisteredService {
                name: "dyn-svc".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:9090".into(),
                ..Default::default()
            }],
        };
        tokio::spawn(async move {
            protocol::write_register_request(&mut conn_reg_c, &conn_req)
                .await
                .unwrap();
        });

        let mgr_c = mgr.clone();
        let conn_sess_clone = conn_sess.clone();
        tokio::spawn(async move {
            let _ = handle_session(mgr_c, conn_sess_clone, "".into()).await;
        });

        let (c1, c2) = w_handle.await.unwrap();
        assert_eq!(c1.len(), 0);
        assert_eq!(c2.len(), 1);
        assert_eq!(c2[0].name, "dyn-svc");

        sess.close().await;
        conn_sess.close().await;
        drop(accept_tx);
        drop(conn_tx);
    }

    #[tokio::test]
    async fn server_relays_raw_bytes_between_client_stream_and_connector_stream() {
        let mgr = Arc::new(Manager::new());

        // 1. Setup connector session
        let (conn_accept_tx, conn_accept_rx) = mpsc::channel(16);
        let (conn_open_tx, mut conn_open_rx) = mpsc::channel(16);
        let conn_sess = Arc::new(MockSession::new(conn_accept_rx, Some(conn_open_tx)));

        let (mut conn_reg_c, conn_reg_s) = tokio::io::duplex(4096);
        conn_accept_tx.send(Box::new(conn_reg_s)).await.unwrap();
        let conn_req = protocol::RegisterRequest {
            client_type: "connector".into(),
            token: "".into(),
            services: vec![protocol::RegisteredService {
                name: "echo".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:80".into(),
                ..Default::default()
            }],
        };
        tokio::spawn(async move {
            protocol::write_register_request(&mut conn_reg_c, &conn_req)
                .await
                .unwrap();
        });
        let mgr_c = mgr.clone();
        let conn_sess_clone = conn_sess.clone();
        tokio::spawn(async move {
            let _ = handle_session(mgr_c, conn_sess_clone, "".into()).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert!(mgr.has_service("echo").await);

        // 2. Setup client sidecar session
        let (client_accept_tx, client_accept_rx) = mpsc::channel(16);
        let client_sess = Arc::new(MockSession::new(client_accept_rx, None));
        let (mut client_reg_c, client_reg_s) = tokio::io::duplex(4096);
        client_accept_tx.send(Box::new(client_reg_s)).await.unwrap();
        let client_req = protocol::RegisterRequest {
            client_type: "client".into(),
            token: "".into(),
            services: vec![],
        };
        tokio::spawn(async move {
            protocol::write_register_request(&mut client_reg_c, &client_req)
                .await
                .unwrap();
        });
        let mgr_cs = mgr.clone();
        let client_sess_clone = client_sess.clone();
        tokio::spawn(async move {
            let _ = handle_session(mgr_cs, client_sess_clone, "".into()).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. Client opens stream to server
        let (mut client_stream_c, client_stream_s) = tokio::io::duplex(4096);
        client_accept_tx
            .send(Box::new(client_stream_s))
            .await
            .unwrap();

        // Write PRPX header from client
        protocol::write_proxy_stream_header_with_flags(
            &mut client_stream_c,
            protocol::ProxyStreamKind::Tcp,
            "echo",
            0,
        )
        .await
        .unwrap();

        // Write client payload
        client_stream_c.write_all(b"PING").await.unwrap();

        // Connector accepts stream opened by manager.dial_service_tcp_with_meta
        let mut conn_stream_s = conn_open_rx.recv().await.unwrap();
        // Read header on connector
        let (kind, svc, flags) = protocol::read_proxy_stream_header_with_flags(&mut conn_stream_s)
            .await
            .unwrap();
        assert_eq!(kind, protocol::ProxyStreamKind::Tcp);
        assert_eq!(svc, "echo");
        assert_eq!(flags, 0);

        // Read payload on connector
        let mut buf = [0u8; 4];
        conn_stream_s.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"PING");

        // Connector sends reply
        conn_stream_s.write_all(b"PONG").await.unwrap();

        // Client receives reply
        let mut reply = [0u8; 4];
        client_stream_c.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"PONG");

        client_sess.close().await;
        conn_sess.close().await;
        drop(conn_accept_tx);
        drop(client_accept_tx);
    }
}
