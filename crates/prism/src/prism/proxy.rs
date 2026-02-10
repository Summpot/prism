use std::{
    collections::HashMap,
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time,
};

use dashmap::DashMap;

use crate::prism::{net, protocol, router, telemetry, tunnel};

#[derive(Debug, Clone, Hash, PartialEq, Eq)]
struct StatusCacheKey {
    upstream: String,
    protocol_version: i32,
}

#[derive(Debug, Clone)]
struct StatusCacheItem {
    expires_at: Instant,
    data: Arc<Vec<u8>>,
}

#[derive(Debug)]
struct InFlight {
    done: AtomicBool,
    notify: tokio::sync::Notify,
    // Ok(data) is cached; Err is not cached, but is shared with concurrent waiters.
    result: tokio::sync::Mutex<Option<Result<Arc<Vec<u8>>, String>>>,
}

impl InFlight {
    fn new() -> Self {
        Self {
            done: AtomicBool::new(false),
            notify: tokio::sync::Notify::new(),
            result: tokio::sync::Mutex::new(None),
        }
    }

    fn is_done(&self) -> bool {
        self.done.load(Ordering::Acquire)
    }

    fn finish(&self) {
        self.done.store(true, Ordering::Release);
        self.notify.notify_waiters();
    }
}

/// Caches raw Minecraft Status response packets (length-prefixed frames).
///
/// Entries are stored per-upstream and protocol version with a per-route TTL.
/// Failed loads are not cached.
///
/// This cache is optimized for correctness and simplicity; it performs lazy expiration.
#[derive(Debug)]
struct StatusCache {
    items: tokio::sync::Mutex<HashMap<StatusCacheKey, StatusCacheItem>>,
    inflight: tokio::sync::Mutex<HashMap<StatusCacheKey, Arc<InFlight>>>,
}

impl StatusCache {
    fn new() -> Self {
        Self {
            items: tokio::sync::Mutex::new(HashMap::new()),
            inflight: tokio::sync::Mutex::new(HashMap::new()),
        }
    }

    async fn get(&self, key: &StatusCacheKey) -> Option<Arc<Vec<u8>>> {
        let mut items = self.items.lock().await;
        let it = items.get(key)?.clone();
        if Instant::now() >= it.expires_at {
            items.remove(key);
            return None;
        }
        if it.data.is_empty() {
            items.remove(key);
            return None;
        }
        Some(it.data)
    }

    async fn set(&self, key: StatusCacheKey, data: Arc<Vec<u8>>, ttl: Duration) {
        if ttl <= Duration::from_millis(0) {
            return;
        }
        if data.is_empty() {
            return;
        }
        let exp = Instant::now() + ttl;
        let mut items = self.items.lock().await;
        items.insert(
            key,
            StatusCacheItem {
                expires_at: exp,
                data,
            },
        );
    }

    async fn get_or_load<F, Fut>(
        &self,
        key: StatusCacheKey,
        ttl: Duration,
        load: F,
    ) -> anyhow::Result<Arc<Vec<u8>>>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<Vec<u8>>> + Send,
    {
        if ttl <= Duration::from_millis(0) {
            return Ok(Arc::new(load().await?));
        }
        if let Some(v) = self.get(&key).await {
            return Ok(v);
        }

        let (flight, created) = {
            let mut inflight = self.inflight.lock().await;
            if let Some(existing) = inflight.get(&key) {
                (existing.clone(), false)
            } else {
                let f = Arc::new(InFlight::new());
                inflight.insert(key.clone(), f.clone());
                (f, true)
            }
        };

        if !created {
            if !flight.is_done() {
                flight.notify.notified().await;
            }
            let r = flight
                .result
                .lock()
                .await
                .clone()
                .unwrap_or_else(|| Err("status cache: inflight missing result".into()));
            return match r {
                Ok(v) => Ok(v),
                Err(e) => Err(anyhow::anyhow!(e)),
            };
        }

        // We are the loader.
        let out = match load().await {
            Ok(v) => {
                let data = Arc::new(v);
                self.set(key.clone(), data.clone(), ttl).await;
                Ok(data)
            }
            Err(err) => Err(err),
        };

        // Publish result to waiters.
        {
            let mut slot = flight.result.lock().await;
            *slot = Some(out.as_ref().map(|d| d.clone()).map_err(|e| e.to_string()));
        }
        flight.finish();
        self.inflight.lock().await.remove(&key);

        out
    }
}

fn default_status_cache() -> &'static StatusCache {
    static CACHE: OnceLock<StatusCache> = OnceLock::new();
    CACHE.get_or_init(StatusCache::new)
}

struct ActiveConnGuard;

impl ActiveConnGuard {
    fn new() -> Self {
        metrics::counter!("prism_connections_total").increment(1);
        metrics::gauge!("prism_active_connections").increment(1.0);
        Self
    }
}

impl Drop for ActiveConnGuard {
    fn drop(&mut self) {
        metrics::gauge!("prism_active_connections").decrement(1.0);
    }
}

#[derive(Clone)]
pub enum TcpHandler {
    Routing(Arc<TcpRoutingHandlerOptions>),
    Forward(Arc<TcpForwardHandlerOptions>),
}

impl TcpHandler {
    pub fn routing(opts: TcpRoutingHandlerOptions) -> Self {
        Self::Routing(Arc::new(opts))
    }

    pub fn forward(opts: TcpForwardHandlerOptions) -> Self {
        Self::Forward(Arc::new(opts))
    }

    async fn handle(&self, conn: TcpStream) {
        match self {
            TcpHandler::Routing(opts) => handle_routing(conn, opts.clone()).await,
            TcpHandler::Forward(opts) => handle_forward(conn, opts.clone()).await,
        }
    }
}

pub struct TcpRoutingHandlerOptions {
    pub router: Arc<router::Router>,
    pub sessions: telemetry::SharedSessions,

