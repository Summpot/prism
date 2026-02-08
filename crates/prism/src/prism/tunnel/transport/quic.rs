use std::{net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use pin_project_lite::pin_project;
use quinn::{ClientConfig, Connection, Endpoint, ServerConfig, TransportConfig};
use tokio::sync::mpsc;

use crate::prism::net;
use crate::prism::tunnel::transport::{
    BoxedStream, QuicDialOptions, QuicListenOptions, Transport, TransportDialOptions,
    TransportListenOptions, TransportListener, TransportSession, default_alpn,
};

pub struct QuicTransport;

impl QuicTransport {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Transport for QuicTransport {
    fn name(&self) -> &'static str {
        "quic"
    }

    async fn listen(
        &self,
        addr: &str,
        opts: TransportListenOptions,
    ) -> anyhow::Result<Box<dyn TransportListener>> {
        let bind_addr = net::normalize_bind_addr(addr);
        let addr: SocketAddr = bind_addr.parse()?;
        let QuicListenOptions {
            cert_file,
            key_file,
            next_protos,
        } = opts.quic;

        let next_protos = default_alpn(&next_protos);
        let (cert_chain, key) = quic_tls::load_or_generate_cert(cert_file, key_file)?;

        let mut transport_cfg = TransportConfig::default();
        transport_cfg.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
        transport_cfg.keep_alive_interval(Some(Duration::from_secs(20)));

        let server_crypto = quic_tls::server_crypto_config(cert_chain, key, next_protos)?;
        let mut server_cfg = ServerConfig::with_crypto(Arc::new(
            quinn::crypto::rustls::QuicServerConfig::try_from(server_crypto)?,
        ));
        server_cfg.transport_config(Arc::new(transport_cfg));

        let endpoint = Endpoint::server(server_cfg, addr)?;
        Ok(Box::new(QuicTransportListener { endpoint }))
    }

    async fn dial(
        &self,
        addr: &str,
        opts: TransportDialOptions,
    ) -> anyhow::Result<Arc<dyn TransportSession>> {
        let QuicDialOptions {
            server_name,
            insecure_skip_verify,
            next_protos,
        } = opts.quic;
        let next_protos = default_alpn(&next_protos);

        let mut transport_cfg = TransportConfig::default();
        transport_cfg.max_idle_timeout(Some(Duration::from_secs(60).try_into()?));
        transport_cfg.keep_alive_interval(Some(Duration::from_secs(20)));

        let client_crypto = quic_tls::client_crypto_config(insecure_skip_verify, next_protos)?;
        let mut client_cfg = ClientConfig::new(Arc::new(
            quinn::crypto::rustls::QuicClientConfig::try_from(client_crypto)?,
        ));
        client_cfg.transport_config(Arc::new(transport_cfg));

        let bind: SocketAddr = "0.0.0.0:0".parse().unwrap();
        let mut endpoint = Endpoint::client(bind)?;
        endpoint.set_default_client_config(client_cfg);

        let name = if server_name.trim().is_empty() {
            "localhost".to_string()
        } else {
            server_name
        };

        let remote = resolve_socket_addr(addr).await?;
        let connecting = endpoint.connect(remote, &name)?;
        let conn = connecting.await?;
        Ok(Arc::new(QuicSession::new(conn)))
    }
}

async fn resolve_socket_addr(addr: &str) -> anyhow::Result<SocketAddr> {
    if let Ok(sa) = addr.parse::<SocketAddr>() {
        return Ok(sa);
    }
    let mut it = tokio::net::lookup_host(addr).await?;
    it.next()
        .ok_or_else(|| anyhow::anyhow!("tunnel: could not resolve {addr:?}"))
}

pub struct QuicTransportListener {
    endpoint: Endpoint,
}

#[async_trait]
impl TransportListener for QuicTransportListener {
    async fn accept(&self) -> anyhow::Result<Arc<dyn TransportSession>> {
        let incoming = self.endpoint.accept();
        let connecting = incoming
            .await
            .ok_or_else(|| anyhow::anyhow!("tunnel: quic endpoint closed"))?;
        let conn = connecting.await?;
        Ok(Arc::new(QuicSession::new(conn)))
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.endpoint.local_addr().ok()
    }

    async fn close(&self) -> anyhow::Result<()> {
        self.endpoint.close(0u32.into(), b"");
        Ok(())
    }
}

struct QuicSession {
    conn: Connection,
    incoming: tokio::sync::Mutex<mpsc::Receiver<(quinn::SendStream, quinn::RecvStream)>>,
    task: tokio::task::JoinHandle<()>,
}

impl QuicSession {
    fn new(conn: Connection) -> Self {
        let (tx, rx) = mpsc::channel(64);
        let c = conn.clone();
        let task = tokio::spawn(async move {
            loop {
                match c.accept_bi().await {
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
            conn,
            incoming: tokio::sync::Mutex::new(rx),
            task,
        }
    }
}

#[async_trait]
impl TransportSession for QuicSession {
    async fn open_stream(&self) -> anyhow::Result<BoxedStream> {
        let (send, recv) = self.conn.open_bi().await?;
        Ok(Box::new(QuicBiStream { send, recv }))
    }

    async fn accept_stream(&self) -> anyhow::Result<BoxedStream> {
        let mut rx = self.incoming.lock().await;
        let (send, recv) = rx
            .recv()
            .await
            .ok_or_else(|| anyhow::anyhow!("tunnel: session closed"))?;
        Ok(Box::new(QuicBiStream { send, recv }))
    }

    async fn close(&self) {
        self.task.abort();
        self.conn.close(0u32.into(), b"");
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        Some(self.conn.remote_address())
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        // quinn doesn't expose local addr on Connection; get it from endpoint is possible.
        None
    }
}

pin_project! {
    struct QuicBiStream {
        #[pin]
        send: quinn::SendStream,
        #[pin]
        recv: quinn::RecvStream,
    }
}

impl tokio::io::AsyncRead for QuicBiStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().recv.poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for QuicBiStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        use std::task::Poll;
        match self.project().send.poll_write(cx, data) {
            Poll::Ready(Ok(n)) => Poll::Ready(Ok(n)),
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::task::Poll;
        match self.project().send.poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        use std::task::Poll;
        match self.project().send.poll_shutdown(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => {
                Poll::Ready(Err(std::io::Error::new(std::io::ErrorKind::Other, e)))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

mod quic_tls {
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
                    "tunnel: quic requires both cert_file and key_file (or neither to auto-generate)"
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
        let certs = rustls_pemfile::certs(&mut rd)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .map(|c| CertificateDer::from(c))
            .collect();
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
        next_protos: Vec<Vec<u8>>,
    ) -> anyhow::Result<rustls::ServerConfig> {
        let mut cfg = rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)?;
        cfg.alpn_protocols = next_protos;
        Ok(cfg)
    }

    pub fn client_crypto_config(
        insecure_skip_verify: bool,
        next_protos: Vec<Vec<u8>>,
    ) -> anyhow::Result<rustls::ClientConfig> {
        if insecure_skip_verify {
            let mut cfg = rustls::ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(SkipServerVerification::new())
                .with_no_client_auth();
            cfg.alpn_protocols = next_protos;
            return Ok(cfg);
        }

        let root = rustls::RootCertStore::empty();
        let mut cfg = rustls::ClientConfig::builder()
            .with_root_certificates(root)
            .with_no_client_auth();
        cfg.alpn_protocols = next_protos;
        Ok(cfg)
    }

    /// Dummy certificate verifier that treats any certificate as valid.
    ///
    /// NOTE: vulnerable to MITM. Intended for local dev / testing only.
    #[derive(Debug)]
    struct SkipServerVerification(Arc<rustls::crypto::CryptoProvider>);

    impl SkipServerVerification {
        fn new() -> Arc<Self> {
            Arc::new(Self(Arc::new(rustls::crypto::ring::default_provider())))
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
