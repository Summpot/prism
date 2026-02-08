use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::sync::{mpsc, Mutex};
use tokio_kcp::{KcpConfig, KcpListener, KcpStream};

use crate::prism::tunnel::transport::{
    BoxedStream, Transport, TransportDialOptions, TransportListener, TransportListenOptions, TransportSession,
};

/// UDP transport implemented as KCP (reliable UDP) + yamux multiplexing.
///
/// This matches the design intent of "udp" transport being KCP-based.
pub struct UdpTransport {
    kcp: KcpConfig,
}

impl UdpTransport {
    pub fn new() -> Self {
        Self {
            kcp: KcpConfig::default(),
        }
    }
}

#[async_trait]
impl Transport for UdpTransport {
    fn name(&self) -> &'static str {
        "udp"
    }

    async fn listen(&self, addr: &str, _opts: TransportListenOptions) -> anyhow::Result<Box<dyn TransportListener>> {
        let bind_addr: SocketAddr = addr.parse()?;
        let ln = KcpListener::bind(self.kcp.clone(), bind_addr).await?;
        let local = ln.local_addr().ok();
        Ok(Box::new(UdpTransportListener {
            ln: Mutex::new(ln),
            local,
        }))
    }

    async fn dial(&self, addr: &str, _opts: TransportDialOptions) -> anyhow::Result<Arc<dyn TransportSession>> {
        let remote = resolve_socket_addr(addr).await?;
        let c = KcpStream::connect(&self.kcp, remote).await?;
        Ok(Arc::new(YamuxSession::client(c, Some(remote))))
    }
}

pub struct UdpTransportListener {
    ln: Mutex<KcpListener>,
    local: Option<SocketAddr>,
}

#[async_trait]
impl TransportListener for UdpTransportListener {
    async fn accept(&self) -> anyhow::Result<Arc<dyn TransportSession>> {
        let mut ln = self.ln.lock().await;
        let (c, peer) = ln.accept().await?;
        let local = self.local;
        Ok(Arc::new(YamuxSession::server(c, Some(peer), local)))
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.local
    }

    async fn close(&self) -> anyhow::Result<()> {
        Ok(())
    }
}

struct YamuxSession {
    control: Mutex<tokio_yamux::Control>,
    incoming: Mutex<mpsc::Receiver<tokio_yamux::StreamHandle>>,
    remote: Option<SocketAddr>,
    local: Option<SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}

impl YamuxSession {
    fn server(c: KcpStream, remote: Option<SocketAddr>, local: Option<SocketAddr>) -> Self {
        let session = tokio_yamux::Session::new_server(c, tokio_yamux::Config::default());
        Self::from_session(session, remote, local)
    }

    fn client(c: KcpStream, remote: Option<SocketAddr>) -> Self {
        let session = tokio_yamux::Session::new_client(c, tokio_yamux::Config::default());
        Self::from_session(session, remote, None)
    }

    fn from_session(
        mut session: tokio_yamux::Session<KcpStream>,
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
            control: Mutex::new(control),
            incoming: Mutex::new(rx),
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

async fn resolve_socket_addr(addr: &str) -> anyhow::Result<SocketAddr> {
    if let Ok(sa) = addr.parse::<SocketAddr>() {
        return Ok(sa);
    }
    let mut it = tokio::net::lookup_host(addr).await?;
    it.next()
        .ok_or_else(|| anyhow::anyhow!("tunnel: could not resolve {addr:?}"))
}