    pub tunnel_manager: Option<Arc<tunnel::manager::Manager>>,

    pub runtime: Arc<tokio::sync::RwLock<TcpRuntimeConfig>>,
}

pub struct TcpForwardHandlerOptions {
    pub upstream: String,
    pub sessions: telemetry::SharedSessions,

    pub tunnel_manager: Option<Arc<tunnel::manager::Manager>>,

    pub runtime: Arc<tokio::sync::RwLock<TcpRuntimeConfig>>,
}

#[derive(Debug, Clone)]
pub struct TcpRuntimeConfig {
    pub max_header_bytes: usize,
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
    pub upstream_dial_timeout: Duration,
    pub buffer_size: usize,
    pub proxy_protocol_v2: bool,
}

pub async fn serve_tcp(listen_addr: &str, handler: TcpHandler) -> anyhow::Result<()> {
    // Backwards-compatible entrypoint: run until process shutdown.
    let (tx, rx) = tokio::sync::watch::channel(false);
    // Keep sender alive for the lifetime of the listener.
    let _tx = tx;
    serve_tcp_with_shutdown(listen_addr, handler, rx).await
}

pub async fn serve_tcp_with_shutdown(
    listen_addr: &str,
    handler: TcpHandler,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_addr = net::normalize_bind_addr(listen_addr);
    let ln = TcpListener::bind(bind_addr.as_ref())
        .await
        .with_context(|| format!("bind tcp {listen_addr}"))?;

    tracing::info!(listen_addr = %listen_addr, "tcp: listening");

    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            res = ln.accept() => {
                let (conn, peer) = res?;
                let h = handler.clone();

                tokio::spawn(async move {
                    if tracing::enabled!(tracing::Level::DEBUG) {
                        tracing::debug!(client = %peer, "tcp: accepted");
                    }
                    h.handle(conn).await;
                });
            }
        }
    }

    Ok(())
}

pub struct UdpForwardOptions {
    pub upstream: String,
    pub sessions: telemetry::SharedSessions,
    pub tunnel_manager: Option<Arc<tunnel::manager::Manager>>,
    pub idle_timeout: Duration,
}

pub async fn serve_udp(listen_addr: &str, opts: UdpForwardOptions) -> anyhow::Result<()> {
    // Backwards-compatible entrypoint: run until process shutdown.
    let (tx, rx) = tokio::sync::watch::channel(false);
    // Keep sender alive for the lifetime of the listener.
    let _tx = tx;
    serve_udp_with_shutdown(listen_addr, opts, rx).await
}

pub async fn serve_udp_with_shutdown(
    listen_addr: &str,
    opts: UdpForwardOptions,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let bind_addr = net::normalize_bind_addr(listen_addr);
    let sock = UdpSocket::bind(bind_addr.as_ref())
        .await
        .with_context(|| format!("bind udp {listen_addr}"))?;

    tracing::info!(listen_addr = %listen_addr, "udp: listening");

    let sock = Arc::new(sock);
    let sessions: Arc<DashMap<std::net::SocketAddr, Arc<UdpSession>>> = Arc::new(DashMap::new());

    if opts.idle_timeout > Duration::from_millis(0) {
        let sessions = sessions.clone();
        let shutdown2 = shutdown.clone();
        tokio::spawn(async move {
            udp_sweep_loop(sessions, opts.idle_timeout, shutdown2).await;
        });
    }

    let mut buf = vec![0u8; 64 * 1024];
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            res = sock.recv_from(&mut buf) => {
                let (n, src) = res?;
                if n == 0 {
                    continue;
                }

                let payload = buf[..n].to_vec();

                let mut sess = sessions
                    .get(&src)
                    .map(|s| s.value().clone())
                    .unwrap_or_else(|| {
                        let s = Arc::new(UdpSession::new(
                            telemetry::new_session_id(),
                            src,
                            opts.upstream.clone(),
                            sock.clone(),
                            opts.sessions.clone(),
                            opts.tunnel_manager.clone(),
                        ));
                        sessions.insert(src, s.clone());
                        s
                    });

                sess.touch();

                if sess.tx.try_send(payload).is_err() {
                    // Session is likely closed or congested; recreate once.
                    let _ = sessions.remove(&src);
                    sess = Arc::new(UdpSession::new(
                        telemetry::new_session_id(),
                        src,
                        opts.upstream.clone(),
                        sock.clone(),
                        opts.sessions.clone(),
                        opts.tunnel_manager.clone(),
                    ));
                    sessions.insert(src, sess.clone());
                    // Best-effort re-send.
                    let _ = sess.tx.try_send(buf[..n].to_vec());
                }
            }
        }
    }

    Ok(())
}

struct UdpSession {
    sid: String,
    src: std::net::SocketAddr,
    upstream: String,
    sock: Arc<UdpSocket>,
    sessions: telemetry::SharedSessions,
    tunnel_manager: Option<Arc<tunnel::manager::Manager>>,
    last_seen_unix_ms: std::sync::atomic::AtomicU64,
    tx: tokio::sync::mpsc::Sender<Vec<u8>>,
}

impl UdpSession {
    fn new(
        sid: String,
        src: std::net::SocketAddr,
        upstream: String,
        sock: Arc<UdpSocket>,
        sessions: telemetry::SharedSessions,
        tunnel_manager: Option<Arc<tunnel::manager::Manager>>,
    ) -> Self {
        let (tx, rx) = tokio::sync::mpsc::channel::<Vec<u8>>(128);

        let s = Self {
            sid: sid.clone(),
            src,
            upstream: upstream.clone(),
            sock,
            sessions,
            tunnel_manager,
            last_seen_unix_ms: std::sync::atomic::AtomicU64::new(telemetry::now_unix_ms()),
            tx,
        };

        s.spawn(rx);
        s
    }

    fn touch(&self) {
        self.last_seen_unix_ms.store(
            telemetry::now_unix_ms(),
            std::sync::atomic::Ordering::Relaxed,
        );
    }

