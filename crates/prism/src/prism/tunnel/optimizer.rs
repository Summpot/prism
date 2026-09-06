//! Native Traffic Pipeline (traffic optimizer) for Prism.
//!
//! Provides high-performance time-slice aggregation (`Batcher`) and continuous
//! stateful Zstandard compression/decompression (`ZstdStreamCompressor` and `ZstdStreamDecompressor`).

#![allow(dead_code)]

use std::io;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use pin_project_lite::pin_project;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use zstd::stream::raw::{CParameter, DParameter, Decoder, Encoder, InBuffer, Operation, OutBuffer};

use crate::prism::config::{
    ManagedOptimizerClientDocument, ManagedOptimizerDocument, OptimizerClientConfig,
    OptimizerConfig as PrismOptimizerConfig,
};
use crate::prism::middleware::FramePriority;

pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(20);
pub const DEFAULT_BUFFER_THRESHOLD: usize = 64 * 1024; // 64 KB
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;
pub const DEFAULT_ZSTD_WINDOW_LOG: u32 = 23; // 8 MB sliding window
pub const MAX_CHUNK_SIZE: usize = 32 * 1024 * 1024; // 32 MB guard limit

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Native Optimizer pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizerConfig {
    pub enabled: bool,
    pub flush_interval: Duration,
    pub buffer_threshold: usize,
    pub zstd_level: i32,
    pub zstd_window_log: u32,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: DEFAULT_ZSTD_LEVEL,
            zstd_window_log: DEFAULT_ZSTD_WINDOW_LOG,
        }
    }
}

impl From<&ManagedOptimizerDocument> for OptimizerConfig {
    fn from(doc: &ManagedOptimizerDocument) -> Self {
        Self {
            enabled: doc.enabled,
            flush_interval: Duration::from_millis(doc.flush_interval_ms.unwrap_or(20)),
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: doc.zstd_level.unwrap_or(DEFAULT_ZSTD_LEVEL),
            zstd_window_log: doc.zstd_window_log.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG),
        }
    }
}

impl From<&ManagedOptimizerClientDocument> for OptimizerConfig {
    fn from(doc: &ManagedOptimizerClientDocument) -> Self {
        Self {
            enabled: doc.enabled,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: DEFAULT_ZSTD_LEVEL,
            zstd_window_log: doc.zstd_window_log.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG),
        }
    }
}

impl From<&PrismOptimizerConfig> for OptimizerConfig {
    fn from(cfg: &PrismOptimizerConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            flush_interval: Duration::from_millis(cfg.flush_interval_ms()),
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: cfg.zstd_level(),
            zstd_window_log: cfg.zstd_window_log(),
        }
    }
}

impl From<&OptimizerClientConfig> for OptimizerConfig {
    fn from(cfg: &OptimizerClientConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: DEFAULT_ZSTD_LEVEL,
            zstd_window_log: cfg.zstd_window_log(),
        }
    }
}

// ============================================================================
// Traffic Observability & Statistics
// ============================================================================

/// Direction of traffic in the optimizer pipeline.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrafficDirection {
    /// Client -> Server (Player to upstream).
    Uplink,
    /// Server -> Client (Upstream to player).
    Downlink,
}

impl Default for TrafficDirection {
    fn default() -> Self {
        Self::Uplink
    }
}

/// Statistics snapshot for a single traffic direction (uplink or downlink).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DirectionStatsSnapshot {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub batches: u64,
    pub compression_time_us: u64,
    pub decompression_time_us: u64,
    pub est_transfer_time_saved_ms: f64,
    pub est_processing_time_ms: f64,
    pub net_latency_saved_ms: f64,
}

/// Detailed snapshot of traffic & latency optimization metrics.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OptimizerStatsSnapshot {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub urgent_batches: u64,
    pub timer_batches: u64,
    pub threshold_batches: u64,

    // Directional metrics
    pub uplink: DirectionStatsSnapshot,
    pub downlink: DirectionStatsSnapshot,

    // Latency & processing metrics
    pub compression_time_us: u64,
    pub decompression_time_us: u64,
    pub batching_delay_us: u64,
    pub est_transfer_time_saved_ms: f64,
    pub est_processing_time_ms: f64,
    pub net_latency_saved_ms: f64,
}

/// Lock-free atomic directional traffic statistics counter.
#[derive(Debug, Default)]
pub struct DirectionStats {
    pub raw_bytes: AtomicU64,
    pub wire_bytes: AtomicU64,
    pub batches: AtomicU64,
    pub compression_time_us: AtomicU64,
    pub decompression_time_us: AtomicU64,
}

