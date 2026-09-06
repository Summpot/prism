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
    FramePriority, PollResult, SessionState, StreamResult, WasmMiddleware, WasmProtocolSession,
    frame_uncompressed_packet,
};
use crate::prism::tunnel::{
    optimizer::{
        self, BatcherConfig, CompressorConfig, DecompressorConfig, OptimizedReader,
        OptimizedWriter, TrafficDirection,
    },
    protocol::{self, ProxyStreamKind, RegisterRequest, RegisteredService},
    transport::{BoxedStream, TransportDialOptions, transport_by_name},
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
                    websocket: crate::prism::tunnel::transport::WebSocketDialOptions {
                        server_name: self.opts.websocket.server_name.clone(),
                        insecure_skip_verify: self.opts.websocket.insecure_skip_verify,
                    },
                },
            )
            .await
        };

        let sess = tokio::time::timeout(self.opts.dial_timeout, dial).await??;

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
            transport=%tr.name(),
            server=%self.opts.server_addr,
            services=self.opts.services.len(),
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
                    tokio::spawn(async move {
                        if let Err(err) = handle_stream(map, mw_dir, optimizer, st).await {
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

            if optimizer_enabled {
                run_optimized_tcp_pipeline(
                    st,
                    local_sock,
                    meta,
                    middleware_dir.as_deref(),
                    optimizer,
                )
                .await?;
            } else {
                let mut up = local_sock;
                let mut st = st;
                let _ = tokio::io::copy_bidirectional(&mut st, &mut up).await;
            }
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
) -> anyhow::Result<()> {
    let (st_read, st_write) = tokio::io::split(st);
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
    let decompressor_config = DecompressorConfig { window_log };
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
    };
    let mut opt_writer = OptimizedWriter::new(st_write, batcher_config, compressor_config)?
        .with_direction(TrafficDirection::Downlink);
    if let Some(ref optimizer) = optimizer {
        opt_writer.add_stats(optimizer.service(&meta.name));
        opt_writer.add_stats(optimizer.global());
    }
    let mut wasm_session = load_wasm_session(meta.middleware.as_deref(), middleware_dir)?;

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

                    if let Some(ref mut sess) = wasm_session {
                        let mut offset = 0;
                        while offset < read_buf.len() {
                            let slice = &read_buf[offset..];
                            match sess.poll(slice) {
                                Ok(PollResult::Stream(StreamResult::Frame {
                                    len,
                                    priority,
                                    payload,
                                })) => {
                                    if len == 0 || len > slice.len() {
                                        // Need more data for a full frame
                                        break;
                                    }
                                    if let Some(ref decompressed) = payload {
                                        let framed = frame_uncompressed_packet(decompressed);
                                        opt_writer
                                            .write_frame_with_metric(len, &framed, priority)
                                            .await?;
                                    } else {
                                        opt_writer.write_frame(&slice[..len], priority).await?;
                                    }
                                    offset += len;
                                }
                                Ok(PollResult::Stream(StreamResult::NeedMoreData)) => {
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

pub fn load_wasm_session(
    mw_name: Option<&str>,
    middleware_dir: Option<&Path>,
) -> anyhow::Result<Option<WasmProtocolSession>> {
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
                found = Some(mw.create_session()?);
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
            sess = Some(mw.create_session()?);
        }
    }

    if sess.is_none() {
        if let Some(wat) = crate::prism::middleware::get_default_middleware_wat(base_name) {
            sess = Some(WasmProtocolSession::from_wat(wat)?);
        }
    }

    let Some(mut sess) = sess else {
        anyhow::bail!("middleware not found: {name}");
    };

    sess.set_state(SessionState::Streaming);
    if let Some(data) = crate::prism::middleware::get_injected_middleware_data(base_name, None) {
        let _ = sess.set_data(&data);
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
                async move { handle_stream(local_map, None, None, Box::new(server_st)).await },
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
                middleware: Some("minecraft.wat".into()),
                optimizer: Some(crate::prism::config::OptimizerConfig {
                    enabled: true,
                    flush_interval_ms: Some(20),
                    zstd_window_log: Some(23),
                    zstd_level: Some(3),
                }),
            },
        );
        let local_map = Arc::new(map);

        let (client_st, server_st) = tokio::io::duplex(64 * 1024);
        let (mut client_read, mut client_write) = tokio::io::split(client_st);

        let h =
            tokio::spawn(
                async move { handle_stream(local_map, None, None, Box::new(server_st)).await },
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
}
