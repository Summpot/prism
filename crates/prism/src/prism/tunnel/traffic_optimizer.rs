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
    ManagedTrafficOptimizerClientDocument, ManagedTrafficOptimizerDocument,
    TrafficOptimizerClientConfig, TrafficOptimizerConfig as PrismTrafficOptimizerConfig,
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

/// Configuration for the Native Traffic Optimizer pipeline.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrafficOptimizerConfig {
    pub enabled: bool,
    pub flush_interval: Duration,
    pub buffer_threshold: usize,
    pub zstd_level: i32,
    pub zstd_window_log: u32,
}

impl Default for TrafficOptimizerConfig {
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

impl From<&ManagedTrafficOptimizerDocument> for TrafficOptimizerConfig {
    fn from(doc: &ManagedTrafficOptimizerDocument) -> Self {
        Self {
            enabled: doc.enabled,
            flush_interval: Duration::from_millis(doc.flush_interval_ms.unwrap_or(20)),
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: doc.zstd_level.unwrap_or(DEFAULT_ZSTD_LEVEL),
            zstd_window_log: doc.zstd_window_log.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG),
        }
    }
}

impl From<&ManagedTrafficOptimizerClientDocument> for TrafficOptimizerConfig {
    fn from(doc: &ManagedTrafficOptimizerClientDocument) -> Self {
        Self {
            enabled: doc.enabled,
            flush_interval: DEFAULT_FLUSH_INTERVAL,
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: DEFAULT_ZSTD_LEVEL,
            zstd_window_log: doc.zstd_window_log.unwrap_or(DEFAULT_ZSTD_WINDOW_LOG),
        }
    }
}

impl From<&PrismTrafficOptimizerConfig> for TrafficOptimizerConfig {
    fn from(cfg: &PrismTrafficOptimizerConfig) -> Self {
        Self {
            enabled: cfg.enabled,
            flush_interval: Duration::from_millis(cfg.flush_interval_ms()),
            buffer_threshold: DEFAULT_BUFFER_THRESHOLD,
            zstd_level: cfg.zstd_level(),
            zstd_window_log: cfg.zstd_window_log(),
        }
    }
}

impl From<&TrafficOptimizerClientConfig> for TrafficOptimizerConfig {
    fn from(cfg: &TrafficOptimizerClientConfig) -> Self {
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
        })
    }

    /// Creates a new `OptimizedWriter` with default configurations.
    pub fn with_defaults(inner: W) -> io::Result<Self> {
        Self::new(inner, BatcherConfig::default(), CompressorConfig::default())
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
    /// Writes a frame with the specified priority.
    ///
    /// If `FramePriority::Urgent` is given, or if the buffer/time threshold is reached,
    /// the batch is flushed and written out immediately.
    pub async fn write_frame(&mut self, frame: &[u8], priority: FramePriority) -> io::Result<()> {
        // Flush any pending write_buf
        self.flush_pending_write_buf().await?;

        if let Some(batch) = self.batcher.push(frame, priority) {
            encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            self.flush_pending_write_buf().await?;
        }
        Ok(())
    }

    /// Explicitly flushes any buffered frames through compression and writes them to `inner`.
    pub async fn flush_batch(&mut self) -> io::Result<()> {
        self.flush_pending_write_buf().await?;

        if !self.batcher.is_empty() {
            let batch = self.batcher.flush();
            encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
            self.flush_pending_write_buf().await?;
        }

        tokio::io::AsyncWriteExt::flush(&mut self.inner).await?;
        Ok(())
    }

    /// Checks if a time-slice flush is due and writes the flushed batch to `inner`.
    /// Returns `true` if a batch was flushed.
    pub async fn flush_if_due(&mut self) -> io::Result<bool> {
        if let Some(batch) = self.batcher.check_timer() {
            self.flush_pending_write_buf().await?;
            encode_batch_into(&mut self.compressor, &batch, &mut self.write_buf)?;
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

        // 2. Add incoming bytes to batcher with defer priority
        let flushed = this.batcher.push(buf, FramePriority::Defer);
        if let Some(batch) = flushed {
            if let Err(e) = encode_batch_into(this.compressor, &batch, this.write_buf) {
                return Poll::Ready(Err(e));
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
            let batch = this.batcher.flush();
            if let Err(e) = encode_batch_into(this.compressor, &batch, this.write_buf) {
                return Poll::Ready(Err(e));
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
        })
    }

    /// Creates a new `OptimizedReader` with default configuration.
    pub fn with_defaults(inner: R) -> io::Result<Self> {
        Self::new(inner, DecompressorConfig::default())
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
            if let Err(e) = this
                .decompressor
                .decompress_chunk_into(&this.payload_buf[..len], this.decompressed_buf)
            {
                return Poll::Ready(Err(e));
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
        let doc = ManagedTrafficOptimizerDocument {
            enabled: true,
            flush_interval_ms: Some(15),
            zstd_window_log: Some(22),
            zstd_level: Some(5),
        };
        let cfg = TrafficOptimizerConfig::from(&doc);
        assert!(cfg.enabled);
        assert_eq!(cfg.flush_interval, Duration::from_millis(15));
        assert_eq!(cfg.zstd_window_log, 22);
        assert_eq!(cfg.zstd_level, 5);

        let client_doc = ManagedTrafficOptimizerClientDocument {
            enabled: true,
            zstd_window_log: Some(21),
        };
        let client_cfg = TrafficOptimizerConfig::from(&client_doc);
        assert!(client_cfg.enabled);
        assert_eq!(client_cfg.zstd_window_log, 21);
    }
}
