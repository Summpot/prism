use std::{sync::Arc, time::Duration};

use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream, UdpSocket},
    time,
};

use dashmap::DashMap;

use crate::prism::{middleware, net, router, telemetry, tunnel};

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

    let (up, upstream_used, _tunnel_masquerade_host) = match dial_upstream(
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
                    break Ok::<Option<router::Resolution>, middleware::MiddlewareError>(None);
                }
                let n = conn
                    .read(&mut tmp)
                    .await
                    .map_err(|e| middleware::MiddlewareError::Fatal(format!("read failed: {e}")))?;
                if n == 0 {
                    break Ok(None);
                }

                let need = (max_header - captured.len()).min(n);
                captured.extend_from_slice(&tmp[..need]);

                match opts.router.resolve_prelude(&captured) {
                    Ok(Some(r)) => break Ok(Some(r)),
                    Ok(None) => break Ok(None),
                    Err(middleware::MiddlewareError::NeedMoreData) => continue,
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

    let router::Resolution {
        host: resolved_host,
        upstreams,
        middleware,
        prelude_override,
        captures,
        ..
    } = res;

    let host = resolved_host.trim().to_ascii_lowercase();
    if host.is_empty() {
        let _ = conn.shutdown().await;
        return;
    }

    metrics::counter!("prism_route_hits_total", "host" => host.clone()).increment(1);

    let default_port = conn.local_addr().ok().map(|a| a.port());

    // Dial upstream candidates with failover.
    let mut last_err: Option<anyhow::Error> = None;
    let mut upstream_used = String::new();
    let mut up_conn: Option<tunnel::transport::BoxedStream> = None;
    let mut tunnel_masquerade_host: Option<String> = None;

    for cand in &upstreams {
        let addr = cand.trim().to_string();
        match dial_upstream(
            &addr,
            default_port,
            rt.upstream_dial_timeout,
            opts.tunnel_manager.as_ref(),
        )
        .await
        {
            Ok((c, label, masq)) => {
                upstream_used = label;
                up_conn = Some(c);
                tunnel_masquerade_host = masq;
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

    // Apply any middleware prelude overrides from parse phase, then allow a rewrite pass based on
    // the selected upstream.
    let mut prelude = prelude_override.unwrap_or(captured);

    let selected_for_rewrite = if let Some(tpl) = tunnel_masquerade_host.as_ref() {
        let v = router::substitute_params(tpl, &captures);
        let v = v.trim().to_ascii_lowercase();
        if v.is_empty() {
            upstream_used.clone()
        } else {
            v
        }
    } else {
        upstream_used.clone()
    };

    if let Some(rw) = middleware.rewrite(&prelude, &selected_for_rewrite) {
        prelude = rw;
    }

    // Forward captured prelude upstream.
    if rt.proxy_protocol_v2 {
        if let Err(err) = write_proxy_proto_v2(&mut *up, &conn).await {
            tracing::warn!(sid=%sid, err=%err, "proxy: proxy_protocol_v2 write failed");
            let _ = conn.shutdown().await;
            opts.sessions.remove(&sid);
            return;
        }
    }

    if let Err(err) = (&mut *up).write_all(&prelude).await {
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
) -> anyhow::Result<(tunnel::transport::BoxedStream, String, Option<String>)> {
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
        let (st, svc) = mgr
            .dial_service_tcp_with_meta(service)
            .await
            .map_err(|e| anyhow::anyhow!("tunnel dial failed: {e}"))?;

        let masq = svc.masquerade_host.trim().to_string();
        let masq = if masq.is_empty() { None } else { Some(masq) };

        return Ok((st, format!("tunnel:{service}"), masq));
    }

    if let Some(p) = default_port {
        if upstream_needs_port(&addr) {
            addr = format!("{addr}:{p}");
        }
    }

    Ok((dial_tcp_stream(&addr, timeout).await?, addr, None))
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
