use std::{sync::Arc, time::Duration};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::prism::tunnel::{
    protocol::{self, ProxyStreamKind, RegisterRequest, RegisteredService},
    transport::{transport_by_name, TransportDialOptions},
};

#[derive(Debug, Clone)]
pub struct QuicClientOptions {
    pub server_name: String,
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub services: Vec<RegisteredService>,
    pub dial_timeout: Duration,
    pub quic: QuicClientOptions,
}

pub struct Client {
    opts: ClientOptions,
    local_map: Arc<std::collections::HashMap<String, RegisteredService>>,
}

impl Client {
    pub fn new(mut opts: ClientOptions) -> anyhow::Result<Self> {
        if opts.dial_timeout <= Duration::from_millis(0) {
            opts.dial_timeout = Duration::from_secs(5);
        }

        let mut map = std::collections::HashMap::new();
        let mut svcs = Vec::new();
        for s in opts.services.drain(..) {
            let Some(ns) = s.normalize() else { continue; };
            if ns.local_addr.trim().is_empty() {
                continue;
            }
            map.insert(ns.name.clone(), ns.clone());
            svcs.push(ns);
        }
        opts.services = svcs;

        Ok(Self {
            opts,
            local_map: Arc::new(map),
        })
    }

    pub async fn run(&self, mut shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        if self.opts.server_addr.trim().is_empty() {
            anyhow::bail!("tunnel: client server_addr is required");
        }

        let mut backoff = Duration::from_secs(1);
        loop {
            if *shutdown.borrow() {
                return Ok(());
            }

            match self.run_once(shutdown.clone()).await {
                Ok(()) => return Ok(()),
                Err(err) => {
                    tracing::warn!(
                        transport=%self.opts.transport,
                        server=%self.opts.server_addr,
                        err=%err,
                        backoff=%humantime::format_duration(backoff),
                        "tunnel: disconnected; retrying"
                    );
                }
            }

            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        return Ok(());
                    }
                }
                _ = tokio::time::sleep(backoff) => {}
            }

            backoff = (backoff * 2).min(Duration::from_secs(10));
        }
    }

    async fn run_once(&self, shutdown: tokio::sync::watch::Receiver<bool>) -> anyhow::Result<()> {
        let tr = transport_by_name(&self.opts.transport)?;

        let dial = async {
            tr.dial(
                &self.opts.server_addr,
                TransportDialOptions {
                    quic: crate::prism::tunnel::transport::QuicDialOptions {
                        server_name: self.opts.quic.server_name.clone(),
                        insecure_skip_verify: self.opts.quic.insecure_skip_verify,
                        next_protos: vec![],
                    },
                },
            )
            .await
        };

        let sess = tokio::time::timeout(self.opts.dial_timeout, dial).await??;

        // Register on first stream.
        let mut reg = sess.open_stream().await?;
        let req = RegisterRequest {
            token: self.opts.auth_token.clone(),
            services: self.opts.services.clone(),
        };
        protocol::write_register_request(&mut reg, &req).await?;
        reg.shutdown().await?;

        tracing::info!(
            transport=%tr.name(),
            server=%self.opts.server_addr,
            services=self.opts.services.len(),
            "tunnel: connected"
        );

        // Accept proxy streams.
        let mut shutdown = shutdown;
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        sess.close().await;
                        return Ok(());
                    }
                }
                st = sess.accept_stream() => {
                    let st = st?;
                    let map = self.local_map.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_stream(map, st).await {
                            tracing::debug!(err=%err, "tunnel: stream ended");
                        }
                    });
                }
            }
        }
    }
}

async fn handle_stream(
    local_map: Arc<std::collections::HashMap<String, RegisteredService>>,
    mut st: crate::prism::tunnel::transport::BoxedStream,
) -> anyhow::Result<()> {
    let (kind, svc) = protocol::read_proxy_stream_header(&mut st).await?;
    let meta = local_map.get(&svc).cloned();
    let Some(meta) = meta else {
        tracing::warn!(service=%svc, "tunnel: unknown service");
        return Ok(());
    };
    let local = meta.local_addr.trim().to_string();
    if local.is_empty() {
        return Ok(());
    }

    match kind {
        ProxyStreamKind::Tcp => {
            let mut up = tokio::net::TcpStream::connect(&local).await?;
            let mut st = st;
            let _ = tokio::io::copy_bidirectional(&mut st, &mut up).await;
        }
        ProxyStreamKind::Udp => {
            // Proxy framed datagrams over the tunnel stream <-> local UDP socket.
            let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
            sock.connect(&local).await?;

            let sock = Arc::new(sock);

            let (mut rd, mut wr) = tokio::io::split(st);

            // We cannot reuse AsyncRead/Write-based copying for UDP because datagram framing must be preserved.
            let sock_to_local = sock.clone();
            let t1 = tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = rd.read_u32().await?;
                    if n > protocol::MAX_DATAGRAM_BYTES {
                        break;
                    }
                    let n = n as usize;
                    if n > buf.len() {
                        buf.resize(n, 0);
                    }
                    rd.read_exact(&mut buf[..n]).await?;
                    let _ = sock_to_local.send(&buf[..n]).await?;
                }
                Ok::<(), anyhow::Error>(())
            });

            let sock_from_local = sock;
            let t2 = tokio::spawn(async move {
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    let n = sock_from_local.recv(&mut buf).await?;
                    let n32: u32 = n.try_into().unwrap_or(u32::MAX);
                    if n32 > protocol::MAX_DATAGRAM_BYTES {
                        continue;
                    }
                    wr.write_u32(n32).await?;
                    wr.write_all(&buf[..n]).await?;
                    wr.flush().await?;
                }
                #[allow(unreachable_code)]
                Ok::<(), anyhow::Error>(())
            });

            let _ = tokio::try_join!(t1, t2);
        }
    }

    Ok(())
}
