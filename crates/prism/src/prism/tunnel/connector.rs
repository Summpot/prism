//! Connector host implementation for Prism tunnel mode.
//!
//! Connects to a remote Prism server, registers published services, and handles
//! incoming proxy streams from the server (either plain TCP/UDP or through the
//! Native Traffic Optimizer pipeline).

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::io::{AsyncReadExt, AsyncWriteExt};

use crate::prism::middleware::{
    FramePriority, PollResult, SessionHandle, SessionState, StreamResult, WasmMiddleware,
    WasmProtocolSession,
};
use crate::prism::tunnel::{
    optimizer::{
        self, BatcherConfig, CompressorConfig, DecompressorConfig, OptimizedReader,
        OptimizedWriter, TrafficDirection,
    },
    protocol::{self, ProxyStreamKind, RegisterRequest, RegisteredService},
    transport::{BoxedStream, TransportDialOptions},
};

#[derive(Debug, Clone, Default)]
pub struct QuicConnectorOptions {
    pub server_name: String,
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WebSocketConnectorOptions {
    pub server_name: String,
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone)]
pub struct ConnectorOptions {
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub services: Vec<RegisteredService>,
    pub dial_timeout: Duration,
    pub quic: QuicConnectorOptions,
    pub websocket: WebSocketConnectorOptions,
    pub middleware_dir: Option<PathBuf>,
    pub optimizer: Option<crate::prism::telemetry::SharedOptimizerRegistry>,
    pub sessions: Option<crate::prism::telemetry::SharedSessions>,
    pub doh_servers: Vec<String>,
}

pub struct Connector {
    opts: ConnectorOptions,
    local_map: Arc<HashMap<String, RegisteredService>>,
}

impl Connector {
    pub fn new(mut opts: ConnectorOptions) -> anyhow::Result<Self> {
        if opts.dial_timeout <= Duration::from_millis(0) {
            opts.dial_timeout = Duration::from_secs(5);
        }

        let mut map = HashMap::new();
        let mut svcs = Vec::new();
        for s in opts.services.drain(..) {
            let Some(ns) = s.normalize() else {
                continue;
            };
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

    pub async fn run(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        if self.opts.server_addr.trim().is_empty() {
            anyhow::bail!("tunnel: connector server_addr is required");
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
                        "tunnel: connector disconnected; retrying"
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

    pub async fn run_once(
        &self,
        mut shutdown: tokio::sync::watch::Receiver<bool>,
    ) -> anyhow::Result<()> {
        let dial_opts = TransportDialOptions {
            quic: crate::prism::tunnel::transport::QuicDialOptions {
                server_name: self.opts.quic.server_name.clone(),
                insecure_skip_verify: self.opts.quic.insecure_skip_verify,
                next_protos: vec![],
            },
            websocket: crate::prism::tunnel::transport::WebSocketDialOptions {
                server_name: self.opts.websocket.server_name.clone(),
                insecure_skip_verify: self.opts.websocket.insecure_skip_verify,
            },
            webtransport: crate::prism::tunnel::transport::WebTransportDialOptions {
                server_name: self.opts.quic.server_name.clone(),
                insecure_skip_verify: self.opts.quic.insecure_skip_verify,
            },
        };

        let doh_refs: Vec<&str> = self.opts.doh_servers.iter().map(|s| s.as_str()).collect();
        let custom_doh = if doh_refs.is_empty() { None } else { Some(doh_refs.as_slice()) };
        let candidates = crate::prism::tunnel::negotiator::resolve_candidates(
            &self.opts.server_addr,
            Some(&self.opts.transport),
            custom_doh,
        )
        .await?;

        let (sess, chosen) = crate::prism::tunnel::negotiator::dial_with_fallback(
            &candidates,
            self.opts.dial_timeout,
            &dial_opts,
        )
        .await?;

        // Register on first stream
        let mut reg = sess.open_stream().await?;
        let req = RegisterRequest {
            client_type: "connector".into(),
            token: self.opts.auth_token.clone(),
            services: self.opts.services.clone(),
        };
        protocol::write_register_request(&mut reg, &req).await?;
        reg.shutdown().await?;

        tracing::info!(
            transport = %chosen.protocol,
            port = chosen.port,
            server = %self.opts.server_addr,
            services = self.opts.services.len(),
            "tunnel: connector registered"
        );

        // Accept proxy streams
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
                    let mw_dir = self.opts.middleware_dir.clone();
                    let optimizer = self.opts.optimizer.clone();
                    let sessions = self.opts.sessions.clone();
                    tokio::spawn(async move {
                        if let Err(err) = handle_stream(map, mw_dir, optimizer, sessions, st).await {
                            tracing::debug!(err=%err, "tunnel: connector stream ended");
                        }
                    });
                }
            }
        }
    }
}

pub async fn handle_stream(
    local_map: Arc<HashMap<String, RegisteredService>>,
    middleware_dir: Option<PathBuf>,
    optimizer: Option<crate::prism::telemetry::SharedOptimizerRegistry>,
    sessions: Option<crate::prism::telemetry::SharedSessions>,
    mut st: BoxedStream,
) -> anyhow::Result<()> {
    let (kind, svc, flags) = protocol::read_proxy_stream_header_with_flags(&mut st).await?;
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
            let local_sock = tokio::net::TcpStream::connect(&local).await?;
            let optimizer_enabled = (flags & protocol::FLAG_OPTIMIZER != 0)
                || meta.optimizer.as_ref().is_some_and(|to| to.enabled);

            let sid = crate::prism::telemetry::new_session_id();
            let session_stats = sessions.as_ref().and_then(|reg| {
                reg.track(
                    crate::prism::telemetry::SessionInfo::new(
                        sid.clone(),
                        String::new(),
                        meta.name.clone(),
                        local.clone(),
                    ),
                    optimizer_enabled,
                )
            });

            let res = if optimizer_enabled {
                crate::prism::net::set_nodelay(&local_sock);
                run_optimized_tcp_pipeline(
                    st,
                    local_sock,
                    meta,
                    middleware_dir.as_deref(),
                    optimizer,
                    session_stats,
                )
                .await
            } else {
                let mut up = local_sock;
                let mut st = st;
                let _ = tokio::io::copy_bidirectional(&mut st, &mut up).await;
                Ok(())
            };

            if let Some(ref sessions) = sessions {
                sessions.remove(&sid);
            }
            res?;
        }
        ProxyStreamKind::Udp => {
            handle_udp_stream(st, &local).await?;
        }
    }

    Ok(())
}

