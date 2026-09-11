//! Native Traffic Pipeline (traffic optimizer) for Prism.
//!
//! Provides high-performance time-slice aggregation (`Batcher`) and continuous
//! stateful Zstandard compression/decompression (`ZstdStreamCompressor` and `ZstdStreamDecompressor`).

#![allow(dead_code)]

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};

use pin_project_lite::pin_project;
use tokio::io::{AsyncRead, AsyncWrite, ReadBuf};
use zstd::stream::raw::{CParameter, DParameter, Decoder, Encoder, InBuffer, Operation, OutBuffer};

use crate::prism::config::{
    ManagedOptimizerClientDocument, ManagedOptimizerDocument, OptimizerClientConfig,
    OptimizerConfig as PrismOptimizerConfig,
};
use crate::prism::middleware::FramePriority;

mod dict;
pub mod pipeline;
mod stats;
pub use dict::*;
pub use stats::*;

pub const DEFAULT_FLUSH_INTERVAL: Duration = Duration::from_millis(20);
pub const DEFAULT_FLUSH_INTERVAL_MIN: Duration = Duration::from_millis(8);
pub const DEFAULT_FLUSH_INTERVAL_MAX: Duration = Duration::from_millis(50);
pub const DEFAULT_BUFFER_THRESHOLD: usize = 64 * 1024; // 64 KB
pub const DEFAULT_BUFFER_THRESHOLD_UPLINK: usize = 16 * 1024;
pub const DEFAULT_ZSTD_LEVEL: i32 = 3;
pub const DEFAULT_ZSTD_WINDOW_LOG: u32 = 23; // 8 MB sliding window
pub const DEFAULT_ZSTD_WINDOW_LOG_UPLINK: u32 = 18; // 256 KB
pub const MAX_CHUNK_SIZE: usize = 32 * 1024 * 1024; // 32 MB guard limit

/// Bits 0-29 of the 4-byte frame header are the payload length.
pub const FRAME_LEN_MASK: u32 = 0x3FFF_FFFF;
/// Payload is stored uncompressed (compressor would have expanded it).
pub const FRAME_FLAG_RAW: u32 = 1 << 31;
/// Payload is opaque session data for the peer middleware, not game bytes.
pub const FRAME_FLAG_CONTROL: u32 = 1 << 30;

fn pack_frame_header(len: usize, flags: u32) -> [u8; 4] {
    (((len as u32) & FRAME_LEN_MASK) | flags).to_be_bytes()
}

fn unpack_frame_header(header: [u8; 4]) -> (usize, u32) {
    let raw = u32::from_be_bytes(header);
    ((raw & FRAME_LEN_MASK) as usize, raw & !FRAME_LEN_MASK)
}

// ============================================================================
// Configuration
// ============================================================================

/// Configuration for the Native Optimizer pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizerConfig {
    pub enabled: bool,
    pub flush_interval: Duration,
    pub flush_interval_min: Duration,
    pub flush_interval_max: Duration,
    pub adaptive_flush: bool,
    pub buffer_threshold: usize,
    pub buffer_threshold_uplink: usize,
    pub zstd_level: i32,
    pub zstd_window_log: u32,
    pub zstd_window_log_uplink: u32,
    pub zstd_window_log_downlink: u32,
    pub dictionary: Option<Vec<u8>>,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            flush_interval_min: DEFAULT_FLUSH_INTERVAL_MIN,
            flush_interval_max: DEFAULT_FLUSH_INTERVAL_MAX,
            adaptive_flush: true,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            buffer_threshold_uplink: DEFAULT_BUFFER_THRESHOLD_UPLINK,
            zstd_level: DEFAULT_ZSTD_LEVEL,
            zstd_window_log: DEFAULT_ZSTD_WINDOW_LOG,
            zstd_window_log_uplink: DEFAULT_ZSTD_WINDOW_LOG_UPLINK,
            zstd_window_log_downlink: DEFAULT_ZSTD_WINDOW_LOG,
            dictionary: None,
        }
    }
}