impl DirectionStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_raw_bytes(&self, bytes: u64) {
        self.raw_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_wire_bytes(&self, bytes: u64) {
        self.wire_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn inc_batches(&self) {
        self.batches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn add_compression_time(&self, us: u64) {
        self.compression_time_us.fetch_add(us, Ordering::Relaxed);
    }

    pub fn add_decompression_time(&self, us: u64) {
        self.decompression_time_us.fetch_add(us, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> DirectionStatsSnapshot {
        let raw = self.raw_bytes.load(Ordering::Relaxed);
        let wire = self.wire_bytes.load(Ordering::Relaxed);
        let saved_bytes = raw.saturating_sub(wire);
        let saved_ratio = if raw > 0 {
            (saved_bytes as f64) / (raw as f64)
        } else {
            0.0
        };
        let batches = self.batches.load(Ordering::Relaxed);
        let comp_us = self.compression_time_us.load(Ordering::Relaxed);
        let decomp_us = self.decompression_time_us.load(Ordering::Relaxed);

        // Reference bandwidth: 20 Mbps = 2,500,000 bytes/sec = 2,500 bytes/ms
        let est_transfer_time_saved_ms = (saved_bytes as f64) / 2500.0;
        let est_processing_time_ms = ((comp_us + decomp_us) as f64) / 1000.0;
        let net_latency_saved_ms = est_transfer_time_saved_ms - est_processing_time_ms;

        DirectionStatsSnapshot {
            raw_bytes: raw,
            wire_bytes: wire,
            saved_bytes,
            saved_ratio,
            batches,
            compression_time_us: comp_us,
            decompression_time_us: decomp_us,
            est_transfer_time_saved_ms,
            est_processing_time_ms,
            net_latency_saved_ms,
        }
    }
}

/// Lock-free atomic traffic & latency statistics counter.
#[derive(Debug, Default)]
pub struct OptimizerStats {
    pub raw_bytes: AtomicU64,
    pub wire_bytes: AtomicU64,
    pub urgent_batches: AtomicU64,
    pub timer_batches: AtomicU64,
    pub threshold_batches: AtomicU64,

    pub uplink: DirectionStats,
    pub downlink: DirectionStats,

    pub compression_time_us: AtomicU64,
    pub decompression_time_us: AtomicU64,
    pub batching_delay_us: AtomicU64,
}

impl OptimizerStats {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_raw_bytes(&self, bytes: u64) {
        self.raw_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_wire_bytes(&self, bytes: u64) {
        self.wire_bytes.fetch_add(bytes, Ordering::Relaxed);
    }

    pub fn add_direction_raw_bytes(&self, dir: TrafficDirection, bytes: u64) {
        self.add_raw_bytes(bytes);
        match dir {
            TrafficDirection::Uplink => self.uplink.add_raw_bytes(bytes),
            TrafficDirection::Downlink => self.downlink.add_raw_bytes(bytes),
        }
    }

    pub fn add_direction_wire_bytes(&self, dir: TrafficDirection, bytes: u64) {
        self.add_wire_bytes(bytes);
        match dir {
            TrafficDirection::Uplink => self.uplink.add_wire_bytes(bytes),
            TrafficDirection::Downlink => self.downlink.add_wire_bytes(bytes),
        }
    }

    pub fn record_compression(&self, dir: TrafficDirection, duration_us: u64, queue_delay_us: u64) {
        self.compression_time_us
            .fetch_add(duration_us, Ordering::Relaxed);
        self.batching_delay_us
            .fetch_add(queue_delay_us, Ordering::Relaxed);
        match dir {
            TrafficDirection::Uplink => {
                self.uplink.inc_batches();
                self.uplink.add_compression_time(duration_us);
            }
            TrafficDirection::Downlink => {
                self.downlink.inc_batches();
                self.downlink.add_compression_time(duration_us);
            }
        }
    }

    pub fn record_decompression(&self, dir: TrafficDirection, duration_us: u64) {
        self.decompression_time_us
            .fetch_add(duration_us, Ordering::Relaxed);
        match dir {
            TrafficDirection::Uplink => {
                self.uplink.inc_batches();
                self.uplink.add_decompression_time(duration_us);
            }
            TrafficDirection::Downlink => {
                self.downlink.inc_batches();
                self.downlink.add_decompression_time(duration_us);
            }
        }
    }

    pub fn inc_urgent(&self) {
        self.urgent_batches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_timer(&self) {
        self.timer_batches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn inc_threshold(&self) {
        self.threshold_batches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> OptimizerStatsSnapshot {
        let raw = self.raw_bytes.load(Ordering::Relaxed);
        let wire = self.wire_bytes.load(Ordering::Relaxed);
        let saved_bytes = raw.saturating_sub(wire);
        let saved_ratio = if raw > 0 {
            (saved_bytes as f64) / (raw as f64)
        } else {
            0.0
        };

        let comp_us = self.compression_time_us.load(Ordering::Relaxed);
        let decomp_us = self.decompression_time_us.load(Ordering::Relaxed);
        let delay_us = self.batching_delay_us.load(Ordering::Relaxed);

        let est_transfer_time_saved_ms = (saved_bytes as f64) / 2500.0;
        let est_processing_time_ms = ((comp_us + decomp_us + delay_us) as f64) / 1000.0;
        let net_latency_saved_ms = est_transfer_time_saved_ms - est_processing_time_ms;

        OptimizerStatsSnapshot {
            raw_bytes: raw,
            wire_bytes: wire,
            saved_bytes,
            saved_ratio,
            urgent_batches: self.urgent_batches.load(Ordering::Relaxed),
            timer_batches: self.timer_batches.load(Ordering::Relaxed),
            threshold_batches: self.threshold_batches.load(Ordering::Relaxed),
            uplink: self.uplink.snapshot(),
            downlink: self.downlink.snapshot(),
            compression_time_us: comp_us,
            decompression_time_us: decomp_us,
            batching_delay_us: delay_us,
            est_transfer_time_saved_ms,
            est_processing_time_ms,
            net_latency_saved_ms,
        }
    }
}

pub type SharedOptimizerStats = Arc<OptimizerStats>;

// ============================================================================
// Component 1: Batcher (Time-slice aggregator)
// ============================================================================

/// Configuration for the [`Batcher`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatcherConfig {
    pub flush_interval: Duration,
    pub buffer_threshold: usize,
}

impl Default for BatcherConfig {
    fn default() -> Self {
        Self {
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
        }
    }
}

/// Time-slice frame aggregator.
///
/// Collects `FRAME_DEFER` frames into a contiguous buffer. When elapsed time
/// reaches `flush_interval` (default 20ms / 1 Tick) OR the buffer reaches
/// `buffer_threshold` (default 64KB), a batch flush is triggered.
///
/// When a `FRAME_URGENT` frame arrives, the frame is appended and a batch flush
/// is triggered immediately (0 extra queuing delay).
pub struct Batcher {
    config: BatcherConfig,
    buffer: Vec<u8>,
    first_frame_at: Option<Instant>,
}

impl Batcher {
    /// Creates a new `Batcher` with the specified configuration.
    pub fn new(config: BatcherConfig) -> Self {
        let capacity = config.buffer_threshold;
        Self {
            config,
            buffer: Vec::with_capacity(capacity),
            first_frame_at: None,
        }
    }

    /// Creates a new `Batcher` with default configuration (20ms, 64KB).
    pub fn with_defaults() -> Self {
        Self::new(BatcherConfig::default())
    }

    /// Returns the configuration.
    pub fn config(&self) -> &BatcherConfig {
        &self.config
    }

    /// Returns `true` if the internal buffer is empty.
    pub fn is_empty(&self) -> bool {
        self.buffer.is_empty()
    }

    /// Returns the number of bytes currently buffered.
    pub fn buffered_len(&self) -> usize {
        self.buffer.len()
    }

    /// Returns the time when the first frame of the current batch arrived.
    pub fn first_frame_at(&self) -> Option<Instant> {
        self.first_frame_at
    }

    /// Returns the remaining duration until the current batch should be flushed,
    /// or `None` if the buffer is empty.
    pub fn time_until_flush(&self) -> Option<Duration> {
        self.time_until_flush_at(Instant::now())
    }

    /// Returns the remaining duration until flush at a specific timestamp.
    pub fn time_until_flush_at(&self, now: Instant) -> Option<Duration> {
        self.first_frame_at.map(|start| {
            if now >= start + self.config.flush_interval {
                Duration::ZERO
            } else {
                (start + self.config.flush_interval) - now
            }
        })
    }

    /// Pushes a frame into the batcher using the current system time (`Instant::now()`).
    ///
    /// If the frame causes a flush (due to `FRAME_URGENT`, buffer threshold, or elapsed time),
    /// the flushed batch is returned as `Some(Vec<u8>)`.
    pub fn push(&mut self, frame: &[u8], priority: FramePriority) -> Option<Vec<u8>> {
        self.push_at(frame, priority, Instant::now())
    }

    /// Pushes a frame into the batcher with an explicit timestamp.
    pub fn push_at(
        &mut self,
        frame: &[u8],
        priority: FramePriority,
        now: Instant,
    ) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            self.first_frame_at = Some(now);
        }

        self.buffer.extend_from_slice(frame);

        match priority {
            FramePriority::Urgent => {
                // High-priority frames immediately trigger flush (0 extra queuing delay).
                Some(self.flush())
            }
            FramePriority::Defer => {
                let time_reached = self
                    .first_frame_at
                    .map(|start| now.duration_since(start) >= self.config.flush_interval)
                    .unwrap_or(false);
                let size_reached = self.buffer.len() >= self.config.buffer_threshold;

                if time_reached || size_reached {
                    Some(self.flush())
                } else {
                    None
                }
            }
        }
    }

    /// Checks if a time-based flush is due based on current time.
    pub fn check_timer(&mut self) -> Option<Vec<u8>> {
        self.check_timer_at(Instant::now())
    }

    /// Checks if a time-based flush is due at the specified timestamp.
    pub fn check_timer_at(&mut self, now: Instant) -> Option<Vec<u8>> {
        if self.buffer.is_empty() {
            return None;
        }

        if let Some(start) = self.first_frame_at {
            if now.duration_since(start) >= self.config.flush_interval {
                return Some(self.flush());
            }
        }

        None
    }

    /// Unconditionally flushes all buffered frames.
    pub fn flush(&mut self) -> Vec<u8> {
        self.first_frame_at = None;
        if self.buffer.is_empty() {
            Vec::new()
        } else {
            let capacity = self.config.buffer_threshold;
            std::mem::replace(&mut self.buffer, Vec::with_capacity(capacity))
        }
    }
}

// ============================================================================
// Component 2: ZstdStreamCompressor
// ============================================================================

/// Configuration for [`ZstdStreamCompressor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompressorConfig {
    pub compression_level: i32,
    pub window_log: u32,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            compression_level: DEFAULT_ZSTD_LEVEL,
            window_log: DEFAULT_ZSTD_WINDOW_LOG,
        }
    }
}

/// Stateful continuous SIMD Zstd stream compressor.
///
/// Maintains a persistent sliding window history (default 8MB via `window_log = 23`)
/// across batches without resetting dictionary history upon flushing. Subsequent
/// frames exploit cross-packet redundancy to achieve high (80%+) compression ratios.
pub struct ZstdStreamCompressor {
    encoder: Encoder<'static>,
    config: CompressorConfig,
}

impl ZstdStreamCompressor {
    /// Creates a new continuous stream compressor with the specified configuration.
    pub fn new(config: CompressorConfig) -> io::Result<Self> {
        let mut encoder = Encoder::new(config.compression_level)?;
        encoder.set_parameter(CParameter::WindowLog(config.window_log))?;
        Ok(Self { encoder, config })
    }

    /// Creates a new continuous stream compressor with default parameters (level 3, 8MB window).
    pub fn with_defaults() -> io::Result<Self> {
        Self::new(CompressorConfig::default())
    }

    /// Returns the configuration.
    pub fn config(&self) -> &CompressorConfig {
        &self.config
    }

    /// Compresses a batch of data, flushing all compressed output to the returned vector
    /// while preserving dictionary history for subsequent batches.
    pub fn compress_batch(&mut self, input: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        self.compress_batch_into(input, &mut out)?;
        Ok(out)
    }

    /// Compresses a batch of data into an existing output buffer, flushing output
    /// while preserving dictionary history for subsequent batches.
    pub fn compress_batch_into(&mut self, input: &[u8], output: &mut Vec<u8>) -> io::Result<()> {
        let mut in_buf = InBuffer::around(input);
        let mut scratch = [0u8; 8192];

        while in_buf.pos() < in_buf.src.len() {
            let mut out = OutBuffer::around(&mut scratch);
            self.encoder.run(&mut in_buf, &mut out)?;
            output.extend_from_slice(out.as_slice());
        }

        loop {
            let mut out = OutBuffer::around(&mut scratch);
            let remaining = self.encoder.flush(&mut out)?;
            output.extend_from_slice(out.as_slice());
            if remaining == 0 {
                break;
            }
        }

        Ok(())
    }

    /// Resets the encoder state and dictionary history.
    pub fn reset(&mut self) -> io::Result<()> {
        self.encoder = Encoder::new(self.config.compression_level)?;
        self.encoder
            .set_parameter(CParameter::WindowLog(self.config.window_log))?;
        Ok(())
    }
}

// ============================================================================
// Component 3: ZstdStreamDecompressor
// ============================================================================

/// Configuration for [`ZstdStreamDecompressor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecompressorConfig {
    pub window_log: u32,
}

impl Default for DecompressorConfig {
    fn default() -> Self {
        Self {
            window_log: DEFAULT_ZSTD_WINDOW_LOG,
        }
    }
}

/// Stateful continuous SIMD Zstd stream decompressor.
///
/// Maintains persistent sliding window history across chunks corresponding
/// to [`ZstdStreamCompressor`].
pub struct ZstdStreamDecompressor {
    decoder: Decoder<'static>,
    config: DecompressorConfig,
}

impl ZstdStreamDecompressor {
    /// Creates a new continuous stream decompressor with the specified configuration.
    pub fn new(config: DecompressorConfig) -> io::Result<Self> {
        let mut decoder = Decoder::new()?;
        decoder.set_parameter(DParameter::WindowLogMax(config.window_log))?;
        Ok(Self { decoder, config })
    }