    fn last_seen(&self) -> u64 {
        self.last_seen_unix_ms
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    fn spawn(&self, rx: tokio::sync::mpsc::Receiver<Vec<u8>>) {
        let sid = self.sid.clone();
        let src = self.src;
        let upstream = self.upstream.clone();
        let sock = self.sock.clone();
        let sessions = self.sessions.clone();
        let tunnel_manager = self.tunnel_manager.clone();

        sessions.add(telemetry::SessionInfo {
            id: sid.clone(),
            client: src.to_string(),
            host: "".into(),
            upstream: upstream.clone(),
            started_at_unix_ms: telemetry::now_unix_ms(),
        });

        tokio::spawn(async move {
            let res = udp_session_loop(sock, src, upstream, tunnel_manager, rx).await;
            sessions.remove(&sid);
            if let Err(err) = res {
                if tracing::enabled!(tracing::Level::DEBUG) {
                    tracing::debug!(sid=%sid, err=%err, "udp: session ended");
                }
            }
        });
    }
}

async fn udp_session_loop(
    sock: Arc<UdpSocket>,
    src: std::net::SocketAddr,
    upstream: String,
    tunnel_manager: Option<Arc<tunnel::manager::Manager>>,
    mut rx: tokio::sync::mpsc::Receiver<Vec<u8>>,
) -> anyhow::Result<()> {
    if upstream.trim().is_empty() {
        anyhow::bail!("udp upstream is empty");
    }

    if let Some(rest) = upstream.trim().strip_prefix("tunnel:") {
        let service = rest.trim();
        let mgr = tunnel_manager
            .context("tunnel upstream requested but tunnel manager is not configured")?;

        let st = mgr
            .dial_service_udp(service)
            .await
            .map_err(|e| anyhow::anyhow!("tunnel udp dial failed: {e}"))?;
        let mut up = tunnel::datagram::DatagramConn::new(st);

        let mut buf = vec![0u8; 64 * 1024];
        loop {
            tokio::select! {
                Some(payload) = rx.recv() => {
                    up.write_datagram(&payload).await.map_err(|e| anyhow::anyhow!("tunnel udp write failed: {e}"))?;
                }
                res = up.read_datagram(&mut buf) => {
                    let n = res.map_err(|e| anyhow::anyhow!("tunnel udp read failed: {e}"))?;
                    let _ = sock.send_to(&buf[..n], src).await;
                }
                else => {
                    break;
                }
            }
        }

        return Ok(());
    }

    // Direct UDP forwarding.
    let up = UdpSocket::bind("0.0.0.0:0").await?;
    up.connect(upstream.trim()).await?;
    let up = Arc::new(up);

    let mut buf = vec![0u8; 64 * 1024];
    loop {
        tokio::select! {
            Some(payload) = rx.recv() => {
                let _ = up.send(&payload).await;
            }
            res = up.recv(&mut buf) => {
                let n = res?;
                let _ = sock.send_to(&buf[..n], src).await;
            }
            else => {
                break;
            }
        }
    }

    Ok(())
}

async fn udp_sweep_loop(
    sessions: Arc<DashMap<std::net::SocketAddr, Arc<UdpSession>>>,
    idle_timeout: Duration,
    mut shutdown: tokio::sync::watch::Receiver<bool>,
) {
    let mut tick = tokio::time::interval(Duration::from_secs(1));
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    break;
                }
            }
            _ = tick.tick() => {}
        }

        if idle_timeout <= Duration::from_millis(0) {
            continue;
        }

        let now = telemetry::now_unix_ms();
        let idle_ms = idle_timeout.as_millis() as u64;

        let mut to_remove = Vec::new();
        for s in sessions.iter() {
            let last = s.value().last_seen();
            if now.saturating_sub(last) > idle_ms {
                to_remove.push(*s.key());
            }
        }

        for k in to_remove {
            if let Some((_k, sess)) = sessions.remove(&k) {
                // Dropping the sender closes the session loop eventually.
                drop(sess);
            }
        }
    }
}

#[derive(Debug, Clone)]
struct MinecraftHandshakeMetadata {
    protocol_version: i32,
    host: String,
    port: u16,
    next_state: i32,
}

fn try_parse_minecraft_handshake_metadata(
    prelude: &[u8],
    max_frame_len: usize,
) -> Option<(MinecraftHandshakeMetadata, usize)> {
    // Returns (metadata, frame_len_bytes), where frame_len_bytes includes the VarInt length prefix.
    let mut i = 0usize;
    let (pkt_len, len_n) = read_varint(prelude, i)?;
    i += len_n;
    if pkt_len < 0 {
        return None;
    }
    let pkt_len = pkt_len as usize;
    if pkt_len > max_frame_len {
        return None;
    }
    if i + pkt_len > prelude.len() {
        return None;
    }
    let frame_end = i + pkt_len;

    let (packet_id, n) = read_varint(prelude, i)?;
    i += n;
    if packet_id != 0 {
        return None;
    }

    let (protocol_version, n) = read_varint(prelude, i)?;
    i += n;

    let (addr_len, n) = read_varint(prelude, i)?;
    i += n;
    if addr_len < 0 {
        return None;
    }
    let addr_len = addr_len as usize;
    if i + addr_len + 2 > frame_end {
        return None;
    }

    let host = String::from_utf8_lossy(&prelude[i..i + addr_len])
        .trim()
        .to_ascii_lowercase();
    i += addr_len;

    let port = u16::from_be_bytes([prelude[i], prelude[i + 1]]);
    i += 2;

    let (next_state, _n) = read_varint(prelude, i)?;

    Some((
        MinecraftHandshakeMetadata {
            protocol_version,
            host,
            port,
            next_state,
        },
        len_n + pkt_len,
    ))
}