impl OptimizerConfig {
    pub fn window_log_for(&self, direction: TrafficDirection) -> u32 {
        match direction {
            TrafficDirection::Uplink => self.zstd_window_log_uplink,
            TrafficDirection::Downlink => self.zstd_window_log_downlink,
        }
    }

    pub fn buffer_threshold_for(&self, direction: TrafficDirection) -> usize {
        match direction {
            TrafficDirection::Uplink => self.buffer_threshold_uplink,
            TrafficDirection::Downlink => self.buffer_threshold,
        }
    }

    pub fn dictionary_id(&self) -> u32 {
        self.dictionary
            .as_deref()
            .map(dictionary_id)
            .unwrap_or(0)
    }
}

fn opt_ms(v: Option<u64>, default: u64) -> Duration {
    Duration::from_millis(v.unwrap_or(default))
}

impl From<&ManagedOptimizerDocument> for OptimizerConfig {
    fn from(doc: &ManagedOptimizerDocument) -> Self {
        let window = doc.zstd_window_log.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG);
        Self {
            enabled: doc.enabled,
            flush_interval: opt_ms(doc.flush_interval_ms, 20),
            flush_interval_min: opt_ms(doc.flush_interval_min_ms, 8),
            flush_interval_max: opt_ms(doc.flush_interval_max_ms, 50),
            adaptive_flush: doc.adaptive_flush.unwrap_or(true),
            buffer_threshold: doc.buffer_threshold.unwrap_or(DEFAULT_BUFFER_THRESHOLD),
            buffer_threshold_uplink: doc
                .buffer_threshold_uplink
                .unwrap_or(DEFAULT_BUFFER_THRESHOLD_UPLINK),
            zstd_level: doc.zstd_level.unwrap_or(DEFAULT_ZSTD_LEVEL),
            zstd_window_log: window,
            zstd_window_log_uplink: doc.zstd_window_log_uplink.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG_UPLINK),
            zstd_window_log_downlink: doc.zstd_window_log_downlink.unwrap_or(window),
            dictionary: resolve_dictionary(doc.zstd_dictionary.as_deref(), ""),
        }
    }
}

impl From<&ManagedOptimizerClientDocument> for OptimizerConfig {
    fn from(doc: &ManagedOptimizerClientDocument) -> Self {
        let window = doc.zstd_window_log.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG);
        Self {
            enabled: doc.enabled,
            zstd_window_log: window,
            zstd_window_log_uplink: doc
                .zstd_window_log_uplink
                .unwrap_or(DEFAULT_ZSTD_WINDOW_LOG_UPLINK),
            zstd_window_log_downlink: doc.zstd_window_log_downlink.unwrap_or(window),
            dictionary: resolve_dictionary(doc.zstd_dictionary.as_deref(), ""),
            ..Self::default()
        }
    }
}

impl From<&PrismOptimizerConfig> for OptimizerConfig {
    fn from(cfg: &PrismOptimizerConfig) -> Self {
        let window = cfg.zstd_window_log();
        Self {
            enabled: cfg.enabled,
            flush_interval: Duration::from_millis(cfg.flush_interval_ms()),
            flush_interval_min: Duration::from_millis(cfg.flush_interval_min_ms()),
            flush_interval_max: Duration::from_millis(cfg.flush_interval_max_ms()),
            adaptive_flush: cfg.adaptive_flush(),
            buffer_threshold: cfg.buffer_threshold(),
            buffer_threshold_uplink: cfg.buffer_threshold_uplink(),
            zstd_level: cfg.zstd_level(),
            zstd_window_log: window,
            zstd_window_log_uplink: cfg.zstd_window_log_uplink(),
            zstd_window_log_downlink: cfg.zstd_window_log_downlink(),
            dictionary: resolve_dictionary(cfg.zstd_dictionary.as_deref(), ""),
        }
    }
}

