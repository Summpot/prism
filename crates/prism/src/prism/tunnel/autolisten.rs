use std::{
    collections::HashMap,
    net::SocketAddr,
    sync::Arc,
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    sync::Mutex,
};

use crate::prism::net;
use crate::prism::tunnel::{manager::Manager, protocol};

#[derive(Debug, Clone)]
pub struct AutoListenOptions {
    /// How long to keep per-peer UDP flows alive without activity.
    pub udp_flow_idle_timeout: Duration,
}

impl Default for AutoListenOptions {
    fn default() -> Self {
        Self {
            udp_flow_idle_timeout: Duration::from_secs(60),
        }
    }
}

#[derive(Debug, Clone)]
struct DesiredSvc {
    client_id: String,
    name: String,
    proto: String,
    addr: String,
}

struct RunningListener {
    desired: DesiredSvc,
    stop: tokio::sync::watch::Sender<bool>,
    task: tokio::task::JoinHandle<()>,
}

/// Server-side auto listener manager for tunnel-registered services.
///
/// When enabled, Prism opens listeners for services that specify `remote_addr`.
///
/// Keying model matches the design: later registrations with the same service name
/// do not override routing, but can still be exposed via port, so auto-listen is
/// keyed by `client_id/service`.
pub struct AutoListener {
    mgr: Arc<Manager>,
    opts: AutoListenOptions,
    running: Mutex<HashMap<String, RunningListener>>,
}

impl std::fmt::Debug for AutoListener {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutoListener").finish_non_exhaustive()
    }
}

impl AutoListener {
    pub fn new(mgr: Arc<Manager>, opts: AutoListenOptions) -> Self {
        Self {
            mgr,
            opts,
            running: Mutex::new(HashMap::new()),
        }
    }

    pub async fn run(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let mut sub = self.mgr.subscribe();

        // Initial pass.
        self.reconcile().await;

        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = sub.changed() => {
                    self.reconcile().await;
                }
            }
        }

        self.shutdown_all().await;
        Ok(())
    }

    pub async fn shutdown_all(&self) {
        let mut running = self.running.lock().await;
        for (_k, r) in running.drain() {
            let _ = r.stop.send(true);
            r.task.abort();
        }
    }

    pub async fn reconcile(&self) {
        let snaps = self.mgr.snapshot_services().await;

        let mut desired: HashMap<String, DesiredSvc> = HashMap::new();
        for s in snaps {
            let name = s.service.name.trim().to_string();
            if name.is_empty() {
                continue;
            }
            if s.service.route_only {
                continue;
            }
            let cid = s.client_id.trim().to_string();
            if cid.is_empty() {
                continue;
            }
            let mut proto = s.service.proto.trim().to_ascii_lowercase();
            if proto.is_empty() {
                proto = "tcp".into();
            }
            let remote = s.service.remote_addr.trim().to_string();
            if remote.is_empty() {
                continue;
            }
            let key = format!("{cid}/{name}");
            desired.insert(
                key,
                DesiredSvc {
                    client_id: cid,
                    name,
                    proto,
                    addr: remote,
                },
            );
        }

        let mut running = self.running.lock().await;

        // Stop removed or changed.
        let keys: Vec<String> = running.keys().cloned().collect();
        for key in keys {
            let Some(cur) = running.get(&key) else {
                continue;
            };
            let want = desired.get(&key);
            let should_keep = want.is_some_and(|w| {
                w.client_id == cur.desired.client_id
                    && w.name == cur.desired.name
                    && w.proto == cur.desired.proto
                    && w.addr == cur.desired.addr
            });

            if !should_keep {
                if let Some(old) = running.remove(&key) {
                    let _ = old.stop.send(true);
                    old.task.abort();
                    tracing::info!(key=%key, "tunnel: stopped auto-listen");
                }
            }
        }

        // Start new.
        for (key, svc) in desired {
            if running.contains_key(&key) {
                continue;
            }

            let (stop_tx, stop_rx) = tokio::sync::watch::channel(false);
            let mgr = self.mgr.clone();
            let opts = self.opts.clone();
            let svc2 = svc.clone();
            let task = tokio::spawn(async move {
                match svc2.proto.as_str() {
                    "tcp" => {
                        if let Err(err) = run_tcp_listener(mgr, svc2, stop_rx).await {
                            tracing::warn!(err=%err, "tunnel: auto-listen tcp stopped");
                        }
                    }
                    "udp" => {
                        if let Err(err) = run_udp_listener(mgr, svc2, opts, stop_rx).await {
                            tracing::warn!(err=%err, "tunnel: auto-listen udp stopped");
                        }
                    }
                    _ => {}
                }
            });

            tracing::info!(key=%key, proto=%svc.proto, addr=%svc.addr, "tunnel: auto listening for service");
            running.insert(
                key,
                RunningListener {
                    desired: svc,
                    stop: stop_tx,
                    task,
                },
            );
        }
    }

    #[cfg(test)]
    async fn running_len(&self) -> usize {
        self.running.lock().await.len()
    }
}