fn normalize_status_cache_upstream(
    upstream: &str,
    default_port: u16,
    routed_host: &str,
    md: &MinecraftHandshakeMetadata,
) -> String {
    let addr = upstream.trim();
    if addr.is_empty() {
        return String::new();
    }

    if addr.to_ascii_lowercase().starts_with("tunnel:") {
        return addr.to_string();
    }

    if !upstream_needs_port(addr) {
        return addr.to_string();
    }

    let port = if !md.host.is_empty() && md.host == routed_host {
        md.port
    } else {
        default_port
    };

    format!("{addr}:{port}")
}

fn decode_varint_prefix(buf: &[u8]) -> anyhow::Result<Option<(i32, usize)>> {
    let mut num_read = 0;
    let mut result: i32 = 0;

    for &read in buf.iter().take(5) {
        let value = (read & 0x7F) as i32;
        result |= value << (7 * num_read);
        num_read += 1;
        if num_read > 5 {
            anyhow::bail!("protocol: varint too long");
        }
        if (read & 0x80) == 0 {
            return Ok(Some((result, num_read as usize)));
        }
    }

    Ok(None)
}

async fn read_mc_packet_raw_buffered_opt(
    buf: &mut Vec<u8>,
    conn: &mut TcpStream,
    max_len: usize,
    timeout: Duration,
) -> anyhow::Result<Option<(Vec<u8>, i32)>> {
    let fut = async {
        let mut tmp = vec![0u8; 4096];
        loop {
            let Some((pkt_len, len_n)) = decode_varint_prefix(buf)? else {
                let n = conn.read(&mut tmp).await?;
                if n == 0 {
                    if buf.is_empty() {
                        return Ok(None);
                    }
                    anyhow::bail!("protocol: unexpected eof while reading packet length");
                }
                buf.extend_from_slice(&tmp[..n]);
                continue;
            };

            if pkt_len < 0 {
                anyhow::bail!("protocol: negative packet length");
            }
            let pkt_len = pkt_len as usize;
            if pkt_len > max_len {
                anyhow::bail!("protocol: packet too large ({pkt_len} > {max_len})");
            }

            let total = len_n + pkt_len;
            while buf.len() < total {
                let n = conn.read(&mut tmp).await?;
                if n == 0 {
                    anyhow::bail!("protocol: unexpected eof while reading packet payload");
                }
                buf.extend_from_slice(&tmp[..n]);
            }

            let raw: Vec<u8> = buf.drain(..total).collect();
            let payload = &raw[len_n..];
            let (pid, _n) = read_varint(payload, 0)
                .ok_or_else(|| anyhow::anyhow!("protocol: missing packet id"))?;
            return Ok(Some((raw, pid)));
        }
    };

    if timeout > Duration::from_millis(0) {
        match time::timeout(timeout, fut).await {
            Ok(v) => v,
            Err(_) => Ok(None),
        }
    } else {
        fut.await
    }
}

async fn read_mc_packet_raw_stream(
    r: &mut (dyn tokio::io::AsyncRead + Unpin + Send),
    max_len: usize,
    timeout: Duration,
) -> anyhow::Result<(Vec<u8>, i32)> {
    let fut = async {
        let mut prefix = Vec::with_capacity(5);
        let mut num_read = 0;
        let mut result: i32 = 0;
        loop {
            let mut b = [0u8; 1];
            r.read_exact(&mut b).await?;
            let read = b[0];
            prefix.push(read);

            let value = (read & 0x7F) as i32;
            result |= value << (7 * num_read);
            num_read += 1;
            if num_read > 5 {
                anyhow::bail!("protocol: varint too long");
            }
            if (read & 0x80) == 0 {
                break;
            }
        }

        if result < 0 {
            anyhow::bail!("protocol: negative packet length");
        }
        let pkt_len = result as usize;
        if pkt_len > max_len {
            anyhow::bail!("protocol: packet too large ({pkt_len} > {max_len})");
        }

        let mut payload = vec![0u8; pkt_len];
        r.read_exact(&mut payload).await?;
        let (pid, _n) = read_varint(&payload, 0)
            .ok_or_else(|| anyhow::anyhow!("protocol: missing packet id"))?;

        let mut raw = prefix;
        raw.extend_from_slice(&payload);
        Ok((raw, pid))
    };

    if timeout > Duration::from_millis(0) {
        time::timeout(timeout, fut)
            .await
            .context("protocol: read packet timeout")?
    } else {
        fut.await
    }
}

async fn reply_ping_pong(
    conn: &mut TcpStream,
    buf: &mut Vec<u8>,
    timeout: Duration,
) -> anyhow::Result<()> {
    let Some((raw, pid)) = read_mc_packet_raw_buffered_opt(buf, conn, 64 * 1024, timeout).await?
    else {
        return Ok(());
    };
    if pid != 1 {
        return Ok(());
    }
    conn.write_all(&raw).await.context("proxy: write pong")?;
    Ok(())
}

async fn fetch_status_response(
    upstream: &str,
    default_port: u16,
    dial_timeout: Duration,
    read_timeout: Duration,
    tunnel_manager: Option<&Arc<tunnel::manager::Manager>>,
    proxy_protocol_v2: bool,
    client: &TcpStream,
    handshake_raw: &[u8],
    status_req_raw: &[u8],
) -> anyhow::Result<Vec<u8>> {
    let (mut up, _label) =
        dial_upstream(upstream, Some(default_port), dial_timeout, tunnel_manager).await?;

    if proxy_protocol_v2 {
        write_proxy_proto_v2(&mut *up, client).await?;
    }

    (&mut *up)
        .write_all(handshake_raw)
        .await
        .context("proxy: write status handshake")?;
    (&mut *up)
        .write_all(status_req_raw)
        .await
        .context("proxy: write status request")?;

    let (raw, pid) = read_mc_packet_raw_stream(&mut *up, 512 * 1024, read_timeout).await?;
    if pid != 0 {
        anyhow::bail!("protocol: unexpected status response packet id {pid}");
    }
    Ok(raw)
}