impl From<&OptimizerClientConfig> for OptimizerConfig {
    fn from(cfg: &OptimizerClientConfig) -> Self {
        let window = cfg.zstd_window_log();
        Self {
            enabled: cfg.enabled,
            zstd_window_log: window,
            zstd_window_log_uplink: cfg.zstd_window_log_uplink(),
            zstd_window_log_downlink: cfg.zstd_window_log_downlink(),
            dictionary: resolve_dictionary(cfg.zstd_dictionary.as_deref(), ""),
            ..Self::default()
        }
    }
}

/// Picks a flush interval between `min` and `max` from observed link rate and savings.
pub fn adaptive_flush_interval(
    base: Duration,
    min: Duration,
    max: Duration,
    stats: Option<&OptimizerStats>,
) -> Duration {
    let Some(stats) = stats else {
        return base;
    };
    let snap = stats.snapshot();
    let min_ms = min.as_millis() as f64;
    let max_ms = max.as_millis() as f64;
    let base_ms = base.as_millis() as f64;
    if max_ms <= min_ms {
        return base;
    }

    let mut ms = base_ms;
    if snap.link_rate_measured {
        // Fast links: cut queuing delay. Slow links: wait for more compressible data.
        let rate = snap.link_rate_bps;
        if rate >= 100_000_000.0 {
            ms = min_ms;
        } else if rate <= 10_000_000.0 {
            ms = max_ms;
        } else {
            let t = (rate - 10_000_000.0) / 90_000_000.0;
            ms = max_ms + (min_ms - max_ms) * t;
        }
    }
    if snap.window.saved_ratio >= 0.50 {
        ms = ms.max(base_ms);
    }
    Duration::from_millis(ms.round().clamp(min_ms, max_ms) as u64)
}

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
    /// Updates the time-slice interval (used by adaptive flush).
    pub fn set_flush_interval(&mut self, interval: Duration) {
        self.config.flush_interval = interval.max(Duration::from_millis(1));
    }

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
    pub dictionary: Option<Vec<u8>>,
}

impl Default for CompressorConfig {
    fn default() -> Self {
        Self {
            compression_level: DEFAULT_ZSTD_LEVEL,
            window_log: DEFAULT_ZSTD_WINDOW_LOG,
            dictionary: None,
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
    fn build_encoder(config: &CompressorConfig) -> io::Result<Encoder<'static>> {
        let mut encoder = if let Some(ref dict) = config.dictionary {
            Encoder::with_dictionary(config.compression_level, dict)?
        } else {
            Encoder::new(config.compression_level)?
        };
        encoder.set_parameter(CParameter::WindowLog(config.window_log))?;
        encoder.set_parameter(CParameter::ChecksumFlag(false))?;
        Ok(encoder)
    }

    /// Creates a new continuous stream compressor with the specified configuration.
    pub fn new(config: CompressorConfig) -> io::Result<Self> {
        let encoder = Self::build_encoder(&config)?;
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
        self.encoder = Self::build_encoder(&self.config)?;
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
    pub dictionary: Option<Vec<u8>>,
}

impl Default for DecompressorConfig {
    fn default() -> Self {
        Self {
            window_log: DEFAULT_ZSTD_WINDOW_LOG,
            dictionary: None,
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
    fn build_decoder(config: &DecompressorConfig) -> io::Result<Decoder<'static>> {
        let mut decoder = if let Some(ref dict) = config.dictionary {
            Decoder::with_dictionary(dict)?
        } else {
            Decoder::new()?
        };
        decoder.set_parameter(DParameter::WindowLogMax(config.window_log))?;
        Ok(decoder)
    }

    /// Creates a new continuous stream decompressor with the specified configuration.
    pub fn new(config: DecompressorConfig) -> io::Result<Self> {
        let decoder = Self::build_decoder(&config)?;
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
        self.decoder = Self::build_decoder(&self.config)?;
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

/// Encodes a raw batch into an existing buffer as `[header: u32 BE][payload]`.
///
/// The header stores the payload length in the low 30 bits. [`FRAME_FLAG_RAW`] is set
/// when zstd would not shrink the batch, so the payload is the original bytes.
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
    // Always emit a zstd block. Sending a parallel uncompressed payload would
    // desynchronize the decoder's sliding window, which is built from decoded
    // zstd output. zstd already stores incompressible input as raw blocks.
    let payload_len = output.len() - payload_start;
    let header = pack_frame_header(payload_len, 0);
    output[header_start..header_start + 4].copy_from_slice(&header);

    Ok(payload_len + 4)
}

/// Encodes an opaque control payload (never compressed, never delivered as stream data).
pub fn encode_control_frame(payload: &[u8], output: &mut Vec<u8>) -> io::Result<usize> {
    if payload.is_empty() {
        return Ok(0);
    }
    if payload.len() > MAX_CHUNK_SIZE {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("control frame {} exceeds max {MAX_CHUNK_SIZE}", payload.len()),
        ));
    }
    output.extend_from_slice(&pack_frame_header(payload.len(), FRAME_FLAG_CONTROL));
    output.extend_from_slice(payload);
    Ok(payload.len() + 4)
}

/// Decodes a sequence of length-prefixed chunks (`[u32 BE][payload]`) from a byte slice.
///
/// Control frames are skipped (they are not stream data). Use [`OptimizedReader`] to
/// surface control payloads to middleware.
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
        let header: [u8; 4] = framed_bytes[..4].try_into().unwrap();
        let (chunk_len, flags) = unpack_frame_header(header);
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
        if flags & FRAME_FLAG_CONTROL != 0 {
            continue;
        }
        if flags & FRAME_FLAG_RAW != 0 {
            decompressed.extend_from_slice(payload);
        } else {
            decompressor.decompress_chunk_into(payload, &mut decompressed)?;
        }
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
        flush_interval: Duration,
        flush_interval_min: Duration,
        flush_interval_max: Duration,
        adaptive_flush: bool,
    }
}