    /// Creates a new continuous stream decompressor with default parameters (8MB window).
    pub fn with_defaults() -> io::Result<Self> {
        Self::new(DecompressorConfig::default())
    }

    /// Returns the configuration.
    pub fn config(&self) -> &DecompressorConfig {
        &self.config
    }

    /// Decompresses an incoming chunk of compressed data, appending decompressed bytes
    /// to `output`.
    pub fn decompress_chunk_into(
        &mut self,
        compressed: &[u8],
        output: &mut Vec<u8>,
    ) -> io::Result<()> {
        let mut in_buf = InBuffer::around(compressed);
        let mut scratch = [0u8; 8192];

        while in_buf.pos() < in_buf.src.len() {
            let mut out = OutBuffer::around(&mut scratch);
            let prev_in_pos = in_buf.pos();
            let prev_out_pos = out.pos();

            self.decoder.run(&mut in_buf, &mut out)?;
            output.extend_from_slice(out.as_slice());

            if in_buf.pos() == prev_in_pos && out.pos() == prev_out_pos {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "zstd decompressor made no progress on chunk",
                ));
            }
        }

        Ok(())
    }

    /// Decompresses an incoming chunk of compressed data into a new vector.
    pub fn decompress_chunk(&mut self, compressed: &[u8]) -> io::Result<Vec<u8>> {
        let mut out = Vec::new();
        self.decompress_chunk_into(compressed, &mut out)?;
        Ok(out)
    }

    /// Resets the decoder state and dictionary history.
    pub fn reset(&mut self) -> io::Result<()> {
        self.decoder = Decoder::new()?;
        self.decoder
            .set_parameter(DParameter::WindowLogMax(self.config.window_log))?;
        Ok(())
    }
}

// ============================================================================
// Component 4: Framing & Async Stream Processing Helpers
// ============================================================================

/// Encodes a raw batch into a framed compressed chunk:
/// `[chunk_len: u32 (Big-Endian)][compressed_bytes]`
pub fn encode_batch(
    compressor: &mut ZstdStreamCompressor,
    raw_batch: &[u8],
) -> io::Result<Vec<u8>> {
    let mut out = Vec::new();
    encode_batch_into(compressor, raw_batch, &mut out)?;
    Ok(out)
}

/// Encodes a raw batch into an existing buffer as `[chunk_len: u32 BE][compressed_bytes]`.
pub fn encode_batch_into(
    compressor: &mut ZstdStreamCompressor,
    raw_batch: &[u8],
    output: &mut Vec<u8>,
) -> io::Result<usize> {
    if raw_batch.is_empty() {
        return Ok(0);
    }
    let header_start = output.len();
    output.extend_from_slice(&[0u8; 4]);
    let payload_start = output.len();

    compressor.compress_batch_into(raw_batch, output)?;
    let payload_len = output.len() - payload_start;
    let len_be = (payload_len as u32).to_be_bytes();
    output[header_start..header_start + 4].copy_from_slice(&len_be);

    Ok(payload_len + 4)
}

