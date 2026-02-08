use std::{sync::Arc, time::Duration};

use anyhow::Context;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    time,
};

use crate::prism::{protocol, router, telemetry, tunnel};

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
    pub parser: protocol::SharedHostParser,
    pub router: Arc<router::Router>,
    pub metrics: telemetry::SharedMetrics,
    pub sessions: telemetry::SharedSessions,

    pub tunnel_manager: Option<Arc<tunnel::manager::Manager>>,

    pub max_header_bytes: usize,
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
    pub upstream_dial_timeout: Duration,
    pub buffer_size: usize,
}

pub struct TcpForwardHandlerOptions {
    pub upstream: String,

    pub metrics: telemetry::SharedMetrics,
    pub sessions: telemetry::SharedSessions,

    pub tunnel_manager: Option<Arc<tunnel::manager::Manager>>,

    pub idle_timeout: Duration,
    pub upstream_dial_timeout: Duration,
    pub buffer_size: usize,
}

pub async fn serve_tcp(listen_addr: &str, handler: TcpHandler) -> anyhow::Result<()> {
    let ln = TcpListener::bind(listen_addr)
        .await
        .with_context(|| format!("bind tcp {listen_addr}"))?;

    tracing::info!(listen_addr = %listen_addr, "tcp: listening");

    loop {
        let (conn, peer) = ln.accept().await?;
        let h = handler.clone();

        tokio::spawn(async move {
            if tracing::enabled!(tracing::Level::DEBUG) {
                tracing::debug!(client = %peer, "tcp: accepted");
            }
            h.handle(conn).await;
        });
    }
}

async fn handle_forward(mut conn: TcpStream, opts: Arc<TcpForwardHandlerOptions>) {
    opts.metrics.inc_active();
    let sid = telemetry::new_session_id();
    let client = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();

    let upstream = opts.upstream.trim().to_string();
    if upstream.is_empty() {
        let _ = conn.shutdown().await;
        opts.metrics.dec_active();
        return;
    }

    let (up, upstream_used) = match dial_upstream(
        &upstream,
        None,
        opts.upstream_dial_timeout,
        opts.tunnel_manager.as_ref(),
    )
    .await
    {
        Ok(v) => v,
        Err(err) => {
            tracing::warn!(sid = %sid, client = %client, upstream = %upstream, err = %err, "proxy: forward dial failed");
            let _ = conn.shutdown().await;
            opts.metrics.dec_active();
            return;
        }
    };

    opts.sessions.add(telemetry::SessionInfo {
        id: sid.clone(),
        client,
        host: "".into(),
        upstream: upstream_used,
        started_at_unix_ms: telemetry::now_unix_ms(),
    });

    let res = proxy_bidirectional(&mut conn, up, opts.buffer_size, opts.idle_timeout).await;

    opts.sessions.remove(&sid);
    opts.metrics.dec_active();

    match res {
        Ok((ingress, egress)) => {
            opts.metrics.add_bytes(ingress, egress);
        }
        Err(err) => {
            tracing::debug!(sid = %sid, err = %err, "proxy: forward ended with error");
        }
    }
}

async fn handle_routing(mut conn: TcpStream, opts: Arc<TcpRoutingHandlerOptions>) {
    opts.metrics.inc_active();
    let sid = telemetry::new_session_id();
    let client = conn.peer_addr().map(|a| a.to_string()).unwrap_or_default();

    let max_header = if opts.max_header_bytes == 0 {
        64 * 1024
    } else {
        opts.max_header_bytes
    };

    // Capture prelude.
    let mut captured: Vec<u8> = Vec::with_capacity(4096.min(max_header));
    let mut tmp = vec![0u8; 4096];

    let host = {
        let read_fut = async {
            loop {
                if captured.len() >= max_header {
                    break Ok::<String, protocol::ParseError>(String::new());
                }
                let n = conn
                    .read(&mut tmp)
                    .await
                    .map_err(|e| protocol::ParseError::Fatal(format!("read failed: {e}")))?;
                if n == 0 {
                    break Ok(String::new());
                }

                let need = (max_header - captured.len()).min(n);
                captured.extend_from_slice(&tmp[..need]);

                match opts.parser.parse(&captured) {
                    Ok(h) => break Ok(h),
                    Err(protocol::ParseError::NeedMoreData) => continue,
                    Err(protocol::ParseError::NoMatch) => break Err(protocol::ParseError::NoMatch),
                    Err(e) => break Err(e),
                }
            }
        };

        if opts.handshake_timeout > Duration::from_millis(0) {
            match time::timeout(opts.handshake_timeout, read_fut).await {
                Ok(Ok(h)) => h,
                Ok(Err(e)) => {
                    tracing::warn!(sid=%sid, client=%client, parser=%opts.parser.name(), err=%e, "proxy: routing header parse failed");
                    let _ = conn.shutdown().await;
                    opts.metrics.dec_active();
                    return;
                }
                Err(_) => {
                    tracing::debug!(sid=%sid, client=%client, "proxy: handshake timeout");
                    let _ = conn.shutdown().await;
                    opts.metrics.dec_active();
                    return;
                }
            }
        } else {
            match read_fut.await {
                Ok(h) => h,
                Err(e) => {
                    tracing::warn!(sid=%sid, client=%client, parser=%opts.parser.name(), err=%e, "proxy: routing header parse failed");
                    let _ = conn.shutdown().await;
                    opts.metrics.dec_active();
                    return;
                }
            }
        }
    };

    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        let _ = conn.shutdown().await;
        opts.metrics.dec_active();
        return;
    }

    let Some(res) = opts.router.resolve(&host) else {
        tracing::debug!(sid=%sid, client=%client, host=%host, "proxy: no route for host");
        let _ = conn.shutdown().await;
        opts.metrics.dec_active();
        return;
    };

    opts.metrics.add_route_hit(&host);

    let default_port = mc_handshake_port(&captured)
        .or_else(|| conn.local_addr().ok().map(|a| a.port()))
        .unwrap_or(25565);

    // Dial upstream candidates with failover.
    let mut last_err: Option<anyhow::Error> = None;
    let mut upstream_used = String::new();
    let mut up_conn: Option<tunnel::transport::BoxedStream> = None;

    for cand in &res.upstreams {
        let addr = cand.trim().to_string();
        match dial_upstream(
            &addr,
            Some(default_port),
            opts.upstream_dial_timeout,
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
        opts.metrics.dec_active();
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
    if let Err(err) = (&mut *up).write_all(&captured).await {
        tracing::debug!(sid=%sid, err=%err, "proxy: failed writing prelude to upstream");
        let _ = conn.shutdown().await;
        opts.sessions.remove(&sid);
        opts.metrics.dec_active();
        return;
    }

    let res = proxy_bidirectional(&mut conn, up, opts.buffer_size, opts.idle_timeout).await;

    opts.sessions.remove(&sid);
    opts.metrics.dec_active();

    match res {
        Ok((ingress, egress)) => {
            opts.metrics.add_bytes(ingress, egress);
        }
        Err(err) => {
            tracing::debug!(sid=%sid, err=%err, "proxy: session ended with error");
        }
    }
}

async fn dial_tcp_stream(addr: &str, timeout: Duration) -> anyhow::Result<tunnel::transport::BoxedStream> {
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
        let mgr = tunnel_manager.context("tunnel upstream requested but tunnel manager is not configured")?;
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