async fn try_handle_minecraft_status_cached(
    conn: &mut TcpStream,
    captured: &mut Vec<u8>,
    sid: &str,
    client: &str,
    host: &str,
    res: &router::Resolution,
    default_port: u16,
    rt: &TcpRuntimeConfig,
    opts: &TcpRoutingHandlerOptions,
) -> bool {
    let Some(ttl) = res.cache_ping_ttl.filter(|d| *d > Duration::from_millis(0)) else {
        return false;
    };

    let Some((md, handshake_len)) = try_parse_minecraft_handshake_metadata(captured, 256 * 1024)
    else {
        return false;
    };

    if md.next_state != 1 {
        return false;
    }

    let cache = default_status_cache();
    let handshake_raw = captured[..handshake_len].to_vec();
    let mut post_handshake = captured[handshake_len..].to_vec();

    let Some((status_req_raw, status_pid)) = (match read_mc_packet_raw_buffered_opt(
        &mut post_handshake,
        conn,
        64 * 1024,
        rt.handshake_timeout,
    )
    .await
    {
        Ok(v) => v,
        Err(_) => None,
    }) else {
        // Couldn't read the status request cleanly; fall back to normal proxying.
        let mut restored = handshake_raw;
        restored.extend_from_slice(&post_handshake);
        *captured = restored;
        return false;
    };

    // Ensure we can fall back without losing already-consumed bytes.
    let mut restored = handshake_raw.clone();
    restored.extend_from_slice(&status_req_raw);
    restored.extend_from_slice(&post_handshake);
    *captured = restored;

    if status_pid != 0 {
        return false;
    }

    for cand in &res.upstreams {
        let upstream_key = normalize_status_cache_upstream(cand, default_port, host, &md);
        if upstream_key.is_empty() {
            continue;
        }
        let key = StatusCacheKey {
            upstream: upstream_key.clone(),
            protocol_version: md.protocol_version,
        };

        if let Some(resp) = cache.get(&key).await {
            opts.sessions.add(telemetry::SessionInfo {
                id: sid.to_string(),
                client: client.to_string(),
                host: host.to_string(),
                upstream: upstream_key.clone(),
                started_at_unix_ms: telemetry::now_unix_ms(),
            });

            let _ = conn.write_all(&resp).await;
            let _ = reply_ping_pong(conn, &mut post_handshake, rt.idle_timeout).await;
            let _ = conn.shutdown().await;
            opts.sessions.remove(sid);
            return true;
        }

        let loaded = cache
            .get_or_load(key, ttl, || async {
                fetch_status_response(
                    &upstream_key,
                    default_port,
                    rt.upstream_dial_timeout,
                    rt.handshake_timeout,
                    opts.tunnel_manager.as_ref(),
                    rt.proxy_protocol_v2,
                    conn,
                    &handshake_raw,
                    &status_req_raw,
                )
                .await
            })
            .await;

        let resp = match loaded {
            Ok(v) => v,
            Err(_) => continue,
        };

        opts.sessions.add(telemetry::SessionInfo {
            id: sid.to_string(),
            client: client.to_string(),
            host: host.to_string(),
            upstream: upstream_key.clone(),
            started_at_unix_ms: telemetry::now_unix_ms(),
        });

        let _ = conn.write_all(&resp).await;
        let _ = reply_ping_pong(conn, &mut post_handshake, rt.idle_timeout).await;
        let _ = conn.shutdown().await;
        opts.sessions.remove(sid);
        return true;
    }

    false
}

async fn handle_forward(mut conn: TcpStream, opts: Arc<TcpForwardHandlerOptions>) {
    let _active = ActiveConnGuard::new();
    let sid = telemetry::new_session_id();
    let client = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();

    let upstream = opts.upstream.trim().to_string();
    if upstream.is_empty() {
        let _ = conn.shutdown().await;
        return;
    }

    let rt = { opts.runtime.read().await.clone() };

    let (up, upstream_used) = match dial_upstream(
        &upstream,
        None,
        rt.upstream_dial_timeout,
        opts.tunnel_manager.as_ref(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(sid = %sid, client = %client, upstream = %upstream, err = %err, "proxy: forward dial failed");
            let _ = conn.shutdown().await;
            return;
        }
    };

    opts.sessions.add(telemetry::SessionInfo {
        id: sid.clone(),
        client: client.clone(),
        host: "".into(),
        upstream: upstream_used.clone(),
        started_at_unix_ms: telemetry::now_unix_ms(),
    });

    let mut up = up;
    if rt.proxy_protocol_v2 {
        if let Err(err) = write_proxy_proto_v2(&mut *up, &conn).await {
            tracing::warn!(sid = %sid, client = %client, upstream = %upstream_used, err = %err, "proxy: proxy_protocol_v2 write failed");
            let _ = conn.shutdown().await;
            opts.sessions.remove(&sid);
            return;
        }
    }

    let res = proxy_bidirectional(&mut conn, up, rt.buffer_size, rt.idle_timeout).await;

    opts.sessions.remove(&sid);

    match res {
        Ok((ingress, egress)) => {
            metrics::counter!("prism_bytes_ingress_total").increment(ingress);
            metrics::counter!("prism_bytes_egress_total").increment(egress);
        }
        Err(err) => {
            tracing::debug!(sid = %sid, err = %err, "proxy: forward ended with error");
        }
    }
}

