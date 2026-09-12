//! Unified bidirectional optimizer pipeline.
//!
//! Both the player-facing and upstream-facing sides of a tunnel stream run through
//! this module so batching, dictionaries, session-data control frames and WASM
//! ingress/egress stay in one place.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::sync::Notify;

use crate::prism::middleware::{
    FramePriority, PollResult, SessionHandle, SessionState, StreamResult, WasmProtocolSession,
};
use crate::prism::net;
use crate::prism::tunnel::optimizer::{
    self, BatcherConfig, CompressorConfig, DecompressorConfig, DictSampler, OptimizedReader,
    OptimizedWriter, SharedOptimizerStats, TrafficDirection, store_trained_dictionary,
};
use crate::prism::tunnel::protocol::{
    self, OptimizerStreamParams, decode_dictionary, encode_dictionary,
};
use crate::prism::tunnel::transport::BoxedStream;

/// Which side of the PRPX header this host is on (dictates params write/read order).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ParamsRole {
    /// Wrote the PRPX header; sends optimizer params first.
    Opener,
    /// Read the PRPX header; receives optimizer params first.
    Acceptor,
}

/// Whether `local` is the player socket or the upstream (origin) socket.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalRole {
    /// Local socket is the game client. Local→tunnel is uplink.
    Player,
    /// Local socket is the origin server. Local→tunnel is downlink.
    Upstream,
}

impl LocalRole {
    pub fn outbound_direction(self) -> TrafficDirection {
        match self {
            LocalRole::Player => TrafficDirection::Uplink,
            LocalRole::Upstream => TrafficDirection::Downlink,
        }
    }

    pub fn inbound_direction(self) -> TrafficDirection {
        match self {
            LocalRole::Player => TrafficDirection::Downlink,
            LocalRole::Upstream => TrafficDirection::Uplink,
        }
    }

    /// `true` when bytes read from `local` are origin→client (clientbound).
    pub fn local_reads_from_server(self) -> bool {
        matches!(self, LocalRole::Upstream)
    }

    /// `true` when bytes written to `local` are origin→client (clientbound).
    pub fn local_writes_to_client(self) -> bool {
        matches!(self, LocalRole::Player)
    }
}

pub struct PipelineOptions {
    pub optimizer: optimizer::OptimizerConfig,
    pub middleware: Option<String>,
    pub middleware_dir: Option<PathBuf>,
    pub stats: Vec<SharedOptimizerStats>,
    pub local_role: LocalRole,
    pub params_role: ParamsRole,
    pub initial_bytes: Vec<u8>,
    pub idle_timeout: Duration,
    pub service_name: String,
}

