use std::{net::SocketAddr, sync::Arc};

use async_trait::async_trait;
use futures_util::StreamExt;
use tokio::{net::TcpListener, net::TcpStream, sync::mpsc};

use crate::prism::tunnel::transport::{BoxedStream, Transport, TransportDialOptions, TransportListener, TransportListenOptions, TransportSession};

pub struct TcpTransport;

impl TcpTransport {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl Transport for TcpTransport {
    fn name(&self) -> &'static str {
        "tcp"
    }

    async fn listen(&self, addr: &str, _opts: TransportListenOptions) -> anyhow::Result<Box<dyn TransportListener>> {
        let ln = TcpListener::bind(addr).await?;
        Ok(Box::new(TcpTransportListener { ln }))
    }

    async fn dial(&self, addr: &str, _opts: TransportDialOptions) -> anyhow::Result<Arc<dyn TransportSession>> {
        let c = TcpStream::connect(addr).await?;
        Ok(Arc::new(YamuxSession::client(c)))
    }
}

pub struct TcpTransportListener {
    ln: TcpListener,
}

#[async_trait]
impl TransportListener for TcpTransportListener {
    async fn accept(&self) -> anyhow::Result<Arc<dyn TransportSession>> {
        let (c, _) = self.ln.accept().await?;
        Ok(Arc::new(YamuxSession::server(c)))
    }

    fn local_addr(&self) -> Option<SocketAddr> {
        self.ln.local_addr().ok()
    }

    async fn close(&self) -> anyhow::Result<()> {
        // TcpListener doesn't have async close; drop closes.
        Ok(())
    }
}

struct YamuxSession {
    control: tokio::sync::Mutex<tokio_yamux::Control>,
    incoming: tokio::sync::Mutex<mpsc::Receiver<tokio_yamux::StreamHandle>>,
    remote: Option<SocketAddr>,
    local: Option<SocketAddr>,
    task: tokio::task::JoinHandle<()>,
}

impl YamuxSession {
    fn server(c: TcpStream) -> Self {
        let remote = c.peer_addr().ok();
        let local = c.local_addr().ok();
        let session = tokio_yamux::Session::new_server(c, tokio_yamux::Config::default());
        Self::from_session(session, remote, local)
    }

    fn client(c: TcpStream) -> Self {
        let remote = c.peer_addr().ok();
        let local = c.local_addr().ok();
        let session = tokio_yamux::Session::new_client(c, tokio_yamux::Config::default());
        Self::from_session(session, remote, local)
    }

    fn from_session(
        mut session: tokio_yamux::Session<TcpStream>,
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
        let st = rx.recv().await.ok_or_else(|| anyhow::anyhow!("tunnel: session closed"))?;
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