async fn handle_routing(mut conn: TcpStream, opts: Arc<TcpRoutingHandlerOptions>) {
    let _active = ActiveConnGuard::new();
    let sid = telemetry::new_session_id();
    let client = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();

    let rt = { opts.runtime.read().await.clone() };

    let max_header = if rt.max_header_bytes == 0 {
        64 * 1024
    } else {
        rt.max_header_bytes
    };

    // Capture prelude.
    let mut captured: Vec<u8> = Vec::with_capacity(4096.min(max_header));
    let mut tmp = vec![0u8; 4096];

    let res = {
        let read_fut = async {
            loop {
                if captured.len() >= max_header {
                    break Ok::<Option<router::Resolution>, protocol::ParseError>(None);
                }
                let n = conn
                    .read(&mut tmp)
                    .await
                    .map_err(|e| protocol::ParseError::Fatal(format!("read failed: {e}")))?;
                if n == 0 {
                    break Ok(None);
                }

                let need = (max_header - captured.len()).min(n);
                captured.extend_from_slice(&tmp[..need]);

                match opts.router.resolve_prelude(&captured) {
                    Ok(Some(r)) => break Ok(Some(r)),
                    Ok(None) => break Ok(None),
                    Err(protocol::ParseError::NeedMoreData) => continue,
                    Err(e) => break Err(e),
                }
            }
        };

        if rt.handshake_timeout > Duration::from_millis(0) {
            match time::timeout(rt.handshake_timeout, read_fut).await {
                Ok(Ok(r)) => r,
                Ok(Err(e)) => {
                    tracing::warn!(sid=%sid, client=%client, err=%e, "proxy: routing header parse failed");
                    let _ = conn.shutdown().await;
                    return;
                }
                Err(_) => {
                    tracing::debug!(sid=%sid, client=%client, "proxy: handshake timeout");
                    let _ = conn.shutdown().await;
                    return;
                }
            }
        } else {
            match read_fut.await {
                Ok(r) => r,
                Err(e) => {
                    tracing::warn!(sid=%sid, client=%client, err=%e, "proxy: routing header parse failed");
                    let _ = conn.shutdown().await;
                    return;
                }
            }
        }
    };

    let Some(res) = res else {
        tracing::debug!(sid=%sid, client=%client, "proxy: no route matched prelude");
        let _ = conn.shutdown().await;
        return;
    };

    let host = res.host.trim().to_ascii_lowercase();
    if host.is_empty() {
        let _ = conn.shutdown().await;
        return;
    }

    metrics::counter!("prism_route_hits_total", "host" => host.clone()).increment(1);

    let default_port = mc_handshake_port(&captured)
        .or_else(|| conn.local_addr().ok().map(|a| a.port()))
        .unwrap_or(25565);

    if try_handle_minecraft_status_cached(
        &mut conn,
        &mut captured,
        &sid,
        &client,
        &host,
        &res,
        default_port,
        &rt,
        opts.as_ref(),
    )
    .await
    {
        return;
    }

    // Dial upstream candidates with failover.
    let mut last_err: Option<anyhow::Error> = None;
    let mut upstream_used = String::new();
    let mut up_conn: Option<tunnel::transport::BoxedStream> = None;

    for cand in &res.upstreams {
        let addr = cand.trim().to_string();
        match dial_upstream(
            &addr,
            Some(default_port),
            rt.upstream_dial_timeout,
            opts.tunnel_manager.as_ref(),
        )
        .await
        {
            Ok((c, label)) => {
                upstream_used = label;
                up_conn = Some(c);
                break;
            }
            Err(err) => last_err = Some(err),
        }
    }

    let Some(mut up) = up_conn else {
        tracing::warn!(sid=%sid, client=%client, host=%host, err=%last_err.map(|e| e.to_string()).unwrap_or_default(), "proxy: upstream dial failed");
        let _ = conn.shutdown().await;
        return;
    };

    opts.sessions.add(telemetry::SessionInfo {
        id: sid.clone(),
        client,
        host: host.clone(),
        upstream: upstream_used.clone(),
        started_at_unix_ms: telemetry::now_unix_ms(),
    });

    if tracing::enabled!(tracing::Level::DEBUG) {
        tracing::debug!(sid=%sid, host=%host, upstream=%upstream_used, "proxy: routed");
    }

    // Forward captured prelude upstream unchanged.
    if rt.proxy_protocol_v2 {
        if let Err(err) = write_proxy_proto_v2(&mut *up, &conn).await {
            tracing::warn!(sid=%sid, err=%err, "proxy: proxy_protocol_v2 write failed");
            let _ = conn.shutdown().await;
            opts.sessions.remove(&sid);
            return;
        }
    }

    if let Err(err) = (&mut *up).write_all(&captured).await {
        tracing::debug!(sid=%sid, err=%err, "proxy: failed writing prelude to upstream");
        let _ = conn.shutdown().await;
        opts.sessions.remove(&sid);
        return;
    }

    let res = proxy_bidirectional(&mut conn, up, rt.buffer_size, rt.idle_timeout).await;

    opts.sessions.remove(&sid);

    match res {
        Ok((ingress, egress)) => {
            metrics::counter!("prism_bytes_ingress_total").increment(ingress);
            metrics::counter!("prism_bytes_egress_total").increment(egress);
        }
        Err(err) => {
            tracing::debug!(sid=%sid, err=%err, "proxy: session ended with error");
        }
    }
}

async fn dial_tcp_stream(
    addr: &str,
    timeout: Duration,
) -> anyhow::Result<tunnel::transport::BoxedStream> {
    let c = if timeout > Duration::from_millis(0) {
        time::timeout(timeout, TcpStream::connect(addr))
            .await
            .with_context(|| format!("dial timeout {addr}"))??
    } else {
        TcpStream::connect(addr).await?
    };
    Ok(Box::new(c))
}

