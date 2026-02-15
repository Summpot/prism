use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;

/// A bidirectional async byte stream.
///
/// Rust trait objects can only have a single non-auto "principal" trait, so we
/// wrap `AsyncRead + AsyncWrite` into a single trait.
pub trait AsyncStream: tokio::io::AsyncRead + tokio::io::AsyncWrite {}
impl<T> AsyncStream for T where T: tokio::io::AsyncRead + tokio::io::AsyncWrite + ?Sized {}

pub type BoxedStream = Box<dyn AsyncStream + Unpin + Send>;

#[derive(Debug, Clone, Default)]
pub struct QuicListenOptions {
    pub cert_file: String,
    pub key_file: String,
    pub next_protos: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct QuicDialOptions {
    pub server_name: String,
    pub insecure_skip_verify: bool,
    pub next_protos: Vec<Vec<u8>>,
}

#[derive(Debug, Clone, Default)]
pub struct TransportListenOptions {
    pub quic: QuicListenOptions,
}

#[derive(Debug, Clone, Default)]
pub struct TransportDialOptions {
    pub quic: QuicDialOptions,
}

#[async_trait]
pub trait Transport: Send + Sync {
    fn name(&self) -> &'static str;
    async fn listen(
        &self,
        addr: &str,
        opts: TransportListenOptions,
    ) -> anyhow::Result<Box<dyn TransportListener>>;
    async fn dial(
        &self,
        addr: &str,
        opts: TransportDialOptions,
    ) -> anyhow::Result<Arc<dyn TransportSession>>;
}

#[async_trait]
pub trait TransportListener: Send + Sync {
    async fn accept(&self) -> anyhow::Result<Arc<dyn TransportSession>>;
    #[allow(dead_code)]
    fn local_addr(&self) -> Option<SocketAddr>;
    async fn close(&self) -> anyhow::Result<()>;
}

#[async_trait]
pub trait TransportSession: Send + Sync {
    async fn open_stream(&self) -> anyhow::Result<BoxedStream>;
    async fn accept_stream(&self) -> anyhow::Result<BoxedStream>;
    async fn close(&self);
    fn remote_addr(&self) -> Option<SocketAddr>;
    #[allow(dead_code)]
    fn local_addr(&self) -> Option<SocketAddr>;
}

pub fn parse_transport(name: &str) -> anyhow::Result<String> {
    let mut n = name.trim().to_ascii_lowercase();
    if n.is_empty() {
        n = "tcp".into();
    }
    match n.as_str() {
        "tcp" | "udp" | "quic" => Ok(n),
        _ => anyhow::bail!("tunnel: unknown transport {name:?} (expected tcp|udp|quic)"),
    }
}

pub fn default_alpn(next: &[Vec<u8>]) -> Vec<Vec<u8>> {
    if !next.is_empty() {
        return next.to_vec();
    }
    vec![b"prism-tunnel".to_vec()]
}

pub mod quic;
pub mod tcp;
pub mod udp;

pub fn transport_by_name(name: &str) -> anyhow::Result<Arc<dyn Transport>> {
    let n = parse_transport(name)?;
    match n.as_str() {
        "tcp" => Ok(Arc::new(tcp::TcpTransport::new())),
        "quic" => Ok(Arc::new(quic::QuicTransport::new())),
        "udp" => Ok(Arc::new(udp::UdpTransport::new())),
        _ => unreachable!(),
    }
}
