use std::sync::Arc;

use tokio::io::AsyncWriteExt;

use crate::prism::tunnel::{
    manager::Manager,
    protocol,
    transport::{TransportListenOptions, transport_by_name},
};

#[derive(Debug, Clone, Default)]
pub struct QuicServerOptions {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone, Default)]
pub struct WebSocketServerOptions {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone)]
pub struct ServerOptions {
    pub listen_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub quic: QuicServerOptions,
    pub websocket: WebSocketServerOptions,
    pub manager: Arc<Manager>,
    pub auth_manager: Option<Arc<crate::prism::auth::AuthManager>>,
    pub admin_addr: Option<std::net::SocketAddr>,
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
                    websocket: crate::prism::tunnel::transport::WebSocketListenOptions {
                        cert_file: self.opts.websocket.cert_file.clone(),
                        key_file: self.opts.websocket.key_file.clone(),
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
                    let auth_mgr = self.opts.auth_manager.clone();
                    let admin_addr = self.opts.admin_addr;
                    tokio::spawn(async move {
                        if let Err(err) = handle_session(mgr, sess, token, auth_mgr, admin_addr).await {
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
    auth_mgr: Option<Arc<crate::prism::auth::AuthManager>>,
    admin_addr: Option<std::net::SocketAddr>,
) -> anyhow::Result<()> {
    let remote = sess
        .remote_addr()
        .map(|a| a.to_string())
        .unwrap_or_default();

    // First stream must be register.
    let mut reg = sess.accept_stream().await?;
    let req = protocol::read_register_request(&mut reg).await?;

    let identity = if let Some(ref am) = auth_mgr {
        match am.verify_token(&req.token).await {
            Some(ident) => Some(ident),
            None => {
                if !auth_token.trim().is_empty() && req.token == auth_token {
                    Some(crate::prism::auth::AuthIdentity {
                        user_id: "legacy_admin".to_string(),
                        username: "Legacy Admin".to_string(),
                        role: crate::prism::auth::UserRole::Admin,
                        service_rules: vec!["*".to_string()],
                        is_admin: true,
                    })
                } else if auth_token.trim().is_empty() && !am.is_auth_enabled().await {
                    None
                } else {
                    tracing::warn!(client=%remote, "tunnel: bad token");
                    sess.close().await;
                    return Ok(());
                }
            }
        }
    } else if !auth_token.trim().is_empty() {
        if req.token != auth_token {
            tracing::warn!(client=%remote, "tunnel: bad token");
            sess.close().await;
            return Ok(());
        }
        None
    } else {
        None
    };

    if req.is_client() {
        let cid = mgr.next_client_id("cs");
        mgr.register_client_session(cid.clone(), sess.clone())
            .await?;
        tracing::info!(
            cid = %cid,
            client = %remote,
            user = ?identity.as_ref().map(|i| &i.username),
            "tunnel: client sidecar connected"
        );

        // Broadcast loop on reg stream sending active services whenever services change.
        let mgr_broadcast = mgr.clone();
        let auth_mgr_broadcast = auth_mgr.clone();
        let identity_broadcast = identity.clone();

        let broadcast_task = tokio::spawn(async move {
            let mut sub = mgr_broadcast.subscribe();
            let initial = mgr_broadcast.active_services().await;
            let initial = if let (Some(am), Some(id)) = (&auth_mgr_broadcast, &identity_broadcast) {
                am.filter_services(id, &initial)
            } else {
                initial
            };
            if protocol::write_service_catalog(&mut reg, &initial)
                .await
                .is_err()
            {
                return;
            }
            while sub.changed().await.is_ok() {
                let services = mgr_broadcast.active_services().await;
                let services =
                    if let (Some(am), Some(id)) = (&auth_mgr_broadcast, &identity_broadcast) {
                        am.filter_services(id, &services)
                    } else {
                        services
                    };
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
            let auth_mgr = auth_mgr.clone();
            let identity = identity.clone();
            let admin_addr = admin_addr;
            tokio::spawn(async move {
                if let Err(err) =
                    handle_client_stream(mgr, client_stream, auth_mgr, identity, admin_addr).await
                {
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
    auth_mgr: Option<Arc<crate::prism::auth::AuthManager>>,
    identity: Option<crate::prism::auth::AuthIdentity>,
    admin_addr: Option<std::net::SocketAddr>,
) -> anyhow::Result<()> {
    let (kind, service_name, flags) =
        protocol::read_proxy_stream_header_with_flags(&mut client_stream).await?;

    if service_name == protocol::ADMIN_SERVICE_NAME {
        let is_admin = if let (Some(_), Some(id)) = (&auth_mgr, &identity) {
            id.is_admin
        } else if let Some(id) = &identity {
            id.is_admin
        } else {
            // When auth_mgr is None and token matched, or no auth is enabled on server
            true
        };

        if !is_admin {
            tracing::warn!(
                user = ?identity.as_ref().map(|i| &i.username),
                "tunnel: unauthorized client stream blocked from accessing $admin"
            );
            return Ok(());
        }

        if let Some(addr) = admin_addr {
            match tokio::net::TcpStream::connect(addr).await {
                Ok(mut admin_conn) => {
                    let _ =
                        tokio::io::copy_bidirectional(&mut client_stream, &mut admin_conn).await;
                }
                Err(err) => {
                    tracing::warn!(addr = %addr, err = %err, "tunnel: failed to connect to local admin service");
                }
            }
        } else {
            tracing::warn!(
                "tunnel: internal admin stream requested but admin_addr is not configured"
            );
        }
        return Ok(());
    }

    if let (Some(am), Some(id)) = (&auth_mgr, &identity) {
        if !am.can_access_service(id, &service_name) {
            tracing::warn!(
                user = %id.username,
                service = %service_name,
                "tunnel: unauthorized client stream blocked by ACL"
            );
            return Ok(());
        }
    }

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
            let _ = handle_session(mgr_clone, sess_clone, "secret".into(), None, None).await;
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
            let _ = handle_session(mgr_clone, sess_clone, "".into(), None, None).await;
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
            let _ = handle_session(mgr_c, conn_sess_clone, "".into(), None, None).await;
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
            let _ = handle_session(mgr_c, conn_sess_clone, "".into(), None, None).await;
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
            let _ = handle_session(mgr_cs, client_sess_clone, "".into(), None, None).await;
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

    #[tokio::test]
    async fn server_filters_catalog_and_blocks_unauthorized_dial_by_acl() {
        use crate::prism::auth::{AuthConfig, AuthManager, UserRecord, UserRole};

        let auth = Arc::new(AuthManager::new(AuthConfig::default(), None));
        let user = UserRecord {
            id: "u_alice".into(),
            username: "alice".into(),
            display_name: None,
            avatar_url: None,
            role: UserRole::Member,
            service_rules: vec!["mc-*".into()],
            created_at_unix_ms: 100,
            last_login_unix_ms: 100,
        };
        auth.upsert_user(user).await.unwrap();
        let (alice_token, _) = auth
            .create_client_token("u_alice", "Alice PC", None)
            .await
            .unwrap();

        let mgr = Arc::new(Manager::new());

        // 1. Connector registers two services: mc-survival and secret-database
        let (conn_tx, conn_rx) = mpsc::channel(16);
        let (conn_open_tx, mut conn_open_rx) = mpsc::channel(16);
        let conn_sess = Arc::new(MockSession::new(conn_rx, Some(conn_open_tx)));
        let (mut conn_reg_c, conn_reg_s) = tokio::io::duplex(4096);
        conn_tx.send(Box::new(conn_reg_s)).await.unwrap();

        let conn_req = protocol::RegisterRequest {
            client_type: "connector".into(),
            token: "conn_secret".into(),
            services: vec![
                protocol::RegisteredService {
                    name: "mc-survival".into(),
                    proto: "tcp".into(),
                    local_addr: "127.0.0.1:25565".into(),
                    ..Default::default()
                },
                protocol::RegisteredService {
                    name: "secret-database".into(),
                    proto: "tcp".into(),
                    local_addr: "127.0.0.1:3306".into(),
                    ..Default::default()
                },
            ],
        };
        tokio::spawn(async move {
            protocol::write_register_request(&mut conn_reg_c, &conn_req)
                .await
                .unwrap();
        });

        let mgr_c = mgr.clone();
        let conn_sess_c = conn_sess.clone();
        let auth_c = auth.clone();
        tokio::spawn(async move {
            let _ =
                handle_session(mgr_c, conn_sess_c, "conn_secret".into(), Some(auth_c), None).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        assert_eq!(mgr.active_services().await.len(), 2);

        // 2. Alice connects as client with her token
        let (client_tx, client_rx) = mpsc::channel(16);
        let client_sess = Arc::new(MockSession::new(client_rx, None));
        let (mut client_reg_c, client_reg_s) = tokio::io::duplex(4096);
        client_tx.send(Box::new(client_reg_s)).await.unwrap();

        let client_req = protocol::RegisterRequest {
            client_type: "client".into(),
            token: alice_token,
            services: vec![],
        };

        let cat_handle = tokio::spawn(async move {
            protocol::write_register_request(&mut client_reg_c, &client_req)
                .await
                .unwrap();
            protocol::read_service_catalog(&mut client_reg_c)
                .await
                .unwrap()
        });

        let mgr_cs = mgr.clone();
        let client_sess_c = client_sess.clone();
        let auth_cs = auth.clone();
        tokio::spawn(async move {
            let _ = handle_session(mgr_cs, client_sess_c, "".into(), Some(auth_cs), None).await;
        });

        // 3. Verify that Alice ONLY sees mc-survival in catalog (secret-database filtered out)
        let catalog = cat_handle.await.unwrap();
        assert_eq!(catalog.len(), 1);
        assert_eq!(catalog[0].name, "mc-survival");

        // 4. Alice attempts to dial unauthorized "secret-database"
        let (mut dial_client, dial_server) = tokio::io::duplex(4096);
        client_tx.send(Box::new(dial_server)).await.unwrap();

        protocol::write_proxy_stream_header(
            &mut dial_client,
            protocol::ProxyStreamKind::Tcp,
            "secret-database",
        )
        .await
        .unwrap();
        dial_client.write_all(b"HACK").await.unwrap();

        // Connector should NOT receive any dial stream because server ACL blocked it!
        let dialed =
            tokio::time::timeout(std::time::Duration::from_millis(100), conn_open_rx.recv()).await;
        assert!(dialed.is_err()); // timed out = blocked!

        client_sess.close().await;
        conn_sess.close().await;
    }

    #[tokio::test]
    async fn server_admin_stream_relays_to_local_admin() {
        let mgr = Arc::new(Manager::new());

        // 1. Start a mock local admin listener
        let admin_listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let admin_addr = admin_listener.local_addr().unwrap();

        tokio::spawn(async move {
            if let Ok((mut stream, _)) = admin_listener.accept().await {
                let mut buf = [0u8; 1024];
                let n = stream.read(&mut buf).await.unwrap();
                assert!(String::from_utf8_lossy(&buf[..n]).contains("GET /health"));
                stream
                    .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\n{\"ok\":true}")
                    .await
                    .unwrap();
            }
        });

        // 2. Client connects with admin session
        let (client_accept_tx, client_accept_rx) = mpsc::channel(16);
        let client_sess = Arc::new(MockSession::new(client_accept_rx, None));
        let (mut client_reg_c, client_reg_s) = tokio::io::duplex(4096);
        client_accept_tx.send(Box::new(client_reg_s)).await.unwrap();

        let client_req = protocol::RegisterRequest {
            client_type: "client".into(),
            token: "admin_token".into(),
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
            let _ = handle_session(
                mgr_cs,
                client_sess_clone,
                "admin_token".into(),
                None,
                Some(admin_addr),
            )
            .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. Client opens $admin stream
        let (mut client_stream_c, client_stream_s) = tokio::io::duplex(4096);
        client_accept_tx
            .send(Box::new(client_stream_s))
            .await
            .unwrap();

        protocol::write_proxy_stream_header(
            &mut client_stream_c,
            protocol::ProxyStreamKind::Tcp,
            protocol::ADMIN_SERVICE_NAME,
        )
        .await
        .unwrap();

        // 4. Send HTTP request and read response
        client_stream_c
            .write_all(b"GET /health HTTP/1.1\r\nHost: 127.0.0.1\r\n\r\n")
            .await
            .unwrap();

        let mut resp = vec![0u8; 1024];
        let n = client_stream_c.read(&mut resp).await.unwrap();
        let resp_str = String::from_utf8_lossy(&resp[..n]);
        assert!(resp_str.contains("HTTP/1.1 200 OK"));
        assert!(resp_str.contains("{\"ok\":true}"));

        client_sess.close().await;
    }

    #[tokio::test]
    async fn server_admin_stream_blocked_for_non_admin() {
        use crate::prism::auth::{AuthConfig, AuthManager, UserRecord, UserRole};

        let auth = Arc::new(AuthManager::new(AuthConfig::default(), None));
        let user = UserRecord {
            id: "u_bob".into(),
            username: "bob".into(),
            display_name: None,
            avatar_url: None,
            role: UserRole::Member, // NOT admin!
            service_rules: vec!["*".into()],
            created_at_unix_ms: 100,
            last_login_unix_ms: 100,
        };
        auth.upsert_user(user).await.unwrap();
        let (bob_token, _) = auth
            .create_client_token("u_bob", "Bob PC", None)
            .await
            .unwrap();

        let mgr = Arc::new(Manager::new());

        let (client_accept_tx, client_accept_rx) = mpsc::channel(16);
        let client_sess = Arc::new(MockSession::new(client_accept_rx, None));
        let (mut client_reg_c, client_reg_s) = tokio::io::duplex(4096);
        client_accept_tx.send(Box::new(client_reg_s)).await.unwrap();

        let client_req = protocol::RegisterRequest {
            client_type: "client".into(),
            token: bob_token,
            services: vec![],
        };
        tokio::spawn(async move {
            protocol::write_register_request(&mut client_reg_c, &client_req)
                .await
                .unwrap();
        });

        let mgr_cs = mgr.clone();
        let client_sess_clone = client_sess.clone();
        let auth_clone = auth.clone();
        // admin_addr is provided, but bob is Member, so should be blocked
        let dummy_admin_addr = "127.0.0.1:59999".parse().unwrap();
        tokio::spawn(async move {
            let _ = handle_session(
                mgr_cs,
                client_sess_clone,
                "".into(),
                Some(auth_clone),
                Some(dummy_admin_addr),
            )
            .await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        let (mut client_stream_c, client_stream_s) = tokio::io::duplex(4096);
        client_accept_tx
            .send(Box::new(client_stream_s))
            .await
            .unwrap();

        protocol::write_proxy_stream_header(
            &mut client_stream_c,
            protocol::ProxyStreamKind::Tcp,
            protocol::ADMIN_SERVICE_NAME,
        )
        .await
        .unwrap();

        // Bob tries to send request, but stream is closed by server ACL
        client_stream_c
            .write_all(b"GET /health HTTP/1.1\r\n\r\n")
            .await
            .unwrap();
        let mut resp = [0u8; 128];
        let n = client_stream_c.read(&mut resp).await.unwrap();
        assert_eq!(n, 0); // Stream EOF because closed!

        client_sess.close().await;
    }
}
