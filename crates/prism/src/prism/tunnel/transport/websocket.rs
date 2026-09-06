use std::{
    net::SocketAddr,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
};

use async_trait::async_trait;
use bytes::{Buf, Bytes};
use futures_util::{Sink, Stream, StreamExt};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    net::TcpListener,
    sync::mpsc,
};
use tokio_rustls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;

use crate::prism::net;
use crate::prism::tunnel::transport::{
    BoxedStream, Transport, TransportDialOptions, TransportListenOptions, TransportListener,
    TransportSession,
};

pin_project_lite::pin_project! {
    pub struct WsByteStream<S> {
        #[pin]
        inner: S,
        read_buf: Bytes,
    }
}

impl<S> WsByteStream<S> {
    pub fn new(inner: S) -> Self {
        Self {
            inner,
            read_buf: Bytes::new(),
        }
    }
}

impl<S> AsyncRead for WsByteStream<S>
where
    S: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        loop {
            if self.read_buf.has_remaining() {
                let to_read = std::cmp::min(buf.remaining(), self.read_buf.remaining());
                buf.put_slice(&self.read_buf[..to_read]);
                self.read_buf.advance(to_read);
                return Poll::Ready(Ok(()));
            }

            match Pin::new(&mut self.inner).poll_next(cx) {
                Poll::Ready(Some(Ok(msg))) => match msg {
                    Message::Binary(bin) => {
                        self.read_buf = bin;
                    }
                    Message::Text(txt) => {
                        self.read_buf = Bytes::copy_from_slice(txt.as_bytes());
                    }
                    Message::Ping(_) | Message::Pong(_) => {
                        // Tungstenite automatically responds to Ping frames when polling the stream.
                    }
                    Message::Close(_) => {
                        return Poll::Ready(Ok(()));
                    }
                    Message::Frame(_) => {}
                },
                Poll::Ready(Some(Err(e))) => {
                    return Poll::Ready(Err(std::io::Error::new(
                        std::io::ErrorKind::ConnectionReset,
                        e,
                    )));
                }
                Poll::Ready(None) => {
                    return Poll::Ready(Ok(()));
                }
                Poll::Pending => return Poll::Pending,
            }
        }
    }
}

impl<S> AsyncWrite for WsByteStream<S>
where
    S: Sink<Message, Error = tokio_tungstenite::tungstenite::Error> + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        data: &[u8],
    ) -> Poll<std::io::Result<usize>> {
        match Pin::new(&mut self.inner).poll_ready(cx) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => {
                return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, e)));
            }
            Poll::Pending => return Poll::Pending,
        }

        let msg = Message::Binary(Bytes::copy_from_slice(data));
        if let Err(e) = Pin::new(&mut self.inner).start_send(msg) {
            return Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::BrokenPipe, e)));
        }
        Poll::Ready(Ok(data.len()))
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner)
            .poll_flush(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<std::io::Result<()>> {
        Pin::new(&mut self.inner)
            .poll_close(cx)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::BrokenPipe, e))
    }
}

pub struct YamuxSession {
    control: tokio::sync::Mutex<tokio_yamux::Control>,
    incoming: tokio::sync::Mutex<mpsc::Receiver<tokio_yamux::StreamHandle>>,
    remote: Option<SocketAddr>,
    local: Option<SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}

impl YamuxSession {
    pub fn server(
        stream: BoxedStream,
        remote: Option<SocketAddr>,
        local: Option<SocketAddr>,
    ) -> Self {
        let session = tokio_yamux::Session::new_server(stream, tokio_yamux::Config::default());
        Self::from_session(session, remote, local)
    }

    pub fn client(
        stream: BoxedStream,
        remote: Option<SocketAddr>,
        local: Option<SocketAddr>,
    ) -> Self {
        let session = tokio_yamux::Session::new_client(stream, tokio_yamux::Config::default());
        Self::from_session(session, remote, local)
    }