/// Runs the native optimizer between a tunnel stream and a local TCP socket.
pub async fn run(
    mut tunnel: BoxedStream,
    local: tokio::net::TcpStream,
    opts: PipelineOptions,
) -> anyhow::Result<()> {
    net::set_nodelay(&local);

    let local_id = opts.optimizer.dictionary_id();
    let local_params = OptimizerStreamParams {
        encode_window_log: opts
            .optimizer
            .window_log_for(opts.local_role.outbound_direction()) as u8,
        decode_window_log: opts
            .optimizer
            .window_log_for(opts.local_role.inbound_direction()) as u8,
        dict_id: local_id,
        dictionary: if local_id != 0 {
            opts.optimizer.dictionary.clone()
        } else {
            None
        },
    };

    let peer = match opts.params_role {
        ParamsRole::Opener => {
            protocol::exchange_optimizer_params_opener(&mut tunnel, &local_params).await?
        }
        ParamsRole::Acceptor => {
            protocol::exchange_optimizer_params_acceptor(&mut tunnel, &local_params).await?
        }
    };

    let encode_dict = encode_dictionary(opts.optimizer.dictionary.as_deref());
    let decode_dict = decode_dictionary(
        opts.optimizer.dictionary.as_deref(),
        local_id,
        &peer,
    );
    let encode_window = local_params.encode_window_log as u32;
    let decode_window = local_params.agreed_decode_window(peer.encode_window_log);

    let outbound_dir = opts.local_role.outbound_direction();
    let inbound_dir = opts.local_role.inbound_direction();

    let defer_flush = opts.optimizer.flush_interval_for(outbound_dir);
    let batcher_config = BatcherConfig {
        flush_interval: defer_flush,
        high_flush_interval: opts.optimizer.flush_interval_min,
        bulk_flush_interval: opts.optimizer.flush_interval_max.max(defer_flush),
        buffer_threshold: opts.optimizer.buffer_threshold_for(outbound_dir),
    };
    let compressor_config = CompressorConfig {
        compression_level: opts.optimizer.zstd_level,
        window_log: encode_window,
        dictionary: encode_dict,
    };
    let decompressor_config = DecompressorConfig {
        window_log: decode_window.max(encode_window),
        dictionary: decode_dict,
    };

    let skip_recompress = local
        .peer_addr()
        .map(|addr| addr.ip().is_loopback())
        .unwrap_or(false);

    let (st_read, st_write) = tokio::io::split(tunnel);
    let (local_read, local_write) = local.into_split();

    let mw_name = opts.middleware.clone();
    let mw_dir = opts.middleware_dir.clone();
    // One WASM instance per stream so handshake state, compression, and the AES
    // key are shared between ingress and egress. Direction is set on each poll.
    let wasm = load_session(mw_name.as_deref(), mw_dir.as_deref(), skip_recompress)?;
    let from_local = wasm.clone();
    let to_local = wasm;
    let from_server_out = opts.local_role.local_reads_from_server();
    let from_server_in = opts.local_role.local_writes_to_client();

    let (control_tx, mut control_rx) = tokio::sync::mpsc::unbounded_channel::<Vec<u8>>();
    let session_notify = Arc::new(Notify::new());

    let mut opt_reader = OptimizedReader::new(st_read, decompressor_config)?
        .with_direction(inbound_dir)
        .with_control_tx(control_tx);
    let mut opt_writer = OptimizedWriter::new(st_write, batcher_config, compressor_config)?
        .with_direction(outbound_dir)
        .with_adaptive_flush(
            opts.optimizer.adaptive_flush,
            defer_flush,
            opts.optimizer.flush_interval_min,
            opts.optimizer.flush_interval_max,
        );

    let record_raw_on_reader = to_local.is_none();
    for s in &opts.stats {
        opt_reader.add_stats(s.clone());
        opt_writer.add_stats(s.clone());
    }
    opt_reader.set_record_raw_metrics(record_raw_on_reader);

    let from_local_ctrl = from_local.clone();
    let notify_ctrl = session_notify.clone();
    tokio::spawn(async move {
        while let Some(data) = control_rx.recv().await {
            apply_session_data(&from_local_ctrl, &data);
            notify_ctrl.notify_waiters();
        }
    });

    let mut sampler = DictSampler::new();
    let service_name = opts.service_name.clone();
    let mut read_buf = opts.initial_bytes;
    if !read_buf.is_empty() {
        drain_wasm_frames(
            &from_local,
            &mut read_buf,
            &mut opt_writer,
            &session_notify,
            &mut sampler,
            from_server_out,
        )
        .await?;
        opt_writer.flush_batch().await?;
    }

    let inbound_stats = opts.stats.clone();
    let inbound = {
        let to_local = to_local.clone();
        let mut local_write = local_write;
        let session_notify = session_notify.clone();
        async move {
            let mut pending = Vec::new();
            let mut buf = vec![0u8; 64 * 1024];
            loop {
                tokio::select! {
                    res = opt_reader.read(&mut buf) => {
                        let n = res?;
                        if n == 0 {
                            break;
                        }
                        if to_local.is_none() {
                            local_write.write_all(&buf[..n]).await?;
                            continue;
                        }
                        pending.extend_from_slice(&buf[..n]);
                    }
                    _ = session_notify.notified() => {
                        if pending.is_empty() {
                            continue;
                        }
                    }
                }
                if let Some(ref sess) = to_local {
                    let written = process_egress_unlocked(
                        sess,
                        &mut pending,
                        &mut local_write,
                        from_server_in,
                    )
                    .await?;
                    if !record_raw_on_reader {
                        let now = optimizer::unix_ms();
                        for s in &inbound_stats {
                            s.add_direction_raw_bytes(inbound_dir, written as u64, now);
                        }
                    }
                }
            }
            if !pending.is_empty() {
                local_write.write_all(&pending).await?;
            }
            local_write.shutdown().await?;
            Ok::<(), anyhow::Error>(())
        }
    };

    let outbound = {
        let from_local = from_local.clone();
        let mut local_read = local_read;
        let session_notify = session_notify.clone();
        async move {
            let mut tmp = [0u8; 8192];
            loop {
                let flush_dur = opt_writer.time_until_flush();
                tokio::select! {
                    res = local_read.read(&mut tmp) => {
                        let n = res?;
                        if n == 0 {
                            if !read_buf.is_empty() {
                                maybe_sample(&mut sampler, &read_buf);
                                opt_writer.write_frame(&read_buf, FramePriority::Defer).await?;
                                read_buf.clear();
                            }
                            opt_writer.flush_batch().await?;
                            opt_writer.shutdown().await?;
                            break;
                        }
                        read_buf.extend_from_slice(&tmp[..n]);
                        drain_wasm_frames(
                            &from_local,
                            &mut read_buf,
                            &mut opt_writer,
                            &session_notify,
                            &mut sampler,
                            from_server_out,
                        ).await?;
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
                    _ = session_notify.notified() => {
                        drain_wasm_frames(
                            &from_local,
                            &mut read_buf,
                            &mut opt_writer,
                            &session_notify,
                            &mut sampler,
                            from_server_out,
                        ).await?;
                    }
                }
            }
            if let Some(dict) = sampler.finish() {
                store_trained_dictionary(&service_name, dict);
            }
            Ok::<(), anyhow::Error>(())
        }
    };

    let copy_fut = async {
        let mut in_fut = std::pin::pin!(inbound);
        let mut out_fut = std::pin::pin!(outbound);
        tokio::select! {
            res = &mut in_fut => {
                res?;
            }
            res = &mut out_fut => {
                res?;
            }
        }
        Ok::<(), anyhow::Error>(())
    };

    if opts.idle_timeout > Duration::from_millis(0) {
        tokio::time::timeout(opts.idle_timeout, copy_fut)
            .await
            .map_err(|_| anyhow::anyhow!("idle timeout"))??;
    } else {
        copy_fut.await?;
    }
    Ok(())
}

fn maybe_sample(sampler: &mut DictSampler, bytes: &[u8]) {
    if sampler.wants_more() {
        sampler.push(bytes);
    }
}

fn maybe_sample_frame(sampler: &mut DictSampler, raw: &[u8], rewritten: Option<&[u8]>) {
    maybe_sample(sampler, rewritten.unwrap_or(raw));
}

async fn process_egress_unlocked<W: tokio::io::AsyncWrite + Unpin>(
    sess: &SessionHandle,
    pending: &mut Vec<u8>,
    writer: &mut W,
    from_server: bool,
) -> std::io::Result<usize> {
    let mut written = 0;
    let mut offset = 0;
    while offset < pending.len() {
        let slice_len = pending.len() - offset;
        let poll_res = {
            let mut guard = sess.lock().unwrap();
            guard.set_state(SessionState::StreamingEgress);
            guard.set_flow_direction(from_server);
            guard.poll(&pending[offset..])
        };
        match poll_res {
            Ok(PollResult::Stream(StreamResult::Frame { len, payload, .. })) => {
                if len == 0 || len > slice_len {
                    break;
                }
                if let Some(ref p) = payload {
                    tokio::io::AsyncWriteExt::write_all(writer, p).await?;
                    written += p.len();
                } else {
                    tokio::io::AsyncWriteExt::write_all(writer, &pending[offset..offset + len])
                        .await?;
                    written += len;
                }
                offset += len;
            }
            Ok(PollResult::Stream(StreamResult::NeedMoreData))
            | Ok(PollResult::Stream(StreamResult::Blocked)) => break,
            _ => {
                tokio::io::AsyncWriteExt::write_all(writer, &pending[offset..]).await?;
                written += slice_len;
                offset += slice_len;
                break;
            }
        }
    }
    if offset > 0 {
        pending.drain(..offset);
    }
    Ok(written)
}

fn apply_session_data(sess: &Option<SessionHandle>, data: &[u8]) {
    let Some(sess) = sess else {
        return;
    };
    if let Ok(mut guard) = sess.lock() {
        let _ = guard.set_session_data(data);
    }
}

async fn drain_wasm_frames<W: tokio::io::AsyncWrite + Unpin>(
    wasm: &Option<SessionHandle>,
    read_buf: &mut Vec<u8>,
    opt_writer: &mut OptimizedWriter<W>,
    session_notify: &Notify,
    sampler: &mut DictSampler,
    from_server: bool,
) -> anyhow::Result<()> {
    if wasm.is_none() {
        if !read_buf.is_empty() {
            maybe_sample(sampler, read_buf);
            opt_writer
                .write_frame(read_buf, FramePriority::Defer)
                .await?;
            read_buf.clear();
        }
        return Ok(());
    }
    let sess = wasm.as_ref().unwrap();

    loop {
        if read_buf.is_empty() {
            break;
        }
        let poll_res = {
            let mut guard = sess.lock().unwrap();
            guard.set_state(SessionState::Streaming);
            guard.set_flow_direction(from_server);
            let res = guard.poll(read_buf);
            let announced = guard.take_announced();
            (res, announced)
        };
        for blob in poll_res.1 {
            opt_writer.write_control(&blob).await?;
        }
        match poll_res.0 {
            Ok(PollResult::Stream(StreamResult::Frame {
                len,
                priority,
                payload,
            })) => {
                if len == 0 || len > read_buf.len() {
                    break;
                }
                maybe_sample_frame(sampler, &read_buf[..len], payload.as_deref());
                if let Some(ref payload) = payload {
                    opt_writer
                        .write_frame_with_metric(len, payload, priority)
                        .await?;
                } else {
                    opt_writer
                        .write_frame(&read_buf[..len], priority)
                        .await?;
                }
                read_buf.drain(..len);
            }
            Ok(PollResult::Stream(StreamResult::NeedMoreData)) => break,
            Ok(PollResult::Stream(StreamResult::Blocked)) => {
                session_notify.notified().await;
            }
            Ok(PollResult::Handshake(_)) => {
                maybe_sample(sampler, read_buf);
                opt_writer
                    .write_frame(read_buf, FramePriority::Defer)
                    .await?;
                read_buf.clear();
                break;
            }
            Err(err) => {
                tracing::warn!(err=%err, "middleware poll failed; writing defer");
                maybe_sample(sampler, read_buf);
                opt_writer
                    .write_frame(read_buf, FramePriority::Defer)
                    .await?;
                read_buf.clear();
                break;
            }
        }
    }
    Ok(())
}

fn load_session(
    mw_name: Option<&str>,
    middleware_dir: Option<&Path>,
    skip_recompress: bool,
) -> anyhow::Result<Option<SessionHandle>> {
    let Some(name) = mw_name else {
        return Ok(None);
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    let base_name = name.strip_suffix(".wat").unwrap_or(name);

    let mut sess = None;
    if let Some(dir) = middleware_dir {
        for path in [dir.join(name), dir.join(format!("{base_name}.wat"))] {
            if path.exists() {
                let mw = crate::prism::middleware::WasmMiddleware::from_wat_path(base_name, &path)?;
                sess = Some(mw.create_shared_session()?);
                break;
            }
        }
    }
    if sess.is_none() {
        let path = Path::new(name);
        if path.exists() {
            let mw = crate::prism::middleware::WasmMiddleware::from_wat_path(base_name, path)?;
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
        let mut guard = sess.lock().unwrap();
        if let Some(data) =
            crate::prism::middleware::get_injected_middleware_data(base_name, None)
        {
            let _ = guard.set_data(&data);
        }
        if let Some(cfg) = crate::prism::middleware::get_dynamic_middleware_config(base_name) {
            let _ = guard.apply_config_map(&cfg);
        }
        if skip_recompress {
            let mut skip = std::collections::HashMap::new();
            skip.insert(
                "recompress_threshold".to_string(),
                serde_json::json!(0),
            );
            let _ = guard.apply_config_map(&skip);
        }
    }
    Ok(Some(sess))
}

/// Opens a player-facing optimized bridge (client sidecar / prism server).
pub async fn run_player_facing(
    tunnel: BoxedStream,
    local: tokio::net::TcpStream,
    optimizer: optimizer::OptimizerConfig,
    middleware: Option<String>,
    middleware_dir: Option<PathBuf>,
    stats: Vec<SharedOptimizerStats>,
    initial_bytes: Vec<u8>,
    idle_timeout: Duration,
    service_name: String,
    params_role: ParamsRole,
) -> anyhow::Result<()> {
    run(
        tunnel,
        local,
        PipelineOptions {
            optimizer,
            middleware,
            middleware_dir,
            stats,
            local_role: LocalRole::Player,
            params_role,
            initial_bytes,
            idle_timeout,
            service_name,
        },
    )
    .await
}

/// Opens an origin-facing optimized bridge (connector).
pub async fn run_upstream_facing(
    tunnel: BoxedStream,
    local: tokio::net::TcpStream,
    optimizer: optimizer::OptimizerConfig,
    middleware: Option<String>,
    middleware_dir: Option<PathBuf>,
    stats: Vec<SharedOptimizerStats>,
    service_name: String,
) -> anyhow::Result<()> {
    run(
        tunnel,
        local,
        PipelineOptions {
            optimizer,
            middleware,
            middleware_dir,
            stats,
            local_role: LocalRole::Upstream,
            params_role: ParamsRole::Acceptor,
            initial_bytes: Vec::new(),
            idle_timeout: Duration::ZERO,
            service_name,
        },
    )
    .await
}