/// Decodes a sequence of length-prefixed chunks (`[u32 BE][payload]`) from a byte slice.
pub fn decode_stream(
    decompressor: &mut ZstdStreamDecompressor,
    mut framed_bytes: &[u8],
) -> io::Result<Vec<u8>> {
    let mut decompressed = Vec::new();
    while !framed_bytes.is_empty() {
        if framed_bytes.len() < 4 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete chunk length prefix in stream",
            ));
        }
        let chunk_len = u32::from_be_bytes(framed_bytes[..4].try_into().unwrap()) as usize;
        framed_bytes = &framed_bytes[4..];

        if chunk_len > MAX_CHUNK_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("chunk length {chunk_len} exceeds max allowed {MAX_CHUNK_SIZE}"),
            ));
        }

        if framed_bytes.len() < chunk_len {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "incomplete chunk payload in stream",
            ));
        }

        let payload = &framed_bytes[..chunk_len];
        framed_bytes = &framed_bytes[chunk_len..];
        decompressor.decompress_chunk_into(payload, &mut decompressed)?;
    }
    Ok(decompressed)
}

pin_project! {
    /// Transparent async writer that batches and compresses data using [`Batcher`]
    /// and [`ZstdStreamCompressor`].
    ///
    /// Can be used as a standard [`tokio::io::AsyncWrite`] stream, or directly via
    /// priority-aware methods like [`Self::write_frame`].
    pub struct OptimizedWriter<W> {
        #[pin]
        inner: W,
        batcher: Batcher,
        compressor: ZstdStreamCompressor,
        write_buf: Vec<u8>,
        write_pos: usize,
        stats: Vec<SharedOptimizerStats>,
        direction: TrafficDirection,
    }
}

impl<W> OptimizedWriter<W> {
    /// Creates a new `OptimizedWriter` with given configurations.
    pub fn new(
        inner: W,
        batcher_config: BatcherConfig,
        compressor_config: CompressorConfig,
    ) -> io::Result<Self> {
        Ok(Self {
            inner,
            batcher: Batcher::new(batcher_config),
            compressor: ZstdStreamCompressor::new(compressor_config)?,
            write_buf: Vec::new(),
            write_pos: 0,
            stats: Vec::new(),
            direction: TrafficDirection::Uplink,
        })
    }

    /// Creates a new `OptimizedWriter` with default configurations.
    pub fn with_defaults(inner: W) -> io::Result<Self> {
        Self::new(inner, BatcherConfig::default(), CompressorConfig::default())
    }

    /// Configures the traffic direction for this writer.
    pub fn with_direction(mut self, direction: TrafficDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Returns the configured traffic direction.
    pub fn direction(&self) -> TrafficDirection {
        self.direction
    }

    /// Attaches a shared traffic statistics collector to this writer.
    pub fn add_stats(&mut self, stats: SharedOptimizerStats) {
        self.stats.push(stats);
    }

    /// Builder method to attach a shared traffic statistics collector.
    pub fn with_stats(mut self, stats: SharedOptimizerStats) -> Self {
        self.stats.push(stats);
        self
    }

    /// Returns references to attached traffic statistics collectors.
    pub fn stats(&self) -> &[SharedOptimizerStats] {
        &self.stats
    }

    fn record_raw(&self, bytes: usize) {
        for s in &self.stats {
            s.add_direction_raw_bytes(self.direction, bytes as u64);
        }
    }

    fn record_wire(&self, bytes: usize) {
        for s in &self.stats {
            s.add_direction_wire_bytes(self.direction, bytes as u64);
        }
    }

    fn record_compression(&self, duration_us: u64, queue_delay_us: u64) {
        for s in &self.stats {
            s.record_compression(self.direction, duration_us, queue_delay_us);
        }
    }

    fn record_urgent(&self) {
        for s in &self.stats {
            s.inc_urgent();
        }
    }

    fn record_timer(&self) {
        for s in &self.stats {
            s.inc_timer();
        }
    }

    fn record_threshold(&self) {
        for s in &self.stats {
            s.inc_threshold();
        }
    }

    /// Returns a reference to the inner writer.
    pub fn get_ref(&self) -> &W {
        &self.inner
    }

    /// Returns a mutable reference to the inner writer.
    pub fn get_mut(&mut self) -> &mut W {
        &mut self.inner
    }

    /// Unwraps the inner writer, discarding any unwritten buffered data.
    pub fn into_inner(self) -> W {
        self.inner
    }

    /// Returns the remaining duration until a time-based batch flush is due.
    pub fn time_until_flush(&self) -> Option<Duration> {
        self.batcher.time_until_flush()
    }
}

impl<W: AsyncWrite + Unpin> OptimizedWriter<W> {
    /// Writes a frame with an explicit raw metric byte count.
    ///
    /// This is used when the incoming frame was decompressed from an upstream Deflate
    /// stream: `metric_raw_len` is the wire byte count of the incoming Deflate packet
    /// (preventing zip-bomb / raw uncompressed ratio inflation), while `frame` is the
    /// decompressed raw payload that [`Batcher`] and [`ZstdStreamCompressor`] will
    /// aggregate and compress over the PRPX tunnel.
    pub async fn write_frame_with_metric(
        &mut self,
        metric_raw_len: usize,
        frame: &[u8],
        priority: FramePriority,
    ) -> io::Result<()> {
        // Flush any pending write_buf
        self.flush_pending_write_buf().await?;

        self.record_raw(metric_raw_len);

        let queue_delay = self
            .batcher
            .first_frame_at()
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);

        if let Some(batch) = self.batcher.push(frame, priority) {
            match priority {
                FramePriority::Urgent => self.record_urgent(),
                FramePriority::Defer => self.record_threshold(),
            }
            let prev_len = self.write_buf.len();
            let start = Instant::now();
            encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            let comp_us = start.elapsed().as_micros() as u64;
            let encoded = self.write_buf.len() - prev_len;
            self.record_wire(encoded);
            self.record_compression(comp_us, queue_delay);
            self.flush_pending_write_buf().await?;
        }
        Ok(())
    }

    /// Writes a frame with the specified priority.
    ///
    /// If `FramePriority::Urgent` is given, or if the buffer/time threshold is reached,
    /// the batch is flushed and written out immediately.
    pub async fn write_frame(&mut self, frame: &[u8], priority: FramePriority) -> io::Result<()> {
        self.write_frame_with_metric(frame.len(), frame, priority)
            .await
    }

    /// Explicitly flushes any buffered frames through compression and writes them to `inner`.
    pub async fn flush_batch(&mut self) -> io::Result<()> {
        self.flush_pending_write_buf().await?;

        if !self.batcher.is_empty() {
            let queue_delay = self
                .batcher
                .first_frame_at()
                .map(|t| t.elapsed().as_micros() as u64)
                .unwrap_or(0);
            let batch = self.batcher.flush();
            self.record_threshold();
            let prev_len = self.write_buf.len();
            let start = Instant::now();
            encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            let comp_us = start.elapsed().as_micros() as u64;
            let encoded = self.write_buf.len() - prev_len;
            self.record_wire(encoded);
            self.record_compression(comp_us, queue_delay);
            self.flush_pending_write_buf().await?;
        }

        tokio::io::AsyncWriteExt::flush(&mut self.inner).await?;
        Ok(())
    }