    fn from_session(
        mut session: tokio_yamux::Session<BoxedStream>,
        remote: Option<SocketAddr>,
        local: Option<SocketAddr>,
    ) -> Self {
        let control = session.control();
        let (tx, rx) = mpsc::channel::<tokio_yamux::StreamHandle>(64);
        let task = tokio::spawn(async move {
            while let Some(next) = session.next().await {
                match next {
                    Ok(st) => {
                        if tx.send(st).await.is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });

        Self {
            control: tokio::sync::Mutex::new(control),
            incoming: tokio::sync::Mutex::new(rx),
            remote,
            local,
            task,
        }
    }
}

#[async_trait]
impl TransportSession for YamuxSession {
    async fn open_stream(&self) -> anyhow::Result<BoxedStream> {
        let mut ctrl = self.control.lock().await;
        let st = ctrl.open_stream().await?;
        Ok(Box::new(st))
    }

    async fn accept_stream(&self) -> anyhow::Result<BoxedStream> {
        let mut rx = self.incoming.lock().await;
        let st = rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("tunnel: session closed"))?;
        Ok(Box::new(st))
    }

    async fn close(&self) {
        self.task.abort();
        let mut ctrl = self.control.lock().await;
        ctrl.close().await;
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.local
    }
}

pub struct WsTransport {
    require_tls: bool,
}

impl WsTransport {
    pub fn new(require_tls: bool) -> Self {
        Self { require_tls }
    }
}

#[async_trait]
impl Transport for WsTransport {
    fn name(&self) -> &'static str {
        if self.require_tls { "wss" } else { "websocket" }
    }

    async fn listen(
        &self,
        addr: &str,
        opts: TransportListenOptions,
    ) -> anyhow::Result<Box<dyn TransportListener>> {
        let bind_addr = net::normalize_bind_addr(addr);
        let ln = TcpListener::bind(bind_addr.as_ref()).await?;
        let local = ln.local_addr().ok();

        let cert_file = opts.websocket.cert_file.trim().to_string();
        let key_file = opts.websocket.key_file.trim().to_string();

        let tls_acceptor = if self.require_tls || !cert_file.is_empty() || !key_file.is_empty() {
            let (certs, key) = ws_tls::load_or_generate_cert(cert_file, key_file)?;
            let server_config = ws_tls::server_crypto_config(certs, key)?;
            Some(TlsAcceptor::from(Arc::new(server_config)))
        } else {
            None
        };

        Ok(Box::new(WsTransportListener {
            ln,
            local,
            tls_acceptor,
        }))
    }

    async fn dial(
        &self,
        addr: &str,
        opts: TransportDialOptions,
    ) -> anyhow::Result<Arc<dyn TransportSession>> {
        let url = normalize_ws_url(addr, self.require_tls)?;
        let is_wss = url.starts_with("wss://");

        let stream: BoxedStream = if is_wss && opts.websocket.insecure_skip_verify {
            let client_config = ws_tls::client_crypto_config(true)?;
            let connector = tokio_tungstenite::Connector::Rustls(Arc::new(client_config));
            let (ws, _) = tokio_tungstenite::connect_async_tls_with_config(
                url.as_str(),
                None,
                false,
                Some(connector),
            )
            .await?;
            Box::new(WsByteStream::new(ws))
        } else {
            let (ws, _) = tokio_tungstenite::connect_async(url.as_str()).await?;
            Box::new(WsByteStream::new(ws))
        };

        let remote_addr = parse_socket_addr_from_url(&url);
        Ok(Arc::new(YamuxSession::client(stream, remote_addr, None)))
    }
}

pub struct WsTransportListener {
    ln: TcpListener,
    local: Option<SocketAddr>,
    tls_acceptor: Option<TlsAcceptor>,
}

#[async_trait]
impl TransportListener for WsTransportListener {
    async fn accept(&self) -> anyhow::Result<Arc<dyn TransportSession>> {
        loop {
            let (tcp_stream, peer) = self.ln.accept().await?;
            let local = self.local;

            if let Some(ref acceptor) = self.tls_acceptor {
                match acceptor.accept(tcp_stream).await {
                    Ok(tls_stream) => match tokio_tungstenite::accept_async(tls_stream).await {
                        Ok(ws) => {
                            let stream: BoxedStream = Box::new(WsByteStream::new(ws));
                            return Ok(Arc::new(YamuxSession::server(stream, Some(peer), local)));
                        }
                        Err(err) => {
                            tracing::debug!(err=%err, peer=%peer, "tunnel: websocket accept failed");
                            continue;
                        }
                    },
                    Err(err) => {
                        tracing::debug!(err=%err, peer=%peer, "tunnel: tls accept failed");
                        continue;
                    }
                }
            } else {
                match tokio_tungstenite::accept_async(tcp_stream).await {
                    Ok(ws) => {
                        let stream: BoxedStream = Box::new(WsByteStream::new(ws));
                        return Ok(Arc::new(YamuxSession::server(stream, Some(peer), local)));
                    }
                    Err(err) => {
                        tracing::debug!(err=%err, peer=%peer, "tunnel: websocket accept failed");
                        continue;
                    }
                }
            }
        }
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.local
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub fn normalize_ws_url(addr: &str, default_tls: bool) -> anyhow::Result<String> {
    let raw = addr.trim();
    if raw.is_empty() {
        anyhow::bail!("tunnel: empty websocket target address");
    }

    if raw.starts_with("ws://") || raw.starts_with("wss://") {
        return Ok(raw.to_string());
    }

    let scheme = if default_tls { "wss://" } else { "ws://" };
    let rest = raw;

    let target = if let Some(stripped) = rest.strip_prefix(":") {
        format!("127.0.0.1:{stripped}")
    } else {
        rest.to_string()
    };

    if !target.contains('/') {
        Ok(format!("{scheme}{target}/"))
    } else {
        Ok(format!("{scheme}{target}"))
    }
}

fn parse_socket_addr_from_url(url: &str) -> Option<SocketAddr> {
    let stripped = url
        .strip_prefix("ws://")
        .or_else(|| url.strip_prefix("wss://"))?;
    let host_port = stripped.split('/').next()?;
    host_port.parse::<SocketAddr>().ok()
}

mod ws_tls {
    use std::{fs, path::Path, sync::Arc};

    use rcgen::generate_simple_self_signed;
    use rustls::{
        client::danger::{ServerCertVerified, ServerCertVerifier},
        pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime},
    };

    pub fn load_or_generate_cert(
        cert_file: String,
        key_file: String,
    ) -> anyhow::Result<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
        let cert_file = cert_file.trim().to_string();
        let key_file = key_file.trim().to_string();

        if !cert_file.is_empty() || !key_file.is_empty() {
            if cert_file.is_empty() || key_file.is_empty() {
                anyhow::bail!(
                    "tunnel: websocket requires both cert_file and key_file (or neither to auto-generate)"
                );
            }

            let certs = load_certs(Path::new(&cert_file))?;
            let key = load_key(Path::new(&key_file))?;
            return Ok((certs, key));
        }

        let rcgen::CertifiedKey { cert, signing_key } =
            generate_simple_self_signed(["localhost".to_string()])?;
        let cert_der = cert.der().clone();
        let key_der = PrivateKeyDer::from(PrivatePkcs8KeyDer::from(signing_key.serialize_der()));
        Ok((vec![cert_der], key_der))
    }