impl<W> OptimizedWriter<W> {
    /// Creates a new `OptimizedWriter` with given configurations.
    pub fn new(
        inner: W,
        batcher_config: BatcherConfig,
        compressor_config: CompressorConfig,
    ) -> io::Result<Self> {
        let flush_interval = batcher_config.flush_interval;
        Ok(Self {
            inner,
            batcher: Batcher::new(batcher_config),
            compressor: ZstdStreamCompressor::new(compressor_config)?,
            write_buf: Vec::new(),
            write_pos: 0,
            stats: Vec::new(),
            direction: TrafficDirection::Uplink,
            flush_interval,
            flush_interval_min: DEFAULT_FLUSH_INTERVAL_MIN,
            flush_interval_max: DEFAULT_FLUSH_INTERVAL_MAX,
            adaptive_flush: true,
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
        let now_ms = unix_ms();
        for s in &self.stats {
            s.add_direction_raw_bytes(self.direction, bytes as u64, now_ms);
        }
    }

    /// Records one flushed batch.
    ///
    /// `framed_bytes` is the complete on-wire frame (`[len: u32 BE][payload]`) exactly
    /// as returned by [`encode_batch_into`], so both ends account for the same bytes.
    fn record_batch(&self, framed_bytes: usize, compression_us: u64, queue_delay_us: u64) {
        let now_ms = unix_ms();
        for s in &self.stats {
            s.record_batch(
                self.direction,
                framed_bytes as u64,
                queue_delay_us,
                compression_us,
                now_ms,
            );
        }
    }

    /// Records how long the underlying link spent draining `bytes`.
    fn record_link(&self, bytes: usize, busy: Duration) {
        if bytes == 0 {
            return;
        }
        let now_ms = unix_ms();
        let busy_us = busy.as_micros() as u64;
        for s in &self.stats {
            s.record_link_sample_at(now_ms, bytes as u64, busy_us);
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

    fn record_explicit(&self) {
        for s in &self.stats {
            s.inc_explicit();
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

    /// Configures adaptive flush bounds. `base` is the configured interval.
    pub fn with_adaptive_flush(
        mut self,
        enabled: bool,
        base: Duration,
        min: Duration,
        max: Duration,
    ) -> Self {
        self.adaptive_flush = enabled;
        self.flush_interval = base;
        self.flush_interval_min = min;
        self.flush_interval_max = max;
        self.batcher.set_flush_interval(base);
        self
    }

    fn apply_adaptive_flush(&mut self) {
        if !self.adaptive_flush {
            return;
        }
        let stats = self.stats.first().map(|s| s.as_ref());
        let next = adaptive_flush_interval(
            self.flush_interval,
            self.flush_interval_min,
            self.flush_interval_max,
            stats,
        );
        self.batcher.set_flush_interval(next);
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
        self.apply_adaptive_flush();

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
            let start = Instant::now();
            let framed = encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            let comp_us = start.elapsed().as_micros() as u64;
            self.record_batch(framed, comp_us, queue_delay);
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
            self.record_explicit();
            let start = Instant::now();
            let framed = encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            let comp_us = start.elapsed().as_micros() as u64;
            self.record_batch(framed, comp_us, queue_delay);
            self.flush_pending_write_buf().await?;
        }

        tokio::io::AsyncWriteExt::flush(&mut self.inner).await?;
        Ok(())
    }

    /// Checks if a time-slice flush is due and writes the flushed batch to `inner`.
    /// Returns `true` if a batch was flushed.
    pub async fn flush_if_due(&mut self) -> io::Result<bool> {
        self.apply_adaptive_flush();
        let queue_delay = self
            .batcher
            .first_frame_at()
            .map(|t| t.elapsed().as_micros() as u64)
            .unwrap_or(0);
        if let Some(batch) = self.batcher.check_timer() {
            self.flush_pending_write_buf().await?;
            self.record_timer();
            let start = Instant::now();
            let framed = encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            let comp_us = start.elapsed().as_micros() as u64;
            self.record_batch(framed, comp_us, queue_delay);
            self.flush_pending_write_buf().await?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Writes an opaque control payload immediately (not batched, not compressed).
    pub async fn write_control(&mut self, payload: &[u8]) -> io::Result<()> {
        self.flush_pending_write_buf().await?;
        encode_control_frame(payload, &mut self.write_buf)?;
        self.flush_pending_write_buf().await?;
        Ok(())
    }

    async fn flush_pending_write_buf(&mut self) -> io::Result<()> {
        let pending = (self.write_buf.len() - self.write_pos) as u64;
        let start = Instant::now();
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
        self.record_link(pending as usize, start.elapsed());
        Ok(())
    }
}

/// Records one flushed batch for pinned (poll-based) writer paths.
fn record_batch_pinned(
    stats: &[SharedOptimizerStats],
    direction: TrafficDirection,
    framed_bytes: usize,
    compression_us: u64,
    queue_delay_us: u64,
) {
    let now_ms = unix_ms();
    for s in stats {
        s.record_batch(
            direction,
            framed_bytes as u64,
            queue_delay_us,
            compression_us,
            now_ms,
        );
    }
}

/// Records one link drain for pinned (poll-based) writer paths.
fn record_link_pinned(stats: &[SharedOptimizerStats], bytes: usize, busy: Duration) {
    if bytes == 0 {
        return;
    }
    let now_ms = unix_ms();
    let busy_us = busy.as_micros() as u64;
    for s in stats {
        s.record_link_sample_at(now_ms, bytes as u64, busy_us);
    }
}

/// Drains the pending write buffer through the pinned inner writer while recording how
/// long the link spent moving those bytes.
fn poll_drain_pinned<W: AsyncWrite>(
    inner: Pin<&mut W>,
    write_buf: &mut Vec<u8>,
    write_pos: &mut usize,
    stats: &[SharedOptimizerStats],
    cx: &mut Context<'_>,
) -> Poll<io::Result<()>> {
    let pending = (write_buf.len() - *write_pos) as u64;
    let start = Instant::now();
    let result = poll_flush_write_buf_pinned(inner, write_buf, write_pos, cx);
    let remaining = (write_buf.len() - *write_pos) as u64;
    record_link_pinned(
        stats,
        pending.saturating_sub(remaining) as usize,
        start.elapsed(),
    );
    result
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

        // 1. Flush any pending write buffer. Stay pending while the link is
        //    back-pressured so the batcher cannot grow without bound.
        match poll_drain_pinned(
            this.inner.as_mut(),
            this.write_buf,
            this.write_pos,
            this.stats,
            cx,
        ) {
            Poll::Ready(Ok(())) => {}
            Poll::Ready(Err(e)) => return Poll::Ready(Err(e)),
            Poll::Pending => return Poll::Pending,
        }

        let now_ms = unix_ms();
        for s in this.stats.iter() {
            s.add_direction_raw_bytes(*this.direction, buf.len() as u64, now_ms);
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
            let start = Instant::now();
            let framed = match encode_batch_into(this.compressor, &batch, this.write_buf) {
                Ok(framed) => framed,
                Err(e) => return Poll::Ready(Err(e)),
            };
            let comp_us = start.elapsed().as_micros() as u64;
            record_batch_pinned(this.stats, *this.direction, framed, comp_us, queue_delay);
            *this.write_pos = 0;
            // Best effort immediate write
            let _ = poll_drain_pinned(
                this.inner.as_mut(),
                this.write_buf,
                this.write_pos,
                this.stats,
                cx,
            );
        }

        Poll::Ready(Ok(buf.len()))
    }

    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        let mut this = self.project();

        // Flush any pending write buffer
        match poll_drain_pinned(
            this.inner.as_mut(),
            this.write_buf,
            this.write_pos,
            this.stats,
            cx,
        ) {
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
                s.inc_explicit();
            }
            let start = Instant::now();
            let framed = match encode_batch_into(this.compressor, &batch, this.write_buf) {
                Ok(framed) => framed,
                Err(e) => return Poll::Ready(Err(e)),
            };
            let comp_us = start.elapsed().as_micros() as u64;
            record_batch_pinned(this.stats, *this.direction, framed, comp_us, queue_delay);
            *this.write_pos = 0;
            match poll_drain_pinned(
                this.inner.as_mut(),
                this.write_buf,
                this.write_pos,
                this.stats,
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
        pending_control: Vec<Vec<u8>>,
        control_tx: Option<tokio::sync::mpsc::UnboundedSender<Vec<u8>>>,
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
            pending_control: Vec::new(),
            control_tx: None,
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

    /// Drains control-frame payloads received since the last call.
    pub fn take_control_frames(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.pending_control)
    }

    /// Sends each incoming control payload on `tx` as soon as the frame is parsed,
    /// even if `poll_read` stays pending waiting for the next stream chunk.
    pub fn with_control_tx(
        mut self,
        tx: tokio::sync::mpsc::UnboundedSender<Vec<u8>>,
    ) -> Self {
        self.control_tx = Some(tx);
        self
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
                let (len, _flags) = unpack_frame_header(*this.header_buf);
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

            let (_len, flags) = unpack_frame_header(*this.header_buf);

            // Control frames are not stream data.
            if flags & FRAME_FLAG_CONTROL != 0 {
                let payload = this.payload_buf[..len].to_vec();
                this.pending_control.push(payload.clone());
                if let Some(tx) = this.control_tx.as_ref() {
                    let _ = tx.send(payload);
                }
                *this.header_pos = 0;
                *this.chunk_len = 0;
                *this.payload_pos = 0;
                continue;
            }

            this.decompressed_buf.clear();
            *this.decompressed_pos = 0;
            let start = Instant::now();
            if flags & FRAME_FLAG_RAW != 0 {
                this.decompressed_buf
                    .extend_from_slice(&this.payload_buf[..len]);
            } else if let Err(e) = this
                .decompressor
                .decompress_chunk_into(&this.payload_buf[..len], this.decompressed_buf)
            {
                return Poll::Ready(Err(e));
            }
            let decomp_us = start.elapsed().as_micros() as u64;

            let decomp_produced = this.decompressed_buf.len();
            let now_ms = unix_ms();
            for s in this.stats.iter() {
                s.record_chunk(*this.direction, (len + 4) as u64, decomp_us, now_ms);
                if *this.record_raw_metrics {
                    s.add_direction_raw_bytes(*this.direction, decomp_produced as u64, now_ms);
                }
            }

            // Reset chunk state for next iteration
            *this.header_pos = 0;
            *this.chunk_len = 0;
            *this.payload_pos = 0;

            if this.decompressed_buf.is_empty() {
                continue;
            }

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
    use std::sync::Arc;
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
            ..Default::default()
        };
        let cfg = OptimizerConfig::from(&doc);
        assert!(cfg.enabled);
        assert_eq!(cfg.flush_interval, Duration::from_millis(15));
        assert_eq!(cfg.zstd_window_log, 22);
        assert_eq!(cfg.zstd_level, 5);

        let client_doc = ManagedOptimizerClientDocument {
            enabled: true,
            zstd_window_log: Some(21),
            ..Default::default()
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
        assert_eq!(snap2.explicit_batches, 1);
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
        assert!(snap.uplink.transfer_gain_ms > 0.0);
        assert_eq!(
            snap.uplink.net_gain_ms,
            snap.uplink.transfer_gain_ms
                - snap.uplink.batching_penalty_ms
                - snap.uplink.compression_penalty_ms
        );
        assert_eq!(snap.uplink.window.batches, 1);
        assert_eq!(snap.uplink.window.wire_bytes, snap.uplink.wire_bytes);

        // Downlink: 3000 raw bytes, wire < 3000
        assert_eq!(snap.downlink.raw_bytes, 3000);
        assert!(snap.downlink.wire_bytes > 0);
        assert!(snap.downlink.wire_bytes < 3000);
        assert!(snap.downlink.saved_bytes > 0);
        assert_eq!(snap.downlink.batches, 1);
        assert!(snap.downlink.transfer_gain_ms > 0.0);

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
        assert!(snap.transfer_gain_ms > 0.0);
    }

    /// Both ends of the same stream must account for exactly the same on-wire bytes:
    /// the writer's framed length (header included) must match the reader's chunk length.
    #[tokio::test]
    async fn test_writer_and_reader_agree_on_wire_bytes() {
        let writer_stats = Arc::new(OptimizerStats::new());
        let reader_stats = Arc::new(OptimizerStats::new());
        let (client_io, server_io) = tokio::io::duplex(128 * 1024);

        let mut writer = OptimizedWriter::with_defaults(client_io)
            .unwrap()
            .with_direction(TrafficDirection::Uplink)
            .with_stats(writer_stats.clone());
        let mut reader = OptimizedReader::with_defaults(server_io)
            .unwrap()
            .with_direction(TrafficDirection::Uplink)
            .with_stats(reader_stats.clone());

        let payload = vec![0x5au8; 4096];
        writer
            .write_frame(&payload, FramePriority::Urgent)
            .await
            .unwrap();
        let mut received = vec![0u8; payload.len()];
        reader.read_exact(&mut received).await.unwrap();
        assert_eq!(received, payload);

        let written = writer_stats.snapshot().uplink.wire_bytes;
        let read = reader_stats.snapshot().uplink.wire_bytes;
        assert!(written > 4, "wire bytes must include the chunk header");
        assert_eq!(
            written, read,
            "writer wire bytes must include the 4-byte chunk header"
        );
    }

    /// The writer feeds the shared link estimator; a write that has to wait for the peer
    /// to drain the socket must surface as a measured rate instead of the fallback.
    #[tokio::test]
    async fn test_writer_records_link_drain_samples() {
        let stats = Arc::new(OptimizerStats::new());
        // Tiny duplex buffer: the writer can only make progress as fast as the peer drains.
        let (client_io, mut server_io) = tokio::io::duplex(1024);

        let drain = tokio::spawn(async move {
            let mut sink = vec![0u8; 1024];
            let mut reads = 0usize;
            loop {
                match server_io.read(&mut sink).await {
                    Ok(0) | Err(_) => break,
                    Ok(_) => {
                        // Force measurable backpressure so the estimator has drain time
                        // to weight, independent of machine speed.
                        if reads < 8 {
                            tokio::time::sleep(Duration::from_millis(2)).await;
                        }
                        reads += 1;
                    }
                }
            }
        });

        let mut writer = OptimizedWriter::with_defaults(client_io)
            .unwrap()
            .with_direction(TrafficDirection::Uplink)
            .with_stats(stats.clone());

        // Incompressible payload well past the duplex buffer, so the drain blocks.
        let mut payload = Vec::with_capacity(256 * 1024);
        let mut state = 0x2545_f491u32;
        for _ in 0..256 * 1024 {
            state ^= state << 13;
            state ^= state >> 17;
            state ^= state << 5;
            payload.push((state >> 11) as u8);
        }
        writer
            .write_frame(&payload, FramePriority::Urgent)
            .await
            .unwrap();
        writer.flush().await.unwrap();
        drop(writer);
        let _ = drain.await;

        let snap = stats.snapshot();
        assert!(
            snap.link_rate_measured,
            "blocked drain must produce a measured rate, got {snap:?}"
        );
        assert!(snap.link_rate_bytes > 0 && snap.link_rate_busy_us > 0);
        assert!(snap.link_rate_bps > 0.0 && snap.link_rate_bps <= MAX_LINK_RATE_BPS);
    }

    #[test]
    fn test_raw_frame_when_compression_would_expand() {
        let mut compressor = ZstdStreamCompressor::with_defaults().unwrap();
        let mut decompressor = ZstdStreamDecompressor::with_defaults().unwrap();
        let mut incompressible = Vec::with_capacity(256);
        let mut state = 0x9e37_79b9u32;
        for _ in 0..256 {
            state = state.wrapping_mul(1664525).wrapping_add(1013904223);
            incompressible.push((state >> 16) as u8);
        }
        let framed = encode_batch(&mut compressor, &incompressible).unwrap();
        let decoded = decode_stream(&mut decompressor, &framed).unwrap();
        assert_eq!(decoded, incompressible);
        let framed2 = encode_batch(&mut compressor, &incompressible).unwrap();
        let decoded2 = decode_stream(&mut decompressor, &framed2).unwrap();
        assert_eq!(decoded2, incompressible);
    }

    #[tokio::test]
    async fn test_control_frame_is_not_stream_data() {
        let (client_io, server_io) = tokio::io::duplex(64 * 1024);
        let mut writer = OptimizedWriter::with_defaults(client_io).unwrap();
        let mut reader = OptimizedReader::with_defaults(server_io).unwrap();

        writer.write_control(b"session-secret-16").await.unwrap();
        writer
            .write_frame(b"game-bytes", FramePriority::Urgent)
            .await
            .unwrap();

        let mut received = vec![0u8; 10];
        reader.read_exact(&mut received).await.unwrap();
        assert_eq!(&received, b"game-bytes");
        let ctrl = reader.take_control_frames();
        assert_eq!(ctrl, vec![b"session-secret-16".to_vec()]);
    }

    #[test]
    fn test_dictionary_roundtrip() {
        let sample = b"block:stone;block:dirt;entity:zombie;block:stone;".to_vec();
        let samples: Vec<Vec<u8>> = (0..40).map(|_| sample.clone()).collect();
        let dict = train_dictionary(&samples, 2048).unwrap();
        let cfg = CompressorConfig {
            dictionary: Some(dict.clone()),
            ..Default::default()
        };
        let dcfg = DecompressorConfig {
            dictionary: Some(dict),
            ..Default::default()
        };
        let mut c = ZstdStreamCompressor::new(cfg).unwrap();
        let mut d = ZstdStreamDecompressor::new(dcfg).unwrap();
        let framed = encode_batch(&mut c, &sample).unwrap();
        let out = decode_stream(&mut d, &framed).unwrap();
        assert_eq!(out, sample);
    }
}