    /// Checks if a time-slice flush is due and writes the flushed batch to `inner`.
    /// Returns `true` if a batch was flushed.
    pub async fn flush_if_due(&mut self) -> io::Result<bool> {
        let queue_delay = self
            .batcher
            .first_frame_at()
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);
        if let Some(batch) = self.batcher.check_timer() {
            self.flush_pending_write_buf().await?;
            self.record_timer();
            let prev_len = self.write_buf.len();
            let start = Instant::now();
            encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            let comp_us = start.elapsed().as_micros() as u64;
            let encoded = self.write_buf.len() - prev_len;
            self.record_wire(encoded);
            self.record_compression(comp_us, queue_delay);
            self.flush_pending_write_buf().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    async fn flush_pending_write_buf(&mut self) -> io::Result<()> {
        while self.write_pos < self.write_buf.len() {
            let n =
                tokio::io::AsyncWriteExt::write(&mut self.inner, &self.write_buf[self.write_pos..])
                    .await?;
            if n == 0 {
                return Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write compressed data",
                ));
            }
            self.write_pos += n;
        }
        self.write_buf.clear();
        self.write_pos = 0;
        Ok(())
    }
}

fn poll_flush_write_buf_pinned<W: AsyncWrite>(
    mut inner: Pin<&mut W>,
    write_buf: &mut Vec<u8>,
    write_pos: &mut usize,
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    while *write_pos < write_buf.len() {
        match inner.as_mut().poll_write(cx, &write_buf[*write_pos..]) {
            Poll::Ready(Ok(0)) => {
                return Poll::Ready(Err(io::Error::new(
                    io::ErrorKind::WriteZero,
                    "failed to write compressed data",
                )));
            }
            Poll::Ready(Ok(n)) => {
                *write_pos += n;
            }
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }
    }
    write_buf.clear();
    *write_pos = 0;
    Poll::Ready(Ok(()))
}

impl<W: AsyncWrite> AsyncWrite for OptimizedWriter<W> {
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let mut this = self.project();

        // 1. Flush any pending write buffer
        if let Poll::Ready(Err(e)) =
            poll_flush_write_buf_pinned(this.inner.as_mut(), this.write_buf, this.write_pos, cx)
        {
            return Poll::Ready(Err(e));
        }

        for s in this.stats.iter() {
            s.add_direction_raw_bytes(*this.direction, buf.len() as u64);
        }

        let queue_delay = this
            .batcher
            .first_frame_at()
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);

        // 2. Add incoming bytes to batcher with defer priority
        let flushed = this.batcher.push(buf, FramePriority::Defer);
        if let Some(batch) = flushed {
            for s in this.stats.iter() {
                s.inc_threshold();
            }
            let prev_len = this.write_buf.len();
            let start = Instant::now();
            if let Err(e) = encode_batch_into(this.compressor, &batch, this.write_buf) {
                return Poll::Ready(Err(e));
            }
            let comp_us = start.elapsed().as_micros() as u64;
            let encoded = this.write_buf.len() - prev_len;
            for s in this.stats.iter() {
                s.add_direction_wire_bytes(*this.direction, encoded as u64);
                s.record_compression(*this.direction, comp_us, queue_delay);
            }
            *this.write_pos = 0;
            // Best effort immediate write
            let _ = poll_flush_write_buf_pinned(
                this.inner.as_mut(),
                this.write_buf,
                this.write_pos,
                cx,
            );
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();

        // Flush any pending write buffer
        match poll_flush_write_buf_pinned(this.inner.as_mut(), this.write_buf, this.write_pos, cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }

        // Flush batcher if non-empty
        if !this.batcher.is_empty() {
            let queue_delay = this
                .batcher
                .first_frame_at()
                .map(|t| t.elapsed().as_micros() as u64)
                .unwrap_or(0);
            let batch = this.batcher.flush();
            for s in this.stats.iter() {
                s.inc_threshold();
            }
            let prev_len = this.write_buf.len();
            let start = Instant::now();
            if let Err(e) = encode_batch_into(this.compressor, &batch, this.write_buf) {
                return Poll::Ready(Err(e));
            }
            let comp_us = start.elapsed().as_micros() as u64;
            let encoded = this.write_buf.len() - prev_len;
            for s in this.stats.iter() {
                s.add_direction_wire_bytes(*this.direction, encoded as u64);
                s.record_compression(*this.direction, comp_us, queue_delay);
            }
            *this.write_pos = 0;
            match poll_flush_write_buf_pinned(
                this.inner.as_mut(),
                this.write_buf,
                this.write_pos,
                cx,
            ) {
                Poll::Ready(Ok(())) => {}
                other => return other,
            }
        }

        this.inner.poll_flush(cx)
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match self.as_mut().poll_flush(cx) {
            Poll::Ready(Ok(())) => {}
            other => return other,
        }
        self.project().inner.poll_shutdown(cx)
    }
}

pin_project! {
    /// Transparent async reader that decompresses framed continuous zstd streams
    /// using [`ZstdStreamDecompressor`].
    pub struct OptimizedReader<R> {
        #[pin]
        inner: R,
        decompressor: ZstdStreamDecompressor,
        decompressed_buf: Vec<u8>,
        decompressed_pos: usize,
        header_buf: [u8; 4],
        header_pos: usize,
        chunk_len: usize,
        payload_buf: Vec<u8>,
        payload_pos: usize,
        stats: Vec<SharedOptimizerStats>,
        record_raw_metrics: bool,
        direction: TrafficDirection,
    }
}

impl<R> OptimizedReader<R> {
    /// Creates a new `OptimizedReader` with given configuration.
    pub fn new(inner: R, config: DecompressorConfig) -> io::Result<Self> {
        Ok(Self {
            inner,
            decompressor: ZstdStreamDecompressor::new(config)?,
            decompressed_buf: Vec::new(),
            decompressed_pos: 0,
            header_buf: [0u8; 4],
            header_pos: 0,
            chunk_len: 0,
            payload_buf: Vec::new(),
            payload_pos: 0,
            stats: Vec::new(),
            record_raw_metrics: true,
            direction: TrafficDirection::Downlink,
        })
    }

    /// Creates a new `OptimizedReader` with default configuration.
    pub fn with_defaults(inner: R) -> io::Result<Self> {
        Self::new(inner, DecompressorConfig::default())
    }

    /// Configures the traffic direction for this reader.
    pub fn with_direction(mut self, direction: TrafficDirection) -> Self {
        self.direction = direction;
        self
    }

    /// Returns the configured traffic direction.
    pub fn direction(&self) -> TrafficDirection {
        self.direction
    }

    /// Attaches a shared traffic statistics collector to this reader.
    pub fn add_stats(&mut self, stats: SharedOptimizerStats) {
        self.stats.push(stats);
    }

    /// Builder method to attach a shared traffic statistics collector.
    pub fn with_stats(mut self, stats: SharedOptimizerStats) -> Self {
        self.stats.push(stats);
        self
    }