async fn run_tcp_listener(
    mgr: Arc<Manager>,
    svc: DesiredSvc,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_addr = net::normalize_bind_addr(&svc.addr);
    let ln = TcpListener::bind(bind_addr.as_ref())
        .await
        .with_context(|| format!("tunnel: auto-listen tcp bind {}", svc.addr))?;
    let local = ln.local_addr().ok();
    tracing::info!(service=%svc.name, cid=%svc.client_id, bind=%svc.addr, local=?local, "tunnel: auto-listen tcp ready");

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() {
                    break;
                }
            }
            res = ln.accept() => {
                let (mut c, peer) = res?;
                let mgr = mgr.clone();
                let cid = svc.client_id.clone();
                let name = svc.name.clone();
                tokio::spawn(async move {
                    if let Err(err) = handle_tcp_conn(mgr, &cid, &name, &mut c).await {
                        tracing::debug!(service=%name, cid=%cid, peer=%peer, err=%err, "tunnel: auto-listen tcp conn ended");
                    }
                });
            }
        }
    }

    Ok(())
}

async fn handle_tcp_conn(
    mgr: Arc<Manager>,
    client_id: &str,
    service: &str,
    c: &mut TcpStream,
) -> anyhow::Result<()> {
    let mut st = mgr
        .dial_service_tcp_from_client(client_id, service)
        .await
        .map_err(|_| anyhow::anyhow!("tunnel: service not found"))?;

    let _ = tokio::io::copy_bidirectional(c, &mut *st).await;
    let _ = c.shutdown().await;
    let _ = (&mut *st).shutdown().await;
    Ok(())
}

struct UdpFlow {
    wr: Mutex<tokio::io::WriteHalf<crate::prism::tunnel::transport::BoxedStream>>,
    task: tokio::task::JoinHandle<()>,
    last: Instant,
}