async fn dial_upstream(
    upstream: &str,
    default_port: Option<u16>,
    timeout: Duration,
    tunnel_manager: Option<&Arc<tunnel::manager::Manager>>,
) -> anyhow::Result<(tunnel::transport::BoxedStream, String)> {
    let mut addr = upstream.trim().to_string();
    if addr.is_empty() {
        anyhow::bail!("empty upstream");
    }

    if let Some(rest) = addr.strip_prefix("tunnel:") {
        let service = rest.trim();
        if service.is_empty() {
            anyhow::bail!("tunnel upstream missing service name");
        }
        let mgr = tunnel_manager
            .context("tunnel upstream requested but tunnel manager is not configured")?;
        let st = mgr
            .dial_service_tcp(service)
            .await
            .map_err(|e| anyhow::anyhow!("tunnel dial failed: {e}"))?;
        return Ok((st, format!("tunnel:{service}")));
    }

    if let Some(p) = default_port {
        if upstream_needs_port(&addr) {
            addr = format!("{addr}:{p}");
        }
    }

    Ok((dial_tcp_stream(&addr, timeout).await?, addr))
}

async fn proxy_bidirectional(
    client: &mut TcpStream,
    mut upstream: tunnel::transport::BoxedStream,
    buffer_size: usize,
    idle_timeout: Duration,
) -> anyhow::Result<(u64, u64)> {
    // Apply optional idle timeout by bounding the whole copy operation.
    let copy_fut = async {
        let (a, b) = tokio::io::copy_bidirectional(client, &mut *upstream).await?;
        Ok::<(u64, u64), std::io::Error>((a, b))
    };

    let (ingress, egress) = if idle_timeout > Duration::from_millis(0) {
        time::timeout(idle_timeout, copy_fut)
            .await
            .context("idle timeout")??
    } else {
        copy_fut.await?
    };

    // `copy_bidirectional` doesn't allow tuning buffer sizes; keep the field for future improvements.
    let _ = buffer_size;

    // Best-effort shutdown.
    let _ = (&mut *upstream).shutdown().await;
    Ok((ingress, egress))
}

async fn write_proxy_proto_v2(
    upstream: &mut (dyn tokio::io::AsyncWrite + Send + Unpin),
    client: &TcpStream,
) -> anyhow::Result<()> {
    use std::net::{IpAddr, SocketAddr};

    let src: SocketAddr = client.peer_addr().context("proxy: peer_addr")?;
    let dst: SocketAddr = client.local_addr().context("proxy: local_addr")?;

    // Signature: "\r\n\r\n\0\r\nQUIT\n"
    const SIG: [u8; 12] = [13, 10, 13, 10, 0, 13, 10, 81, 85, 73, 84, 10];

    let mut out = Vec::with_capacity(16 + 36);
    out.extend_from_slice(&SIG);

    // ver=2 (0x2) | cmd=PROXY (0x1)
    out.push(0x21);

    match (src.ip(), dst.ip()) {
        (IpAddr::V4(sip), IpAddr::V4(dip)) => {
            // fam=INET(0x1) | proto=STREAM(0x1)
            out.push(0x11);
            out.extend_from_slice(&(12u16).to_be_bytes());
            out.extend_from_slice(&sip.octets());
            out.extend_from_slice(&dip.octets());
            out.extend_from_slice(&src.port().to_be_bytes());
            out.extend_from_slice(&dst.port().to_be_bytes());
        }
        (IpAddr::V6(sip), IpAddr::V6(dip)) => {
            // fam=INET6(0x2) | proto=STREAM(0x1)
            out.push(0x21);
            out.extend_from_slice(&(36u16).to_be_bytes());
            out.extend_from_slice(&sip.octets());
            out.extend_from_slice(&dip.octets());
            out.extend_from_slice(&src.port().to_be_bytes());
            out.extend_from_slice(&dst.port().to_be_bytes());
        }
        _ => {
            // Unknown / unsupported; encode as UNSPEC with zero length.
            out.push(0x00);
            out.extend_from_slice(&(0u16).to_be_bytes());
        }
    }

    upstream.write_all(&out).await.context("proxy: write pp2")?;
    upstream.flush().await.ok();
    Ok(())
}

fn upstream_needs_port(addr: &str) -> bool {
    // Very small heuristic: if there is no ':' after the last ']' (IPv6 brackets), assume missing port.
    let s = addr.trim();
    if s.is_empty() {
        return false;
    }
    let after = if let Some(pos) = s.rfind(']') {
        &s[pos + 1..]
    } else {
        s
    };
    !after.contains(':')
}

fn mc_handshake_port(prelude: &[u8]) -> Option<u16> {
    // Parse a Minecraft handshake enough to read the port field.
    // This is intentionally conservative: return None on any inconsistency.

    let mut i = 0usize;
    let (_pkt_len, n) = read_varint(prelude, i)?;
    i += n;

    let (_packet_id, n) = read_varint(prelude, i)?;
    i += n;

    let (_proto_ver, n) = read_varint(prelude, i)?;
    i += n;

    let (addr_len, n) = read_varint(prelude, i)?;
    i += n;
    if addr_len < 0 {
        return None;
    }
    let addr_len = addr_len as usize;
    if i + addr_len + 2 > prelude.len() {
        return None;
    }
    i += addr_len;

    let port = u16::from_be_bytes([prelude[i], prelude[i + 1]]);
    Some(port)
}