    /// Builder method to attach a shared traffic statistics collector and configure
    /// whether raw decompressed bytes should be recorded as `raw_bytes` metric.
    /// When `record_raw` is false, only wire bytes are recorded on the reader, leaving
    /// raw metrics to be recorded after protocol recompression at egress.
    pub fn with_stats_and_raw_mode(
        mut self,
        stats: SharedOptimizerStats,
        record_raw: bool,
    ) -> Self {
        self.stats.push(stats);
        self.record_raw_metrics = record_raw;
        self
    }

    /// Sets whether this reader records raw decompressed byte counts to attached stats.
    pub fn set_record_raw_metrics(&mut self, record_raw: bool) {
        self.record_raw_metrics = record_raw;
    }

    /// Returns references to attached traffic statistics collectors.
    pub fn stats(&self) -> &[SharedOptimizerStats] {
        &self.stats
    }

    /// Returns a reference to the inner reader.
    pub fn get_ref(&self) -> &R {
        &self.inner
    }

    /// Returns a mutable reference to the inner reader.
    pub fn get_mut(&mut self) -> &mut R {
        &mut self.inner
    }

    /// Unwraps the inner reader.
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R: AsyncRead> AsyncRead for OptimizedReader<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let mut this = self.project();

        // 1. If decompressed data is available, return it immediately
        if *this.decompressed_pos < this.decompressed_buf.len() {
            let avail = &this.decompressed_buf[*this.decompressed_pos..];
            let to_copy = std::cmp::min(avail.len(), buf.remaining());
            buf.put_slice(&avail[..to_copy]);
            *this.decompressed_pos += to_copy;
            if *this.decompressed_pos >= this.decompressed_buf.len() {
                this.decompressed_buf.clear();
                *this.decompressed_pos = 0;
            }
            return Poll::Ready(Ok(()));
        }