async fn run_udp_listener(
    mgr: Arc<Manager>,
    svc: DesiredSvc,
    opts: AutoListenOptions,
    mut stop: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_addr = net::normalize_bind_addr(&svc.addr);
    let sock = UdpSocket::bind(bind_addr.as_ref())
        .await
        .with_context(|| format!("tunnel: auto-listen udp bind {}", svc.addr))?;
    let local = sock.local_addr().ok();
    tracing::info!(service=%svc.name, cid=%svc.client_id, bind=%svc.addr, local=?local, "tunnel: auto-listen udp ready");

    let sock = Arc::new(sock);

    let mut flows: HashMap<SocketAddr, UdpFlow> = HashMap::new();
    let mut buf = vec![0u8; 64 * 1024];
    let mut tick = tokio::time::interval(Duration::from_secs(5));

    loop {
        tokio::select! {
            _ = stop.changed() => {
                if *stop.borrow() { break; }
            }
            _ = tick.tick() => {
                let now = Instant::now();
                let idle = opts.udp_flow_idle_timeout;
                if idle > Duration::from_millis(0) {
                    let dead: Vec<SocketAddr> = flows
                        .iter()
                        .filter_map(|(k, v)| if now.duration_since(v.last) > idle { Some(*k) } else { None })
                        .collect();
                    for k in dead {
                        if let Some(f) = flows.remove(&k) {
                            f.task.abort();
                        }
                    }
                }
            }
            res = sock.recv_from(&mut buf) => {
                let (n, peer) = res?;
                let payload = &buf[..n];

                if n > protocol::MAX_DATAGRAM_BYTES as usize {
                    continue;
                }

                if !flows.contains_key(&peer) {
                    let st = mgr
                        .dial_service_udp_from_client(&svc.client_id, &svc.name)
                        .await
                        .map_err(|_| anyhow::anyhow!("tunnel: service not found"))?;
                    let (mut rd, wr) = tokio::io::split(st);

                    let sock2 = sock.clone();
                    let name = svc.name.clone();
                    let cid = svc.client_id.clone();
                    let name_task = name.clone();
                    let cid_task = cid.clone();
                    let task = tokio::spawn(async move {
                        let mut dbuf = vec![0u8; 64 * 1024];
                        let res: anyhow::Result<()> = async {
                            loop {
                                let n = rd.read_u32().await?;
                                if n > protocol::MAX_DATAGRAM_BYTES {
                                    break;
                                }
                                let n = n as usize;
                                if n > dbuf.len() {
                                    dbuf.resize(n, 0);
                                }
                                rd.read_exact(&mut dbuf[..n]).await?;
                                let _ = sock2.send_to(&dbuf[..n], peer).await?;
                            }
                            Ok(())
                        }
                        .await;

                        if let Err(err) = res {
                            tracing::debug!(service=%name_task, cid=%cid_task, peer=%peer, err=%err, "tunnel: auto-listen udp flow ended");
                        }
                    });

                    flows.insert(
                        peer,
                        UdpFlow {
                            wr: Mutex::new(wr),
                            task,
                            last: Instant::now(),
                        },
                    );

                    tracing::debug!(service=%name, cid=%cid, peer=%peer, "tunnel: auto-listen udp flow created");
                }

                if let Some(flow) = flows.get_mut(&peer) {
                    flow.last = Instant::now();
                    let mut wr = flow.wr.lock().await;
                    wr.write_u32(n as u32).await?;
                    wr.write_all(payload).await?;
                    wr.flush().await?;
                }
            }
        }
    }

    for (_k, f) in flows.drain() {
        f.task.abort();
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeSession {
        remote: Option<SocketAddr>,
    }

    #[async_trait::async_trait]
    impl crate::prism::tunnel::transport::TransportSession for FakeSession {
        async fn open_stream(
            &self,
        ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
            anyhow::bail!("not implemented")
        }

        async fn accept_stream(
            &self,
        ) -> anyhow::Result<crate::prism::tunnel::transport::BoxedStream> {
            anyhow::bail!("not implemented")
        }

        async fn close(&self) {}

        fn remote_addr(&self) -> Option<SocketAddr> {
            self.remote
        }

        fn local_addr(&self) -> Option<SocketAddr> {
            None
        }
    }

    #[tokio::test]
    async fn reconcile_skips_route_only_even_with_remote_addr() {
        let mgr = Arc::new(Manager::new());
        let sess = Arc::new(FakeSession { remote: None });
        mgr.register_client(
            "c-1".into(),
            sess,
            vec![protocol::RegisteredService {
                name: "svc".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:25565".into(),
                route_only: true,
                remote_addr: "127.0.0.1:0".into(),
            }],
        )
        .await
        .unwrap();

        let a = AutoListener::new(mgr, AutoListenOptions::default());
        a.reconcile().await;
        assert_eq!(a.running_len().await, 0);
        a.shutdown_all().await;
    }

    #[tokio::test]
    async fn reconcile_starts_remote_listener() {
        let mgr = Arc::new(Manager::new());
        let sess = Arc::new(FakeSession { remote: None });
        mgr.register_client(
            "c-1".into(),
            sess,
            vec![protocol::RegisteredService {
                name: "svc".into(),
                proto: "tcp".into(),
                local_addr: "127.0.0.1:25565".into(),
                route_only: false,
                remote_addr: "127.0.0.1:0".into(),
            }],
        )
        .await
        .unwrap();

        let a = AutoListener::new(mgr, AutoListenOptions::default());
        a.reconcile().await;

        let deadline = tokio::time::Instant::now() + Duration::from_millis(500);
        loop {
            if a.running_len().await == 1 {
                break;
            }
            if tokio::time::Instant::now() > deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        assert_eq!(a.running_len().await, 1);
        a.shutdown_all().await;
        assert_eq!(a.running_len().await, 0);
    }
}