fn read_varint(buf: &[u8], mut i: usize) -> Option<(i32, usize)> {
    let mut num_read = 0;
    let mut result: i32 = 0;

    loop {
        if i >= buf.len() {
            return None;
        }
        let read = buf[i];
        i += 1;

        let value = (read & 0x7F) as i32;
        result |= value << (7 * num_read);

        num_read += 1;
        if num_read > 5 {
            return None;
        }
        if (read & 0x80) == 0 {
            break;
        }
    }

    Some((result, num_read as usize))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prism::{config, router, telemetry};

    struct MockMinecraftParser;

    impl protocol::HostParser for MockMinecraftParser {
        fn name(&self) -> &str {
            "mock-minecraft"
        }

        fn parse(&self, prelude: &[u8]) -> Result<String, protocol::ParseError> {
            let Some((md, _len)) = try_parse_minecraft_handshake_metadata(prelude, 256 * 1024)
            else {
                return Err(protocol::ParseError::NeedMoreData);
            };
            if md.host.is_empty() {
                return Err(protocol::ParseError::NoMatch);
            }
            Ok(md.host)
        }
    }

    fn write_varint(mut n: i32, out: &mut Vec<u8>) {
        loop {
            let mut temp = (n & 0x7F) as u8;
            n = ((n as u32) >> 7) as i32;
            if n != 0 {
                temp |= 0x80;
            }
            out.push(temp);
            if n == 0 {
                break;
            }
        }
    }

    fn write_mc_string(s: &str, out: &mut Vec<u8>) {
        write_varint(s.len() as i32, out);
        out.extend_from_slice(s.as_bytes());
    }

    fn build_handshake_packet(host: &str, port: u16, proto_ver: i32, next_state: i32) -> Vec<u8> {
        let mut payload = Vec::new();
        write_varint(0, &mut payload); // packet id
        write_varint(proto_ver, &mut payload);
        write_mc_string(host, &mut payload);
        payload.extend_from_slice(&port.to_be_bytes());
        write_varint(next_state, &mut payload);

        let mut out = Vec::new();
        write_varint(payload.len() as i32, &mut out);
        out.extend_from_slice(&payload);
        out
    }

    fn build_status_request_packet() -> Vec<u8> {
        let mut payload = Vec::new();
        write_varint(0, &mut payload); // packet id
        let mut out = Vec::new();
        write_varint(payload.len() as i32, &mut out);
        out.extend_from_slice(&payload);
        out
    }

    fn build_status_response_packet(json: &str) -> Vec<u8> {
        let mut payload = Vec::new();
        write_varint(0, &mut payload); // packet id
        write_mc_string(json, &mut payload);
        let mut out = Vec::new();
        write_varint(payload.len() as i32, &mut out);
        out.extend_from_slice(&payload);
        out
    }

    fn build_ping_packet(v: i64) -> Vec<u8> {
        let mut payload = Vec::new();
        write_varint(1, &mut payload); // packet id
        payload.extend_from_slice(&v.to_be_bytes());
        let mut out = Vec::new();
        write_varint(payload.len() as i32, &mut out);
        out.extend_from_slice(&payload);
        out
    }

    #[tokio::test]
    async fn status_response_caching_avoids_second_dial() {
        let backend_ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_ln.local_addr().unwrap();

        let handshake = build_handshake_packet("play.example.com", 25565, 763, 1);
        let status_req = build_status_request_packet();
        let ping1 = build_ping_packet(42);
        let ping2 = build_ping_packet(7);

        let backend_task = tokio::spawn({
            let handshake = handshake.clone();
            let status_req = status_req.clone();
            async move {
                let (mut s, _) = backend_ln.accept().await.unwrap();
                let mut got = vec![0u8; handshake.len() + status_req.len()];
                s.read_exact(&mut got).await.unwrap();
                assert_eq!(got, [handshake, status_req].concat());
                let resp = build_status_response_packet(
                    r#"{"version":{"name":"x","protocol":763},"players":{"max":0,"online":0},"description":"hi"}"#,
                );
                s.write_all(&resp).await.unwrap();
            }
        });

        let proxy_ln = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let proxy_addr = proxy_ln.local_addr().unwrap();

        let route_cfg = config::RouteConfig {
            host: vec!["play.example.com".into()],
            upstreams: vec![backend_addr.to_string()],
            parsers: vec!["minecraft_handshake".into()],
            strategy: "sequential".into(),
            cache_ping_ttl: Some(Duration::from_secs(5)),
        };
        let parser: protocol::SharedHostParser = Arc::new(MockMinecraftParser);
        let r = Arc::new(router::Router::new(vec![(route_cfg, parser)]));
        let opts = Arc::new(TcpRoutingHandlerOptions {
            router: r,
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            tunnel_manager: None,
            runtime: Arc::new(tokio::sync::RwLock::new(TcpRuntimeConfig {
                max_header_bytes: 64 * 1024,
                handshake_timeout: Duration::from_secs(2),
                idle_timeout: Duration::from_secs(2),
                upstream_dial_timeout: Duration::from_secs(2),
                buffer_size: 16 * 1024,
                proxy_protocol_v2: false,
            })),
        });

        let accept_task = tokio::spawn({
            let opts = opts.clone();
            async move {
                loop {
                    let (c, _) = proxy_ln.accept().await.unwrap();
                    let o = opts.clone();
                    tokio::spawn(async move {
                        handle_routing(c, o).await;
                    });
                }
            }
        });

        // First ping: should dial backend and populate cache.
        {
            let mut c = TcpStream::connect(proxy_addr).await.unwrap();
            c.write_all(&handshake).await.unwrap();
            c.write_all(&status_req).await.unwrap();

            let (_raw, pid) = read_mc_packet_raw_stream(&mut c, 512 * 1024, Duration::from_secs(2))
                .await
                .unwrap();
            assert_eq!(pid, 0);
            c.write_all(&ping1).await.unwrap();
            let (_raw, pid) = read_mc_packet_raw_stream(&mut c, 512 * 1024, Duration::from_secs(2))
                .await
                .unwrap();
            assert_eq!(pid, 1);
        }

        backend_task.await.unwrap();

        // Second ping: should serve from cache without dialing backend (backend listener is gone).
        {
            let mut c = TcpStream::connect(proxy_addr).await.unwrap();
            c.write_all(&handshake).await.unwrap();
            c.write_all(&status_req).await.unwrap();

            let (_raw, pid) = read_mc_packet_raw_stream(&mut c, 512 * 1024, Duration::from_secs(2))
                .await
                .unwrap();
            assert_eq!(pid, 0);
            c.write_all(&ping2).await.unwrap();
            let (_raw, pid) = read_mc_packet_raw_stream(&mut c, 512 * 1024, Duration::from_secs(2))
                .await
                .unwrap();
            assert_eq!(pid, 1);
        }

        accept_task.abort();
    }
}