        // 2. Otherwise, read next chunk from inner
        loop {
            // Read 4-byte BE length header
            while *this.header_pos < 4 {
                let mut header_read_buf = ReadBuf::new(&mut this.header_buf[*this.header_pos..]);
                match this.inner.as_mut().poll_read(cx, &mut header_read_buf) {
                    Poll::Ready(Ok(())) => {
                        let bytes_read = header_read_buf.filled().len();
                        if bytes_read == 0 {
                            if *this.header_pos == 0 {
                                // Clean EOF at frame boundary
                                return Poll::Ready(Ok(()));
                            } else {
                                return Poll::Ready(Err(io::Error::new(
                                    io::ErrorKind::UnexpectedEof,
                                    "unexpected EOF reading chunk header",
                                )));
                            }
                        }
                        *this.header_pos += bytes_read;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                }
            }

            // Parse chunk length if not already set
            if *this.chunk_len == 0 {
                let len = u32::from_be_bytes(*this.header_buf) as usize;
                if len > MAX_CHUNK_SIZE {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::InvalidData,
                        format!("chunk size {len} exceeds limit {MAX_CHUNK_SIZE}"),
                    )));
                }
                if len == 0 {
                    *this.header_pos = 0;
                    continue;
                }
                *this.chunk_len = len;
                this.payload_buf.resize(len, 0);
                *this.payload_pos = 0;
            }

            // Read payload
            let len = *this.chunk_len;
            while *this.payload_pos < len {
                let mut payload_read_buf =
                    ReadBuf::new(&mut this.payload_buf[*this.payload_pos..len]);
                match this.inner.as_mut().poll_read(cx, &mut payload_read_buf) {
                    Poll::Ready(Ok(())) => {
                        let bytes_read = payload_read_buf.filled().len();
                        if bytes_read == 0 {
                            return Poll::Ready(Err(io::Error::new(
                                io::ErrorKind::UnexpectedEof,
                                "unexpected EOF reading chunk payload",
                            )));
                        }
                        *this.payload_pos += bytes_read;
                    }
                    Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
                    Poll::Pending => return Poll::Pending,
                }
            }

            // Decompress payload
            this.decompressed_buf.clear();
            *this.decompressed_pos = 0;
            let start = Instant::now();
            if let Err(e) = this
                .decompressor
                .decompress_chunk_into(&this.payload_buf[..len], this.decompressed_buf)
            {
                return Poll::Ready(Err(e));
            }
            let decomp_us = start.elapsed().as_micros() as u64;

            let decomp_produced = this.decompressed_buf.len();
            for s in this.stats.iter() {
                s.add_direction_wire_bytes(*this.direction, (len + 4) as u64);
                s.record_decompression(*this.direction, decomp_us);
                if *this.record_raw_metrics {
                    s.add_direction_raw_bytes(*this.direction, decomp_produced as u64);
                }
            }

            // Reset chunk state for next iteration
            *this.header_pos = 0;
            *this.chunk_len = 0;
            *this.payload_pos = 0;

            // Output decompressed bytes
            let avail = &this.decompressed_buf[*this.decompressed_pos..];
            let to_copy = std::cmp::min(avail.len(), buf.remaining());
            buf.put_slice(&avail[..to_copy]);
            *this.decompressed_pos += to_copy;
            if *this.decompressed_pos >= this.decompressed_buf.len() {
                this.decompressed_buf.clear();
                *this.decompressed_pos = 0;
            }
            return Poll::Ready(Ok(()));
        }
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    #[test]
    fn test_batcher_defer_and_size_threshold() {
        let config = BatcherConfig {
            flush_interval: Duration::from_millis(20),
            buffer_threshold: 64 * 1024,
        };
        let mut batcher = Batcher::new(config);
        let base_time = Instant::now();

        // 1. Adding small defer frames waits (does not flush)
        let small_frame = vec![0x42; 100];
        assert!(
            batcher
                .push_at(&small_frame, FramePriority::Defer, base_time)
                .is_none()
        );
        assert_eq!(batcher.buffered_len(), 100);

        // 2. Another frame at t = 10ms still waits (< 20ms and < 64KB)
        let t_10ms = base_time + Duration::from_millis(10);
        assert!(
            batcher
                .push_at(&small_frame, FramePriority::Defer, t_10ms)
                .is_none()
        );
        assert_eq!(batcher.buffered_len(), 200);

        // 3. Frame at t = 20ms triggers flush
        let t_20ms = base_time + Duration::from_millis(20);
        let flushed = batcher
            .push_at(&small_frame, FramePriority::Defer, t_20ms)
            .expect("should flush at 20ms");
        assert_eq!(flushed.len(), 300);
        assert!(batcher.is_empty());

        // 4. Large frame exceeding 64KB triggers immediate flush on size
        let large_frame = vec![0xaa; 64 * 1024];
        let flushed_large = batcher
            .push_at(&large_frame, FramePriority::Defer, t_20ms)
            .expect("should flush immediately upon reaching 64KB");
        assert_eq!(flushed_large.len(), 64 * 1024);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_batcher_timer_check() {
        let config = BatcherConfig {
            flush_interval: Duration::from_millis(20),
            buffer_threshold: 64 * 1024,
        };
        let mut batcher = Batcher::new(config);
        let base_time = Instant::now();

        batcher.push_at(&[1, 2, 3], FramePriority::Defer, base_time);

        // Not yet expired at 19ms
        assert!(
            batcher
                .check_timer_at(base_time + Duration::from_millis(19))
                .is_none()
        );

        // Expired at 20ms
        let flushed = batcher
            .check_timer_at(base_time + Duration::from_millis(20))
            .expect("should flush at 20ms timer check");
        assert_eq!(flushed, vec![1, 2, 3]);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_batcher_urgent_frame_flushes_immediately() {
        let mut batcher = Batcher::with_defaults();
        let base_time = Instant::now();

        // Queue a defer frame
        batcher.push_at(&[0x01, 0x02], FramePriority::Defer, base_time);
        assert_eq!(batcher.buffered_len(), 2);

        // Urgent frame arrives at t = 1ms (far before 20ms or 64KB)
        let t_1ms = base_time + Duration::from_millis(1);
        let urgent_frame = &[0x99, 0x88];
        let flushed = batcher
            .push_at(urgent_frame, FramePriority::Urgent, t_1ms)
            .expect("urgent frame must flush immediately");

        assert_eq!(flushed, vec![0x01, 0x02, 0x99, 0x88]);
        assert!(batcher.is_empty());
    }

    #[test]
    fn test_zstd_stream_multi_packet_roundtrip_and_sliding_window_compression() {
        let mut compressor = ZstdStreamCompressor::with_defaults().unwrap();
        let mut decompressor = ZstdStreamDecompressor::with_defaults().unwrap();

        // Simulate typical game/protocol telemetry with redundant structures
        let base_payload =
            b"entity_id:12345;x:100.5;y:64.0;z:-250.25;motion:standing;biome:plains;";
        let mut packet1 = Vec::new();
        for _ in 0..60 {
            packet1.extend_from_slice(base_payload);
        }

        // --- Batch 1 ---
        let compressed1 = compressor.compress_batch(&packet1).unwrap();
        let decompressed1 = decompressor.decompress_chunk(&compressed1).unwrap();
        assert_eq!(decompressed1, packet1);

        // --- Batch 2 (identical or near-identical repetitive telemetry) ---
        let packet2 = packet1.clone();
        let compressed2 = compressor.compress_batch(&packet2).unwrap();
        let decompressed2 = decompressor.decompress_chunk(&compressed2).unwrap();
        assert_eq!(decompressed2, packet2);

        // --- Batch 3 ---
        let packet3 = packet1.clone();
        let compressed3 = compressor.compress_batch(&packet3).unwrap();
        let decompressed3 = decompressor.decompress_chunk(&compressed3).unwrap();
        assert_eq!(decompressed3, packet3);

        // Verify that 8MB sliding dictionary history achieves dramatic compression!
        // The subsequent batches exploit previous dictionary state:
        let original_size = packet1.len();
        let batch2_compressed_size = compressed2.len();
        let compression_ratio =
            (original_size - batch2_compressed_size) as f64 / original_size as f64;

        println!(
            "Original: {} bytes, Batch1 comp: {} bytes, Batch2 comp: {} bytes, Ratio: {:.2}%",
            original_size,
            compressed1.len(),
            batch2_compressed_size,
            compression_ratio * 100.0
        );

        // Ratio for subsequent batches should comfortably exceed 80% (typically 95%+)
        assert!(
            compression_ratio >= 0.80,
            "expected at least 80% compression ratio on subsequent packets, got {:.2}%",
            compression_ratio * 100.0
        );
        assert!(
            compressed2.len() < compressed1.len(),
            "subsequent chunk ({}) should be smaller than first chunk ({}) due to dictionary reuse",
            compressed2.len(),
            compressed1.len()
        );
    }

    #[test]
    fn test_encode_batch_and_decode_stream() {
        let mut compressor = ZstdStreamCompressor::with_defaults().unwrap();
        let mut decompressor = ZstdStreamDecompressor::with_defaults().unwrap();

        let chunk1_data = b"chunk 1 data: user payload hello world";
        let chunk2_data = b"chunk 2 data: subsequent message with state";

        let mut stream = Vec::new();
        stream.extend(encode_batch(&mut compressor, chunk1_data).unwrap());
        stream.extend(encode_batch(&mut compressor, chunk2_data).unwrap());

        let decoded = decode_stream(&mut decompressor, &stream).unwrap();
        let mut expected = Vec::new();
        expected.extend_from_slice(chunk1_data);
        expected.extend_from_slice(chunk2_data);
        assert_eq!(decoded, expected);
    }

    #[tokio::test]
    async fn test_optimized_writer_and_reader_roundtrip() {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);

        let mut writer = OptimizedWriter::with_defaults(client_io).unwrap();
        let mut reader = OptimizedReader::with_defaults(server_io).unwrap();

        let message = b"Hello from OptimizedWriter through PRPX stream!";

        // Write a frame as urgent to flush immediately
        writer
            .write_frame(message, FramePriority::Urgent)
            .await
            .unwrap();

        let mut received = vec![0u8; message.len()];
        reader.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, message);

        // Test multi-packet streaming with defer frames followed by explicit flush
        let msg2 = b"Second message defer aggregated";
        writer
            .write_frame(msg2, FramePriority::Defer)
            .await
            .unwrap();
        writer.flush_batch().await.unwrap();

        let mut received2 = vec![0u8; msg2.len()];
        reader.read_exact(&mut received2).await.unwrap();
        assert_eq!(&received2, msg2);
    }

    #[tokio::test]
    async fn test_transparent_async_write_read() {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);

        let mut writer = OptimizedWriter::with_defaults(client_io).unwrap();
        let mut reader = OptimizedReader::with_defaults(server_io).unwrap();

        // Use standard AsyncWriteExt methods
        writer
            .write_all(b"Testing standard AsyncWrite implementation")
            .await
            .unwrap();
        writer.flush().await.unwrap();

        let mut buf = vec![0u8; 42];
        reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"Testing standard AsyncWrite implementation");
    }

    #[tokio::test]
    async fn test_optimized_writer_flush_if_due_and_timer() {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);

        let config = BatcherConfig {
            flush_interval: Duration::from_millis(20),
            buffer_threshold: 64 * 1024,
        };
        let mut writer =
            OptimizedWriter::new(client_io, config, CompressorConfig::default()).unwrap();
        let mut reader = OptimizedReader::with_defaults(server_io).unwrap();

        assert_eq!(writer.time_until_flush(), None);

        // Push a defer frame
        writer
            .write_frame(b"deferred piece", FramePriority::Defer)
            .await
            .unwrap();
        assert!(writer.time_until_flush().is_some());

        // Immediately checking flush_if_due should be false (< 20ms)
        assert!(!writer.flush_if_due().await.unwrap());

        // Wait 25ms
        tokio::time::sleep(Duration::from_millis(25)).await;

        // Now flush_if_due should be true!
        assert!(writer.flush_if_due().await.unwrap());

        let mut buf = vec![0u8; b"deferred piece".len()];
        reader.read_exact(&mut buf).await.unwrap();
        assert_eq!(&buf, b"deferred piece");
    }

    #[test]
    fn test_compressor_decompressor_reset_and_configs() {
        let mut compressor = ZstdStreamCompressor::with_defaults().unwrap();
        let mut decompressor = ZstdStreamDecompressor::with_defaults().unwrap();

        assert_eq!(compressor.config().compression_level, DEFAULT_ZSTD_LEVEL);
        assert_eq!(compressor.config().window_log, DEFAULT_ZSTD_WINDOW_LOG);
        assert_eq!(decompressor.config().window_log, DEFAULT_ZSTD_WINDOW_LOG);

        let data = b"Reset test data";
        let comp = compressor.compress_batch(data).unwrap();
        let decomp = decompressor.decompress_chunk(&comp).unwrap();
        assert_eq!(decomp, data);

        // Reset both
        compressor.reset().unwrap();
        decompressor.reset().unwrap();

        let comp2 = compressor.compress_batch(data).unwrap();
        let decomp2 = decompressor.decompress_chunk(&comp2).unwrap();
        assert_eq!(decomp2, data);
    }

    #[test]
    fn test_config_conversions() {
        let doc = ManagedOptimizerDocument {
            enabled: true,
            flush_interval_ms: Some(15),
            zstd_window_log: Some(22),
            zstd_level: Some(5),
        };
        let cfg = OptimizerConfig::from(&doc);
        assert!(cfg.enabled);
        assert_eq!(cfg.flush_interval, Duration::from_millis(15));
        assert_eq!(cfg.zstd_window_log, 22);
        assert_eq!(cfg.zstd_level, 5);

        let client_doc = ManagedOptimizerClientDocument {
            enabled: true,
            zstd_window_log: Some(21),
        };
        let client_cfg = OptimizerConfig::from(&client_doc);
        assert!(client_cfg.enabled);
        assert_eq!(client_cfg.zstd_window_log, 21);
    }

    #[tokio::test]
    async fn test_traffic_stats_collection() {
        let stats = Arc::new(OptimizerStats::new());
        let mut sink = Vec::new();
        let mut writer = OptimizedWriter::with_defaults(&mut sink)
            .unwrap()
            .with_stats(stats.clone());

        // 1. Write an urgent frame
        let urgent_msg = b"URGENT_PACKET_PING";
        writer
            .write_frame(urgent_msg, FramePriority::Urgent)
            .await
            .unwrap();

        let snap1 = stats.snapshot();
        assert_eq!(snap1.raw_bytes, urgent_msg.len() as u64);
        assert!(snap1.wire_bytes > 0);
        assert_eq!(snap1.urgent_batches, 1);
        assert_eq!(snap1.threshold_batches, 0);
        assert_eq!(snap1.timer_batches, 0);

        // 2. Write defer frames that accumulate, then flush
        let defer_msg = vec![0x42u8; 1000];
        writer
            .write_frame(&defer_msg, FramePriority::Defer)
            .await
            .unwrap();
        // Check timer/threshold not triggered yet
        assert_eq!(stats.snapshot().threshold_batches, 0);

        writer.flush_batch().await.unwrap();
        let snap2 = stats.snapshot();
        assert_eq!(snap2.raw_bytes, (urgent_msg.len() + defer_msg.len()) as u64);
        assert_eq!(snap2.threshold_batches, 1);
        assert!(snap2.saved_bytes > 0);
        assert!(snap2.saved_ratio > 0.0);
    }

    #[tokio::test]
    async fn test_write_frame_with_metric_prevents_zip_bomb_inflation() {
        let stats = Arc::new(OptimizerStats::new());
        let mut sink = Vec::new();
        let mut writer = OptimizedWriter::with_defaults(&mut sink)
            .unwrap()
            .with_stats(stats.clone());

        // Simulate a 100-byte Deflate packet that decompresses to 10,000 bytes (100x inflation)
        let incoming_deflate_wire_len = 100;
        let decompressed_payload = vec![0x33u8; 10_000];

        writer
            .write_frame_with_metric(
                incoming_deflate_wire_len,
                &decompressed_payload,
                FramePriority::Urgent,
            )
            .await
            .unwrap();

        let snap = stats.snapshot();
        // The raw_bytes metric MUST record the incoming wire length (100 bytes), NOT 10,000 bytes!
        assert_eq!(
            snap.raw_bytes, incoming_deflate_wire_len as u64,
            "raw_bytes must be anchored to incoming Deflate wire size to prevent zip-bomb inflation"
        );
        assert!(snap.wire_bytes > 0);
    }

    #[tokio::test]
    async fn test_directional_and_latency_metrics() {
        let stats = Arc::new(OptimizerStats::new());
        let (client_tx, mut server_rx) = tokio::io::duplex(64 * 1024);
        let (mut server_tx, client_rx) = tokio::io::duplex(64 * 1024);

        // Client writer sends Uplink to server
        let mut client_writer = OptimizedWriter::with_defaults(client_tx)
            .unwrap()
            .with_direction(TrafficDirection::Uplink)
            .with_stats(stats.clone());

        // Client reader receives Downlink from server
        let mut client_reader = OptimizedReader::with_defaults(client_rx)
            .unwrap()
            .with_direction(TrafficDirection::Downlink)
            .with_stats(stats.clone());

        // 1. Client sends 5000 bytes Uplink
        let up_payload = vec![0x77u8; 5000];
        client_writer
            .write_frame(&up_payload, FramePriority::Urgent)
            .await
            .unwrap();

        // Server receives compressed chunks
        let mut server_decompressor = ZstdStreamDecompressor::with_defaults().unwrap();
        let mut server_buf = [0u8; 4096];
        let n = server_rx.read(&mut server_buf).await.unwrap();
        let decomp = decode_stream(&mut server_decompressor, &server_buf[..n]).unwrap();
        assert_eq!(decomp, up_payload);

        // 2. Server sends 3000 bytes Downlink to client
        let down_payload = vec![0x88u8; 3000];
        let mut server_compressor = ZstdStreamCompressor::with_defaults().unwrap();
        let framed_down = encode_batch(&mut server_compressor, &down_payload).unwrap();
        server_tx.write_all(&framed_down).await.unwrap();

        let mut client_read_buf = vec![0u8; 3000];
        client_reader
            .read_exact(&mut client_read_buf)
            .await
            .unwrap();
        assert_eq!(client_read_buf, down_payload);

        let snap = stats.snapshot();
        // Uplink: 5000 raw bytes, wire < 5000
        assert_eq!(snap.uplink.raw_bytes, 5000);
        assert!(snap.uplink.wire_bytes > 0);
        assert!(snap.uplink.wire_bytes < 5000);
        assert!(snap.uplink.saved_bytes > 0);
        assert_eq!(snap.uplink.batches, 1);
        assert!(snap.uplink.est_transfer_time_saved_ms > 0.0);

        // Downlink: 3000 raw bytes, wire < 3000
        assert_eq!(snap.downlink.raw_bytes, 3000);
        assert!(snap.downlink.wire_bytes > 0);
        assert!(snap.downlink.wire_bytes < 3000);
        assert!(snap.downlink.saved_bytes > 0);
        assert_eq!(snap.downlink.batches, 1);
        assert!(snap.downlink.est_transfer_time_saved_ms > 0.0);

        // Total aggregate
        assert_eq!(snap.raw_bytes, 8000);
        assert_eq!(
            snap.wire_bytes,
            snap.uplink.wire_bytes + snap.downlink.wire_bytes
        );
        assert_eq!(
            snap.saved_bytes,
            snap.uplink.saved_bytes + snap.downlink.saved_bytes
        );
        assert!(snap.est_transfer_time_saved_ms > 0.0);
    }
}