    fn load_certs(path: &Path) -> anyhow::Result<Vec<CertificateDer<'static>>> {
        let data = fs::read(path)?;
        let mut rd = std::io::Cursor::new(&data);
        let certs = rustls_pemfile::certs(&mut rd).collect::<Result<Vec<_>, _>>()?;
        Ok(certs)
    }

    fn load_key(path: &Path) -> anyhow::Result<PrivateKeyDer<'static>> {
        let data = fs::read(path)?;
        let mut rd = std::io::Cursor::new(&data);
        let key = rustls_pemfile::private_key(&mut rd)?;
        let Some(k) = key else {
            anyhow::bail!("tunnel: no private key found in {}", path.display());
        };
        Ok(k)
    }

    pub fn server_crypto_config(
        certs: Vec<CertificateDer<'static>>,
        key: PrivateKeyDer<'static>,
    ) -> anyhow::Result<rustls::ServerConfig> {
        let mut cfg = rustls::ServerConfig::builder_with_provider(crypto_provider())
            .with_safe_default_protocol_versions()?
            .with_no_client_auth()
            .with_single_cert(certs, key)?;
        cfg.alpn_protocols = vec![b"http/1.1".to_vec()];
        Ok(cfg)
    }

    pub fn client_crypto_config(
        insecure_skip_verify: bool,
    ) -> anyhow::Result<rustls::ClientConfig> {
        if insecure_skip_verify {
            let cfg = rustls::ClientConfig::builder_with_provider(crypto_provider())
                .with_safe_default_protocol_versions()?
                .dangerous()
                .with_custom_certificate_verifier(SkipServerVerification::new())
                .with_no_client_auth();
            return Ok(cfg);
        }

        let root = rustls::RootCertStore::empty();
        let cfg = rustls::ClientConfig::builder_with_provider(crypto_provider())
            .with_safe_default_protocol_versions()?
            .with_root_certificates(root)
            .with_no_client_auth();
        Ok(cfg)
    }

    fn crypto_provider() -> Arc<rustls::crypto::CryptoProvider> {
        if let Some(provider) = rustls::crypto::CryptoProvider::get_default() {
            Arc::clone(provider)
        } else {
            Arc::new(rustls::crypto::ring::default_provider())
        }
    }

    #[derive(Debug)]
    pub struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

    impl SkipServerVerification {
        pub fn new() -> Arc<Self> {
            Arc::new(Self(crypto_provider()))
        }
    }

    impl ServerCertVerifier for SkipServerVerification {
        fn verify_server_cert(
            &self,
            _end_entity: &CertificateDer<'_>,
            _intermediates: &[CertificateDer<'_>],
            _server_name: &ServerName<'_>,
            _ocsp_response: &[u8],
            _now: UnixTime,
        ) -> Result<ServerCertVerified, rustls::Error> {
            Ok(ServerCertVerified::assertion())
        }

        fn verify_tls12_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls12_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn verify_tls13_signature(
            &self,
            message: &[u8],
            cert: &CertificateDer<'_>,
            dss: &rustls::DigitallySignedStruct,
        ) -> Result<rustls::client::danger::HandshakeSignatureValid, rustls::Error> {
            rustls::crypto::verify_tls13_signature(
                message,
                cert,
                dss,
                &self.0.signature_verification_algorithms,
            )
        }

        fn supported_verify_schemes(&self) -> Vec<rustls::SignatureScheme> {
            self.0.signature_verification_algorithms.supported_schemes()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_normalize_ws_url() {
        assert_eq!(
            normalize_ws_url("127.0.0.1:7000", false).unwrap(),
            "ws://127.0.0.1:7000/"
        );
        assert_eq!(
            normalize_ws_url(":7000", false).unwrap(),
            "ws://127.0.0.1:7000/"
        );
        assert_eq!(
            normalize_ws_url("127.0.0.1:7000", true).unwrap(),
            "wss://127.0.0.1:7000/"
        );
        assert_eq!(
            normalize_ws_url("ws://example.com/custom", true).unwrap(),
            "ws://example.com/custom"
        );
        assert_eq!(
            normalize_ws_url("wss://example.com/path", false).unwrap(),
            "wss://example.com/path"
        );
        assert_eq!(
            normalize_ws_url("example.com:7000/tunnel", false).unwrap(),
            "ws://example.com:7000/tunnel"
        );
    }

    #[tokio::test]
    async fn test_ws_transport_listen_dial_roundtrip() -> anyhow::Result<()> {
        let transport = WsTransport::new(false);
        let ln = transport
            .listen("127.0.0.1:0", TransportListenOptions::default())
            .await?;
        let addr = ln.local_addr().unwrap();

        let srv_task = tokio::spawn(async move {
            let sess = ln.accept().await.unwrap();
            let mut st = sess.accept_stream().await.unwrap();
            let mut buf = [0u8; 5];
            st.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"hello");
            st.write_all(b"world").await.unwrap();
            st.flush().await.unwrap();
        });

        let client_sess = transport
            .dial(&addr.to_string(), TransportDialOptions::default())
            .await?;
        let mut st = client_sess.open_stream().await?;
        st.write_all(b"hello").await?;
        st.flush().await?;

        let mut buf = [0u8; 5];
        st.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"world");

        srv_task.await?;
        client_sess.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_ws_transport_wss_roundtrip() -> anyhow::Result<()> {
        let transport = WsTransport::new(true);
        let ln = transport
            .listen("127.0.0.1:0", TransportListenOptions::default())
            .await?;
        let addr = ln.local_addr().unwrap();

        let srv_task = tokio::spawn(async move {
            let sess = ln.accept().await.unwrap();
            let mut st = sess.accept_stream().await.unwrap();
            let mut buf = [0u8; 4];
            st.read_exact(&mut buf).await.unwrap();
            assert_eq!(&buf, b"ping");
            st.write_all(b"pong").await.unwrap();
            st.flush().await.unwrap();
        });

        let mut dial_opts = TransportDialOptions::default();
        dial_opts.websocket.insecure_skip_verify = true;

        let client_sess = transport.dial(&addr.to_string(), dial_opts).await?;
        let mut st = client_sess.open_stream().await?;
        st.write_all(b"ping").await?;
        st.flush().await?;

        let mut buf = [0u8; 4];
        st.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"pong");

        srv_task.await?;
        client_sess.close().await;
        Ok(())
    }

    #[tokio::test]
    async fn test_tunnel_server_and_connector_e2e_over_websocket() -> anyhow::Result<()> {
        use crate::prism::tunnel::connector::{Connector, ConnectorOptions};
        use crate::prism::tunnel::manager::Manager;
        use crate::prism::tunnel::protocol::RegisteredService;
        use crate::prism::tunnel::server::{Server, ServerOptions};

        // 1. Echo backend TCP server
        let backend = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let backend_addr = backend.local_addr()?.to_string();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = backend.accept().await {
                tokio::spawn(async move {
                    let (mut rd, mut wr) = socket.split();
                    let _ = tokio::io::copy(&mut rd, &mut wr).await;
                });
            }
        });

        // 2. Tunnel server listening on websocket
        let srv_ln = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
        let srv_addr = srv_ln.local_addr()?.to_string();
        drop(srv_ln);

        let mgr = Arc::new(Manager::new());
        let server = Server::new(ServerOptions {
            listen_addr: srv_addr.clone(),
            transport: "websocket".into(),
            auth_token: "ws-secret".into(),
            quic: Default::default(),
            websocket: Default::default(),
            manager: mgr.clone(),
            auth_manager: None,
            admin_addr: None,
        })?;

        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let srv_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = server.listen_and_serve(srv_shutdown).await;
        });

        tokio::time::sleep(std::time::Duration::from_millis(50)).await;

        // 3. Tunnel connector connecting via websocket
        let connector = Connector::new(ConnectorOptions {
            server_addr: format!("ws://{srv_addr}/ws"),
            transport: "websocket".into(),
            auth_token: "ws-secret".into(),
            services: vec![RegisteredService {
                name: "echo-ws".into(),
                proto: "tcp".into(),
                local_addr: backend_addr,
                route_only: false,
                remote_addr: "".into(),
                masquerade_host: "".into(),
                middleware: None,
                traffic_optimizer: None,
            }],
            dial_timeout: std::time::Duration::from_secs(3),
            quic: Default::default(),
            websocket: Default::default(),
            middleware_dir: None,
            traffic: None,
        })?;

        let conn_shutdown = shutdown_rx.clone();
        tokio::spawn(async move {
            let _ = connector.run(conn_shutdown).await;
        });

        // Wait for registration
        let mut registered = false;
        for _ in 0..50 {
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            if mgr.has_service("echo-ws").await {
                registered = true;
                break;
            }
        }
        assert!(
            registered,
            "service echo-ws should be registered in tunnel manager"
        );

        // 4. Dial service from tunnel manager and test bidirectional data transfer
        let mut stream = mgr.dial_service_tcp("echo-ws").await?;
        stream.write_all(b"ping-via-websocket").await?;
        stream.flush().await?;

        let mut buf = [0u8; 18];
        stream.read_exact(&mut buf).await?;
        assert_eq!(&buf, b"ping-via-websocket");

        let _ = shutdown_tx.send(true);
        Ok(())
    }
}