pub async fn run_optimized_tcp_pipeline(
    st: BoxedStream,
    local_sock: tokio::net::TcpStream,
    meta: RegisteredService,
    middleware_dir: Option<&Path>,
    optimizer: Option<crate::prism::telemetry::SharedOptimizerRegistry>,
    session_stats: Option<optimizer::SharedOptimizerStats>,
) -> anyhow::Result<()> {
    crate::prism::net::set_nodelay(&local_sock);
    let mut opt_cfg = meta
        .optimizer
        .as_ref()
        .map(optimizer::OptimizerConfig::from)
        .unwrap_or(optimizer::OptimizerConfig {
            enabled: true,
            ..Default::default()
        });
    opt_cfg.enabled = true;
    if opt_cfg.dictionary.is_none() {
        opt_cfg.dictionary = optimizer::resolve_dictionary(None, &meta.name);
    }
    let stats = crate::prism::telemetry::optimizer_collectors(
        optimizer.as_ref(),
        &meta.name,
        session_stats,
    );
    return optimizer::pipeline::run_upstream_facing(
        st,
        local_sock,
        opt_cfg,
        meta.middleware,
        middleware_dir.map(Path::to_path_buf),
        stats,
        meta.name,
    )
    .await;

    #[allow(unreachable_code)]
    let (st_read, st_write) = tokio::io::split(st);
    #[allow(unused_variables, unused_mut)]
    let (mut local_read, mut local_write) = local_sock.into_split();

    let (flush_interval, window_log, zstd_level) = if let Some(ref to) = meta.optimizer {
        (
            Duration::from_millis(to.flush_interval_ms()),
            to.zstd_window_log(),
            to.zstd_level(),
        )
    } else {
        (
            optimizer::DEFAULT_FLUSH_INTERVAL,
            optimizer::DEFAULT_ZSTD_WINDOW_LOG,
            optimizer::DEFAULT_ZSTD_LEVEL,
        )
    };

    // Inbound: PRPX stream -> OptimizedReader -> local_write
    let decompressor_config = DecompressorConfig {
        window_log,
        dictionary: None,
    };
    let mut opt_reader = OptimizedReader::new(st_read, decompressor_config)?
        .with_direction(TrafficDirection::Uplink);
    if let Some(ref optimizer) = optimizer {
        opt_reader.add_stats(optimizer.service(&meta.name));
        opt_reader.add_stats(optimizer.global());
    }
    let inbound_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = opt_reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            local_write.write_all(&buf[..n]).await?;
        }
        local_write.shutdown().await?;
        Ok::<(), anyhow::Error>(())
    });

    // Outbound: local_read -> WasmProtocolSession (if configured) -> OptimizedWriter -> st_write
    let batcher_config = BatcherConfig {
        flush_interval,
        buffer_threshold: optimizer::DEFAULT_BUFFER_THRESHOLD,
    };
    let compressor_config = CompressorConfig {
        compression_level: zstd_level,
        window_log,
        dictionary: None,
    };
    let mut opt_writer = OptimizedWriter::new(st_write, batcher_config, compressor_config)?
        .with_direction(TrafficDirection::Downlink);
    if let Some(ref optimizer) = optimizer {
        opt_writer.add_stats(optimizer.service(&meta.name));
        opt_writer.add_stats(optimizer.global());
    }
    let wasm_session = load_wasm_session(meta.middleware.as_deref(), middleware_dir)?;

    let outbound_task = tokio::spawn(async move {
        let mut read_buf = Vec::with_capacity(64 * 1024);
        let mut tmp = [0u8; 8192];

        loop {
            let flush_dur = opt_writer.time_until_flush();
            tokio::select! {
                res = local_read.read(&mut tmp) => {
                    let n = res?;
                    if n == 0 {
                        // EOF on local socket: flush remaining bytes
                        if !read_buf.is_empty() {
                            opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                            read_buf.clear();
                        }
                        opt_writer.flush_batch().await?;
                        opt_writer.shutdown().await?;
                        break;
                    }
                    read_buf.extend_from_slice(&tmp[..n]);

                    if let Some(ref sess_handle) = wasm_session {
                        let mut offset = 0;
                        while offset < read_buf.len() {
                            let slice = &read_buf[offset..];
                            let poll_res = {
                                let mut sess = sess_handle.lock().unwrap();
                                sess.poll(slice)
                            };
                            match poll_res {
                                Ok(PollResult::Stream(StreamResult::Frame {
                                    len,
                                    priority,
                                    payload,
                                })) => {
                                    if len == 0 || len > slice.len() {
                                        // Need more data for a full frame
                                        break;
                                    }
                                    if let Some(ref payload) = payload {
                                        opt_writer
                                            .write_frame_with_metric(len, payload, priority)
                                            .await?;
                                    } else {
                                        opt_writer.write_frame(&slice[..len], priority).await?;
                                    }
                                    offset += len;
                                }
                                Ok(PollResult::Stream(StreamResult::NeedMoreData))
                                | Ok(PollResult::Stream(StreamResult::Blocked)) => {
                                    break;
                                }
                                Ok(PollResult::Handshake(_)) => {
                                    opt_writer.write_frame(slice, FramePriority::Defer).await?;
                                    offset += slice.len();
                                    break;
                                }
                                Err(err) => {
                                    tracing::warn!(err=%err, "middleware poll failed; writing defer");
                                    opt_writer.write_frame(slice, FramePriority::Defer).await?;
                                    offset += slice.len();
                                    break;
                                }
                            }
                        }
                        if offset > 0 {
                            read_buf.drain(..offset);
                        }
                    } else {
                        opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                        read_buf.clear();
                    }
                }
                _ = async {
                    if let Some(dur) = flush_dur {
                        tokio::time::sleep(dur).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    opt_writer.flush_if_due().await?;
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    let mut in_task = inbound_task;
    let mut out_task = outbound_task;
    tokio::select! {
        res = &mut in_task => {
            out_task.abort();
            let _ = out_task.await;
            res??;
        }
        res = &mut out_task => {
            in_task.abort();
            let _ = in_task.await;
            res??;
        }
    }
    Ok(())
}

/// Runs the Server-side Native Traffic Optimizer pipeline.
///
/// Bridges an external raw TCP client (e.g. Minecraft player) with a tunnel
/// PRPX stream connected to a Connector.
///
/// - Inbound (Connector -> Server -> Player): `OptimizedReader` (direction Downlink)
///   reads compressed PRPX chunks and writes decompressed bytes to `client_sock`.
/// - Outbound (Player -> Server -> Connector): `OptimizedWriter` (direction Uplink)
///   batches and compresses player bytes (after optional WASM priority sniffing)
///   into compressed PRPX chunks sent to `st`.
pub async fn run_server_optimized_tcp_pipeline(
    st: BoxedStream,
    client_sock: tokio::net::TcpStream,
    initial_bytes: &[u8],
    meta: RegisteredService,
    middleware_dir: Option<&Path>,
    optimizer: Option<crate::prism::telemetry::SharedOptimizerRegistry>,
    idle_timeout: Duration,
    session_stats: Option<optimizer::SharedOptimizerStats>,
) -> anyhow::Result<()> {
    crate::prism::net::set_nodelay(&client_sock);
    let mut opt_cfg = meta
        .optimizer
        .as_ref()
        .map(optimizer::OptimizerConfig::from)
        .unwrap_or(optimizer::OptimizerConfig {
            enabled: true,
            ..Default::default()
        });
    opt_cfg.enabled = true;
    if opt_cfg.dictionary.is_none() {
        opt_cfg.dictionary = optimizer::resolve_dictionary(None, &meta.name);
    }
    let stats = crate::prism::telemetry::optimizer_collectors(
        optimizer.as_ref(),
        &meta.name,
        session_stats,
    );
    return optimizer::pipeline::run_player_facing(
        st,
        client_sock,
        opt_cfg,
        meta.middleware,
        middleware_dir.map(Path::to_path_buf),
        stats,
        initial_bytes.to_vec(),
        idle_timeout,
        meta.name,
        optimizer::pipeline::ParamsRole::Opener,
    )
    .await;

    #[allow(unreachable_code, unused_variables, unused_mut, unused_assignments)]
    let (st_read, st_write) = tokio::io::split(st);
    let (mut client_read, mut client_write) = client_sock.into_split();

    let (flush_interval, window_log, zstd_level) = if let Some(ref to) = meta.optimizer {
        (
            Duration::from_millis(to.flush_interval_ms()),
            to.zstd_window_log(),
            to.zstd_level(),
        )
    } else {
        (
            optimizer::DEFAULT_FLUSH_INTERVAL,
            optimizer::DEFAULT_ZSTD_WINDOW_LOG,
            optimizer::DEFAULT_ZSTD_LEVEL,
        )
    };

    // Inbound: PRPX stream (Connector) -> OptimizedReader (Downlink) -> client_write (Player)
    let decompressor_config = DecompressorConfig {
        window_log,
        dictionary: None,
    };
    let mut opt_reader = OptimizedReader::new(st_read, decompressor_config)?
        .with_direction(TrafficDirection::Downlink);
    if let Some(ref optimizer) = optimizer {
        opt_reader.add_stats(optimizer.service(&meta.name));
        opt_reader.add_stats(optimizer.global());
    }
    let inbound_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 64 * 1024];
        loop {
            let n = opt_reader.read(&mut buf).await?;
            if n == 0 {
                break;
            }
            client_write.write_all(&buf[..n]).await?;
        }
        client_write.shutdown().await?;
        Ok::<(), anyhow::Error>(())
    });

    // Outbound: client_read (Player) -> WasmProtocolSession (sniff keepalive) -> OptimizedWriter (Uplink) -> st_write (Connector)
    let batcher_config = BatcherConfig {
        flush_interval,
        buffer_threshold: optimizer::DEFAULT_BUFFER_THRESHOLD,
    };
    let compressor_config = CompressorConfig {
        compression_level: zstd_level,
        window_log,
        dictionary: None,
    };
    let mut opt_writer = OptimizedWriter::new(st_write, batcher_config, compressor_config)?
        .with_direction(TrafficDirection::Uplink);
    if let Some(ref optimizer) = optimizer {
        opt_writer.add_stats(optimizer.service(&meta.name));
        opt_writer.add_stats(optimizer.global());
    }

    if !initial_bytes.is_empty() {
        opt_writer.write_frame(initial_bytes, FramePriority::Defer).await?;
        opt_writer.flush_batch().await?;
    }

    let wasm_session = load_wasm_session(meta.middleware.as_deref(), middleware_dir)?;

    let outbound_task = tokio::spawn(async move {
        let mut read_buf = Vec::with_capacity(64 * 1024);
        let mut tmp = [0u8; 8192];

        loop {
            let flush_dur = opt_writer.time_until_flush();
            tokio::select! {
                res = client_read.read(&mut tmp) => {
                    let n = res?;
                    if n == 0 {
                        // EOF on client socket: flush remaining bytes
                        if !read_buf.is_empty() {
                            opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                            read_buf.clear();
                        }
                        opt_writer.flush_batch().await?;
                        opt_writer.shutdown().await?;
                        break;
                    }
                    read_buf.extend_from_slice(&tmp[..n]);

                    if let Some(ref sess_handle) = wasm_session {
                        let mut offset = 0;
                        while offset < read_buf.len() {
                            let slice = &read_buf[offset..];
                            let poll_res = {
                                let mut sess = sess_handle.lock().unwrap();
                                sess.poll(slice)
                            };
                            match poll_res {
                                Ok(PollResult::Stream(StreamResult::Frame {
                                    len,
                                    priority,
                                    payload,
                                })) => {
                                    if len == 0 || len > slice.len() {
                                        break;
                                    }
                                    if let Some(ref payload) = payload {
                                        opt_writer
                                            .write_frame_with_metric(len, payload, priority)
                                            .await?;
                                    } else {
                                        opt_writer.write_frame(&slice[..len], priority).await?;
                                    }
                                    offset += len;
                                }
                                Ok(PollResult::Stream(StreamResult::NeedMoreData))
                                | Ok(PollResult::Stream(StreamResult::Blocked)) => {
                                    break;
                                }
                                Ok(PollResult::Handshake(_)) => {
                                    opt_writer.write_frame(slice, FramePriority::Defer).await?;
                                    offset += slice.len();
                                    break;
                                }
                                Err(err) => {
                                    tracing::warn!(err=%err, "middleware poll failed; writing defer");
                                    opt_writer.write_frame(slice, FramePriority::Defer).await?;
                                    offset += slice.len();
                                    break;
                                }
                            }
                        }
                        if offset > 0 {
                            read_buf.drain(..offset);
                        }
                    } else {
                        opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                        read_buf.clear();
                    }
                }
                _ = async {
                    if let Some(dur) = flush_dur {
                        tokio::time::sleep(dur).await;
                    } else {
                        std::future::pending::<()>().await;
                    }
                } => {
                    opt_writer.flush_if_due().await?;
                }
            }
        }
        Ok::<(), anyhow::Error>(())
    });

    let copy_fut = async {
        let mut in_task = inbound_task;
        let mut out_task = outbound_task;
        tokio::select! {
            res = &mut in_task => {
                out_task.abort();
                let _ = out_task.await;
                res??;
            }
            res = &mut out_task => {
                in_task.abort();
                let _ = in_task.await;
                res??;
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    if idle_timeout > Duration::from_millis(0) {
        tokio::time::timeout(idle_timeout, copy_fut)
            .await
            .map_err(|_| anyhow::anyhow!("idle timeout"))??;
    } else {
        copy_fut.await?;
    }

    Ok(())
}

pub fn load_wasm_session(
    mw_name: Option<&str>,
    middleware_dir: Option<&Path>,
) -> anyhow::Result<Option<SessionHandle>> {
    let Some(name) = mw_name else {
        return Ok(None);
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }

    let base_name = name.strip_suffix(".wat").unwrap_or(name);

    let mut sess = if let Some(dir) = middleware_dir {
        let candidates = [dir.join(name), dir.join(format!("{base_name}.wat"))];
        let mut found = None;
        for path in &candidates {
            if path.exists() {
                let mw = WasmMiddleware::from_wat_path(base_name, path)?;
                found = Some(mw.create_shared_session()?);
                break;
            }
        }
        found
    } else {
        None
    };

    if sess.is_none() {
        let path = Path::new(name);
        if path.exists() {
            let mw = WasmMiddleware::from_wat_path(base_name, path)?;
            sess = Some(mw.create_shared_session()?);
        }
    }

    if sess.is_none() {
        if let Some(wat) = crate::prism::middleware::get_default_middleware_wat(base_name) {
            let s = WasmProtocolSession::from_wat(wat)?;
            sess = Some(s.into_shared(base_name));
        }
    }

    let Some(sess) = sess else {
        anyhow::bail!("middleware not found: {name}");
    };

    {
        let mut s = sess.lock().unwrap();
        s.set_state(SessionState::Streaming);
        if let Some(data) = crate::prism::middleware::get_injected_middleware_data(base_name, None)
        {
            let _ = s.set_data(&data);
        }
        if let Some(config) = crate::prism::middleware::get_dynamic_middleware_config(base_name) {
            let _ = s.apply_config_map(&config);
        }
    }

    Ok(Some(sess))
}

async fn handle_udp_stream(st: BoxedStream, local: &str) -> anyhow::Result<()> {
    let sock = tokio::net::UdpSocket::bind("0.0.0.0:0").await?;
    sock.connect(local).await?;

    let sock = Arc::new(sock);
    let (mut rd, mut wr) = tokio::io::split(st);

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
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use tokio::net::TcpListener;
    use tokio::sync::mpsc;

    struct MockSession {
        accept_rx: tokio::sync::Mutex<mpsc::Receiver<BoxedStream>>,
        open_tx: tokio::sync::Mutex<Option<mpsc::Sender<BoxedStream>>>,
    }

    #[async_trait::async_trait]
    impl crate::prism::tunnel::transport::TransportSession for MockSession {
        async fn open_stream(&self) -> anyhow::Result<BoxedStream> {
            let (c, s) = tokio::io::duplex(64 * 1024);
            if let Some(tx) = self.open_tx.lock().await.as_ref() {
                tx.send(Box::new(s))
                    .await
                    .map_err(|e| anyhow::anyhow!("{e}"))?;
            }
            Ok(Box::new(c))
        }

        async fn accept_stream(&self) -> anyhow::Result<BoxedStream> {
            self.accept_rx
                .lock()
                .await
                .recv()
                .await
                .ok_or_else(|| anyhow::anyhow!("closed"))
        }

        async fn close(&self) {}
        fn remote_addr(&self) -> Option<SocketAddr> {
            None
        }
        fn local_addr(&self) -> Option<SocketAddr> {
            None
        }
    }

    #[tokio::test]
    async fn connector_plain_stream_copies_bidirectionally() {
        // Echo TCP server
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 128];
                if let Ok(n) = sock.read(&mut buf).await {
                    let _ = sock.write_all(&buf[..n]).await;
                }
            }
        });

        let mut map = HashMap::new();
        map.insert(
            "echo-svc".to_string(),
            RegisteredService {
                name: "echo-svc".into(),
                proto: "tcp".into(),
                local_addr,
                route_only: false,
                remote_addr: "".into(),
                masquerade_host: "".into(),
                middleware: None,
                optimizer: None,
            },
        );
        let local_map = Arc::new(map);

        let (mut client_st, server_st) = tokio::io::duplex(4096);
        let h =
            tokio::spawn(
                async move { handle_stream(local_map, None, None, None, Box::new(server_st)).await },
            );

        // Write header without flags
        protocol::write_proxy_stream_header(&mut client_st, ProxyStreamKind::Tcp, "echo-svc")
            .await
            .unwrap();

        client_st.write_all(b"PING").await.unwrap();
        let mut reply = [0u8; 4];
        client_st.read_exact(&mut reply).await.unwrap();
        assert_eq!(&reply, b"PING");

        drop(client_st);
        let _ = h.await;
    }

    #[tokio::test]
    async fn connector_optimized_pipeline_decompresses_and_classifies() {
        // Echo server that immediately replies with packet
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let local_addr = listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = listener.accept().await {
                let mut buf = [0u8; 128];
                let n = sock.read(&mut buf).await.unwrap();
                // Send reply with first byte = 1 (urgent) for testing wasm classification
                let mut reply = vec![1u8];
                reply.extend_from_slice(&buf[..n]);
                sock.write_all(&reply).await.unwrap();
            }
        });

        let mut map = HashMap::new();
        map.insert(
            "mc-svc".to_string(),
            RegisteredService {
                name: "mc-svc".into(),
                proto: "tcp".into(),
                local_addr,
                route_only: false,
                remote_addr: "".into(),
                masquerade_host: "".into(),
                middleware: None,
                optimizer: Some(crate::prism::config::OptimizerConfig {
                    enabled: true,
                    flush_interval_ms: Some(20),
                    zstd_window_log: Some(23),
                    zstd_level: Some(3),
                    ..Default::default()
                }),
            },
        );
        let local_map = Arc::new(map);

        let (client_st, server_st) = tokio::io::duplex(64 * 1024);
        let (mut client_read, mut client_write) = tokio::io::split(client_st);

        let h =
            tokio::spawn(
                async move { handle_stream(local_map, None, None, None, Box::new(server_st)).await },
            );

        // Write PRPX header with FLAG_OPTIMIZER
        protocol::write_proxy_stream_header_with_flags(
            &mut client_write,
            ProxyStreamKind::Tcp,
            "mc-svc",
            protocol::FLAG_OPTIMIZER,
        )
        .await
        .unwrap();

        let local_params = protocol::OptimizerStreamParams {
            encode_window_log: 23,
            decode_window_log: 23,
            dict_id: 0,
            dictionary: None,
        };
        protocol::write_optimizer_stream_params(&mut client_write, &local_params)
            .await
            .unwrap();
        let _peer = protocol::read_optimizer_stream_params(&mut client_read)
            .await
            .unwrap();

        // Write compressed data using OptimizedWriter
        let mut client_opt_writer = OptimizedWriter::with_defaults(&mut client_write).unwrap();
        client_opt_writer
            .write_frame(b"HELLO_MC", FramePriority::Urgent)
            .await
            .unwrap();

        // Read compressed reply using OptimizedReader
        let mut client_opt_reader = OptimizedReader::with_defaults(&mut client_read).unwrap();
        let mut reply_buf = [0u8; 9];
        client_opt_reader.read_exact(&mut reply_buf).await.unwrap();

        assert_eq!(reply_buf[0], 1u8); // prefix added by server
        assert_eq!(&reply_buf[1..], b"HELLO_MC");

        drop(client_opt_writer);
        drop(client_opt_reader);
        let _ = client_write.shutdown().await;
        drop(client_write);
        drop(client_read);
        let _ = tokio::time::timeout(Duration::from_secs(2), h).await;
    }

    #[tokio::test]
    async fn test_server_to_connector_optimized_pipeline() {
        // 1. Backend echo TCP server representing local Minecraft server
        let backend_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let backend_addr = backend_listener.local_addr().unwrap().to_string();

        tokio::spawn(async move {
            if let Ok((mut sock, _)) = backend_listener.accept().await {
                let mut buf = [0u8; 1024];
                while let Ok(n) = sock.read(&mut buf).await {
                    if n == 0 {
                        break;
                    }
                    let mut reply = b"ECHO: ".to_vec();
                    reply.extend_from_slice(&buf[..n]);
                    if sock.write_all(&reply).await.is_err() {
                        break;
                    }
                }
            }
        });

        // 2. Connector configuration
        let svc_name = "mc-opt-pipeline";
        let meta = RegisteredService {
            name: svc_name.into(),
            proto: "tcp".into(),
            local_addr: backend_addr,
            route_only: false,
            remote_addr: "".into(),
            masquerade_host: "".into(),
            middleware: None,
            optimizer: Some(crate::prism::config::OptimizerConfig {
                enabled: true,
                flush_interval_ms: Some(20),
                zstd_window_log: Some(23),
                zstd_level: Some(3),
                ..Default::default()
            }),
        };
        let mut map = HashMap::new();
        map.insert(svc_name.to_string(), meta.clone());
        let local_map = Arc::new(map);

        // 3. Duplex stream representing the tunnel PRPX stream between Server and Connector
        let (server_tunnel_st, connector_tunnel_st) = tokio::io::duplex(64 * 1024);

        // Spawn connector stream handler with a session registry so per-stream
        // optimizer stats are visible the same way the admin connections page reads them.
        let connector_sessions = Arc::new(crate::prism::telemetry::SessionRegistry::new());
        let connector_sessions_clone = connector_sessions.clone();
        let connector_handle = tokio::spawn(async move {
            handle_stream(
                local_map,
                None,
                None,
                Some(connector_sessions_clone),
                Box::new(connector_tunnel_st),
            )
            .await
        });

        // Server writes PRPX header with FLAG_OPTIMIZER
        let mut server_tunnel_st = server_tunnel_st;
        protocol::write_proxy_stream_header_with_flags(
            &mut server_tunnel_st,
            ProxyStreamKind::Tcp,
            svc_name,
            protocol::FLAG_OPTIMIZER,
        )
        .await
        .unwrap();

        // 4. Mock Player raw TCP connection to the Server
        let player_listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let player_addr = player_listener.local_addr().unwrap();

        let player_sock = tokio::net::TcpStream::connect(player_addr).await.unwrap();
        let (server_client_sock, _) = player_listener.accept().await.unwrap();

        // 5. Server runs run_server_optimized_tcp_pipeline
        let opt_registry = Arc::new(crate::prism::telemetry::OptimizerStatsRegistry::new());
        let server_sessions = Arc::new(crate::prism::telemetry::SessionRegistry::new());
        let prelude = b"MINECRAFT_PRELUDE_";
        let session_stats = server_sessions.track(
            crate::prism::telemetry::SessionInfo::new(
                crate::prism::telemetry::new_session_id(),
                "player".into(),
                svc_name.into(),
                "echo".into(),
            ),
            true,
        );

        let opt_clone = opt_registry.clone();
        let server_handle = tokio::spawn(async move {
            run_server_optimized_tcp_pipeline(
                Box::new(server_tunnel_st),
                server_client_sock,
                prelude,
                meta,
                None,
                Some(opt_clone),
                Duration::ZERO,
                session_stats,
            )
            .await
        });

        let mut player_sock = player_sock;

        // 6. Player receives echo of prelude
        let mut prelude_reply = vec![0u8; b"ECHO: MINECRAFT_PRELUDE_".len()];
        player_sock.read_exact(&mut prelude_reply).await.unwrap();
        assert_eq!(&prelude_reply, b"ECHO: MINECRAFT_PRELUDE_");

        // 7. Player sends game payload
        player_sock.write_all(b"PLAYER_DATA").await.unwrap();

        // 8. Player receives echo of player data
        let mut payload_reply = vec![0u8; b"ECHO: PLAYER_DATA".len()];
        player_sock.read_exact(&mut payload_reply).await.unwrap();
        assert_eq!(&payload_reply, b"ECHO: PLAYER_DATA");

        // 9. Verify stats were recorded in the registry and on the live sessions
        let (global_stats, service_stats) = opt_registry.snapshot();
        assert!(global_stats.raw_bytes > 0);
        assert!(service_stats.contains_key(svc_name));
        let server_conns = server_sessions.snapshot();
        assert_eq!(server_conns.len(), 1);
        assert!(
            server_conns[0].raw_bytes > 0,
            "server-side optimizer session should record raw_bytes"
        );
        let connector_conns = connector_sessions.snapshot();
        assert_eq!(connector_conns.len(), 1);
        assert!(
            connector_conns[0].raw_bytes > 0,
            "connector-side optimizer session should record raw_bytes"
        );

        drop(player_sock);
        let _ = tokio::time::timeout(Duration::from_secs(2), server_handle).await;
        let _ = tokio::time::timeout(Duration::from_secs(2), connector_handle).await;
    }
}
