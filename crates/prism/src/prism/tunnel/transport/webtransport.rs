use std::{net::SocketAddr, sync::Arc, time::Duration};

use async_trait::async_trait;
use pin_project_lite::pin_project;
use wtransport::{ClientConfig, Endpoint, Identity, ServerConfig};

use crate::prism::net;
use crate::prism::tunnel::transport::{
    BoxedStream, Transport, TransportDialOptions, TransportListenOptions,
    TransportListener, TransportSession, WebTransportDialOptions, WebTransportListenOptions,
};

pub struct WebTransportTransport;

impl WebTransportTransport {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Transport for WebTransportTransport {
    fn name(&self) -> &'static str {
        "webtransport"
    }

    async fn listen(
        &self,
        addr: &str,
        opts: TransportListenOptions,
    ) -> anyhow::Result<Box<dyn TransportListener>> {
        let bind_addr = net::normalize_bind_addr(addr);
        let addr: SocketAddr = bind_addr.parse()?;
        let WebTransportListenOptions { cert_file, key_file } = opts.webtransport;

        let identity = load_or_generate_identity(&cert_file, &key_file).await?;

        let config = ServerConfig::builder()
            .with_bind_address(addr)
            .with_identity(identity)
            .keep_alive_interval(Some(Duration::from_secs(20)))
            .max_idle_timeout(Some(Duration::from_secs(60)))?
            .build();

        let endpoint = Endpoint::server(config)?;
        Ok(Box::new(WebTransportListenerImpl { endpoint }))
    }

    async fn dial(
        &self,
        addr: &str,
        opts: TransportDialOptions,
    ) -> anyhow::Result<Arc<dyn TransportSession>> {
        let WebTransportDialOptions {
            server_name: _,
            insecure_skip_verify,
        } = opts.webtransport;

        let builder = ClientConfig::builder().with_bind_default();
        let builder = if insecure_skip_verify {
            builder.with_no_cert_validation()
        } else {
            builder.with_native_certs()
        };

        let config = builder
            .keep_alive_interval(Some(Duration::from_secs(20)))
            .max_idle_timeout(Some(Duration::from_secs(60)))?
            .build();

        let endpoint = Endpoint::client(config)?;

        let connect_url = if addr.starts_with("https://") || addr.starts_with("http://") {
            addr.to_string()
        } else {
            format!("https://{addr}")
        };

        let conn = endpoint.connect(&connect_url).await?;
        let remote_addr = conn.remote_address();

        Ok(Arc::new(WebTransportSessionImpl {
            conn,
            remote_addr: Some(remote_addr),
            local_addr: endpoint.local_addr().ok(),
        }))
    }
}

pub struct WebTransportListenerImpl {
    endpoint: Endpoint<wtransport::endpoint::endpoint_side::Server>,
}

#[async_trait]
impl TransportListener for WebTransportListenerImpl {
    async fn accept(&self) -> anyhow::Result<Arc<dyn TransportSession>> {
        let incoming_session = self.endpoint.accept().await;
        let incoming_request = incoming_session.await?;
        let conn = incoming_request.accept().await?;
        let remote_addr = conn.remote_address();

        Ok(Arc::new(WebTransportSessionImpl {
            conn,
            remote_addr: Some(remote_addr),
            local_addr: self.endpoint.local_addr().ok(),
        }))
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.endpoint.local_addr().ok()
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

pub struct WebTransportSessionImpl {
    conn: wtransport::Connection,
    remote_addr: Option<SocketAddr>,
    local_addr: Option<SocketAddr>,
}

#[async_trait]
impl TransportSession for WebTransportSessionImpl {
    async fn open_stream(&self) -> anyhow::Result<BoxedStream> {
        let (send, recv) = self.conn.open_bi().await?.await?;
        Ok(Box::new(WebTransportBiStream { send, recv }))
    }

    async fn accept_stream(&self) -> anyhow::Result<BoxedStream> {
        let (send, recv) = self.conn.accept_bi().await?;
        Ok(Box::new(WebTransportBiStream { send, recv }))
    }

    async fn close(&self) {
        self.conn.close(wtransport::VarInt::from_u32(0), b"close");
    }

    fn remote_addr(&self) -> Option<SocketAddr> {
        self.remote_addr
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.local_addr
    }
}

pin_project! {
    pub struct WebTransportBiStream {
        #[pin]
        send: wtransport::SendStream,
        #[pin]
        recv: wtransport::RecvStream,
    }
}

impl tokio::io::AsyncRead for WebTransportBiStream {
    fn poll_read(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().recv.poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for WebTransportBiStream {
    fn poll_write(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        data: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        self.project().send.poll_write(cx, data)
    }

    fn poll_flush(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().send.poll_flush(cx)
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        self.project().send.poll_shutdown(cx)
    }
}

async fn load_or_generate_identity(
    cert_file: &str,
    key_file: &str,
) -> anyhow::Result<Identity> {
    let cert_file = cert_file.trim();
    let key_file = key_file.trim();

    if !cert_file.is_empty() && !key_file.is_empty() {
        let identity = Identity::load_pemfiles(cert_file, key_file).await?;
        return Ok(identity);
    }

    // Auto-generate self-signed cert for localhost / dev
    let identity = Identity::self_signed(["localhost", "127.0.0.1"])?;
    Ok(identity)
}
