//! Traffic observability for the Prism native optimizer.
//!
//! # Accounting model
//!
//! The optimizer reports three separate items per direction instead of one synthetic
//! "latency" number:
//!
//! 1. **Transfer gain** (`transfer_gain_ms`) — bytes kept off the wire (`saved_bytes`)
//!    converted with the *measured* link rate of this tunnel link, not a hardcoded
//!    bandwidth constant.
//! 2. **Batching penalty** (`batching_penalty_ms`) — time-slice aggregation queuing the
//!    flow paid before its bytes were compressed.
//! 3. **Compression penalty** (`compression_penalty_ms`) — host-local CPU spent
//!    compressing (writer side) or decompressing (reader side) this direction.
//!
//! `net_gain_ms` is `transfer_gain - batching_penalty - compression_penalty`.
//!
//! # Parallelism (no double counting)
//!
//! Uplink and downlink run as independent tasks, so their penalties are never summed:
//! every derived value is reported per direction, and the aggregate `net_gain_ms` is
//! the *minimum* net gain across directions that carry traffic.
//!
//! The peer's CPU cost is excluded on purpose: it runs on the other host, in parallel
//! with this host's work, and charging it here would count the same data path twice.
//! On a single host a given direction only ever performs one of compress/decompress,
//! so `compression_penalty_ms` covers both cases without double counting.
//!
//! Byte counters (`raw_bytes`/`wire_bytes`/`saved_bytes`) stay additive across
//! directions, because bytes really are additive even when the pipes are parallel.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Link rate assumed until the estimator has observed enough drain time.
pub const DEFAULT_LINK_RATE_BPS: f64 = 20_000_000.0; // 20 Mbps
/// Sanity ceiling for a measured link rate.
pub const MAX_LINK_RATE_BPS: f64 = 100_000_000_000.0; // 100 Gbps
/// Drain time the estimator must observe before it reports a measured rate.
pub const LINK_RATE_MIN_BUSY_US: u64 = 5_000; // 5 ms
/// Sliding-window geometry: 60 buckets of 1s.
pub const WINDOW_BUCKET_MS: u64 = 1_000;
pub const WINDOW_BUCKETS: usize = 60;

/// Sub-buckets per octave in the log2 histograms.
const HIST_SHIFT: u32 = 2;
/// Buckets for values `0..=3` are exact, above that 4 buckets per octave.
const HIST_EXACT: usize = 1 << HIST_SHIFT;
/// Highest index is `HIST_EXACT + (63 - HIST_SHIFT) * HIST_EXACT + HIST_EXACT - 1`.
const HIST_BUCKETS: usize = 256;

/// Wall-clock milliseconds since the Unix epoch.
pub fn unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Milliseconds needed to move `bytes` at `link_rate_bps`.
pub fn bytes_to_ms(bytes: u64, link_rate_bps: f64) -> f64 {
    if !(link_rate_bps > 0.0) {
        return 0.0;
    }
    bytes as f64 * 8000.0 / link_rate_bps
}

fn ratio(saved: u64, raw: u64) -> f64 {
    if raw > 0 {
        saved as f64 / raw as f64
    } else {
        0.0
    }
}

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

// ============================================================================
// Distribution tracking
// ============================================================================

/// Percentiles (microseconds) of a per-sample distribution.
///
/// Resolution is one histogram bucket: exact below 4µs, within 25% above that.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct Quantiles {
    pub p50_us: u64,
    pub p90_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

/// Lock-free log2-bucketed histogram used for percentiles.
pub struct ValueHistogram {
    buckets: [AtomicU64; HIST_BUCKETS],
}

impl Default for ValueHistogram {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Debug for ValueHistogram {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ValueHistogram")
            .field("count", &self.count())
            .finish()
    }
}

/// Bucket index for `value` (see [`bucket_value`] for the inverse).
fn bucket_index(value: u64) -> usize {
    if value < HIST_EXACT as u64 {
        return value as usize;
    }
    let exp = 63 - value.leading_zeros();
    let sub = ((value >> (exp - HIST_SHIFT)) & (HIST_EXACT as u64 - 1)) as usize;
    HIST_EXACT + (exp as usize - HIST_SHIFT as usize) * HIST_EXACT + sub
}

/// Lower bound of the value range covered by a bucket.
fn bucket_value(index: usize) -> u64 {
    if index < HIST_EXACT {
        return index as u64;
    }
    let j = index - HIST_EXACT;
    let exp = (j / HIST_EXACT) as u32 + HIST_SHIFT;
    let sub = (j % HIST_EXACT) as u64;
    ((HIST_EXACT as u64) | sub) << (exp - HIST_SHIFT)
}

impl ValueHistogram {
    pub fn new() -> Self {
        Self {
            buckets: std::array::from_fn(|_| AtomicU64::new(0)),
        }
    }

    /// Records one sample.
    pub fn record(&self, value: u64) {
        self.buckets[bucket_index(value)].fetch_add(1, Ordering::Relaxed);
    }

    /// Total number of recorded samples.
    pub fn count(&self) -> u64 {
        self.buckets
            .iter()
            .map(|b| b.load(Ordering::Relaxed))
            .sum()
    }

    /// Percentiles over all recorded samples.
    pub fn quantiles(&self) -> Quantiles {
        let total = self.count();
        if total == 0 {
            return Quantiles::default();
        }
        let targets = [0.5_f64, 0.9, 0.99];
        let mut out = Quantiles::default();
        let mut next = 0usize;
        let mut acc = 0u64;
        let mut last = 0usize;
        for (index, bucket) in self.buckets.iter().enumerate() {
            let count = bucket.load(Ordering::Relaxed);
            if count == 0 {
                continue;
            }
            acc += count;
            let value = bucket_value(index);
            while next < targets.len() && acc as f64 >= targets[next] * total as f64 {
                match next {
                    0 => out.p50_us = value,
                    1 => out.p90_us = value,
                    _ => out.p99_us = value,
                }
                next += 1;
            }
            last = index;
        }
        out.max_us = bucket_value(last);
        out
    }
}

// ============================================================================
// Sliding time window
// ============================================================================

/// One time-slice bucket of a [`SlidingWindow`].
#[derive(Debug, Default)]
struct WindowBucket {
    epoch: AtomicU64,
    raw_bytes: AtomicU64,
    wire_bytes: AtomicU64,
    batches: AtomicU64,
    batching_delay_us: AtomicU64,
    compression_time_us: AtomicU64,
    decompression_time_us: AtomicU64,
}

impl WindowBucket {
    fn reset(&self) {
        self.raw_bytes.store(0, Ordering::Relaxed);
        self.wire_bytes.store(0, Ordering::Relaxed);
        self.batches.store(0, Ordering::Relaxed);
        self.batching_delay_us.store(0, Ordering::Relaxed);
        self.compression_time_us.store(0, Ordering::Relaxed);
        self.decompression_time_us.store(0, Ordering::Relaxed);
    }
}

/// Counters a single sample contributes to a window bucket.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowSample {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub batches: u64,
    pub batching_delay_us: u64,
    pub compression_time_us: u64,
    pub decompression_time_us: u64,
}

/// Raw counters accumulated by a window.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WindowTotals {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub batches: u64,
    pub batching_delay_us: u64,
    pub compression_time_us: u64,
    pub decompression_time_us: u64,
}

impl WindowTotals {
    /// Adds another window's counters (used for the cross-direction aggregate window).
    pub fn merged(self, other: Self) -> Self {
        Self {
            raw_bytes: self.raw_bytes + other.raw_bytes,
            wire_bytes: self.wire_bytes + other.wire_bytes,
            batches: self.batches + other.batches,
            batching_delay_us: self.batching_delay_us + other.batching_delay_us,
            compression_time_us: self.compression_time_us + other.compression_time_us,
            decompression_time_us: self.decompression_time_us + other.decompression_time_us,
        }
    }

    /// Byte-only view of this window (safe to aggregate across parallel directions).
    pub fn snapshot(&self, window_ms: u64, link_rate_bps: f64) -> WindowSnapshot {
        let saved = self.raw_bytes.saturating_sub(self.wire_bytes);
        WindowSnapshot {
            window_ms,
            raw_bytes: self.raw_bytes,
            wire_bytes: self.wire_bytes,
            saved_bytes: saved,
            saved_ratio: ratio(saved, self.raw_bytes),
            batches: self.batches,
            transfer_gain_ms: bytes_to_ms(saved, link_rate_bps),
        }
    }
}

/// Traffic observed inside the sliding window (bytes are additive across directions).
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct WindowSnapshot {
    pub window_ms: u64,
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub batches: u64,
    pub transfer_gain_ms: f64,
}

/// Per-direction window snapshot, including that direction's own penalties.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DirectionWindowSnapshot {
    pub window_ms: u64,
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub batches: u64,
    pub transfer_gain_ms: f64,
    pub batching_penalty_ms: f64,
    pub compression_penalty_ms: f64,
    pub net_gain_ms: f64,
}

/// Lock-free sliding window over epoch-indexed buckets.
///
/// Recording claims the slot for the current epoch and zeroes it when the epoch rolls
/// over, so stale buckets age out without a background task. A recorder that races the
/// rotation may land its sample in the new epoch, which can shift the window seam by at
/// most one sample.
pub struct SlidingWindow {
    buckets: Vec<WindowBucket>,
    bucket_ms: u64,
}

impl std::fmt::Debug for SlidingWindow {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SlidingWindow")
            .field("bucket_ms", &self.bucket_ms)
            .field("buckets", &self.buckets.len())
            .finish()
    }
}

impl Default for SlidingWindow {
    fn default() -> Self {
        Self::new(WINDOW_BUCKET_MS, WINDOW_BUCKETS)
    }
}

impl SlidingWindow {
    pub fn new(bucket_ms: u64, bucket_count: usize) -> Self {
        let bucket_ms = bucket_ms.max(1);
        let bucket_count = bucket_count.max(1);
        Self {
            buckets: (0..bucket_count).map(|_| WindowBucket::default()).collect(),
            bucket_ms,
        }
    }

    /// Window span in milliseconds.
    pub fn window_ms(&self) -> u64 {
        self.bucket_ms * self.buckets.len() as u64
    }

    fn claim(&self, now_ms: u64) -> &WindowBucket {
        let epoch = now_ms / self.bucket_ms;
        let bucket = &self.buckets[(epoch % self.buckets.len() as u64) as usize];
        let mut current = bucket.epoch.load(Ordering::Relaxed);
        loop {
            if current == epoch {
                return bucket;
            }
            match bucket.epoch.compare_exchange_weak(
                current,
                epoch,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    bucket.reset();
                    return bucket;
                }
                Err(observed) => current = observed,
            }
        }
    }

    /// Records a sample into the bucket covering `now_ms`.
    pub fn record(&self, now_ms: u64, sample: WindowSample) {
        let bucket = self.claim(now_ms);
        bucket
            .raw_bytes
            .fetch_add(sample.raw_bytes, Ordering::Relaxed);
        bucket
            .wire_bytes
            .fetch_add(sample.wire_bytes, Ordering::Relaxed);
        bucket.batches.fetch_add(sample.batches, Ordering::Relaxed);
        bucket
            .batching_delay_us
            .fetch_add(sample.batching_delay_us, Ordering::Relaxed);
        bucket
            .compression_time_us
            .fetch_add(sample.compression_time_us, Ordering::Relaxed);
        bucket
            .decompression_time_us
            .fetch_add(sample.decompression_time_us, Ordering::Relaxed);
    }

    /// Total counters still inside the window at `now_ms`.
    pub fn totals(&self, now_ms: u64) -> WindowTotals {
        let current = now_ms / self.bucket_ms;
        let span = self.buckets.len() as u64;
        let mut totals = WindowTotals::default();
        for bucket in &self.buckets {
            let epoch = bucket.epoch.load(Ordering::Relaxed);
            if epoch > current || current - epoch >= span {
                continue;
            }
            totals.raw_bytes += bucket.raw_bytes.load(Ordering::Relaxed);
            totals.wire_bytes += bucket.wire_bytes.load(Ordering::Relaxed);
            totals.batches += bucket.batches.load(Ordering::Relaxed);
            totals.batching_delay_us += bucket.batching_delay_us.load(Ordering::Relaxed);
            totals.compression_time_us += bucket.compression_time_us.load(Ordering::Relaxed);
            totals.decompression_time_us += bucket.decompression_time_us.load(Ordering::Relaxed);
        }
        totals
    }
}

// ============================================================================
// Link rate estimation
// ============================================================================

#[derive(Debug, Default)]
struct LinkBucket {
    epoch: AtomicU64,
    bytes: AtomicU64,
    busy_us: AtomicU64,
}

impl LinkBucket {
    fn reset(&self) {
        self.bytes.store(0, Ordering::Relaxed);
        self.busy_us.store(0, Ordering::Relaxed);
    }
}

/// Measured transfer rate of the tunnel link.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq)]
pub struct LinkRateSample {
    pub rate_bps: f64,
    /// `true` when the rate comes from observed drain time rather than the fallback.
    pub measured: bool,
    pub bytes: u64,
    pub busy_us: u64,
}

impl Default for LinkRateSample {
    fn default() -> Self {
        Self {
            rate_bps: DEFAULT_LINK_RATE_BPS,
            measured: false,
            bytes: 0,
            busy_us: 0,
        }
    }
}

/// Duration-weighted throughput of the tunnel link over a sliding window.
///
/// Samples are `(bytes drained, time spent draining)`. Weighting by drain time makes
/// writes that returned immediately (buffered locally, link not saturated) contribute
/// almost nothing, while writes that actually back-pressured dominate — so the rate
/// converges on the real bottleneck rate of the connection instead of on how fast this
/// host can hand bytes to the kernel.
///
/// Until enough drain time has accumulated the estimator reports
/// [`DEFAULT_LINK_RATE_BPS`] with `measured = false`. That fallback is a fixed
/// assumption, so on a link faster than 20 Mbps it *overstates* the transfer gain while
/// it is in use; callers must surface `measured` rather than presenting the number as a
/// measurement.
pub struct LinkRateEstimator {
    buckets: Vec<LinkBucket>,
    bucket_ms: u64,
}

impl std::fmt::Debug for LinkRateEstimator {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LinkRateEstimator")
            .field("bucket_ms", &self.bucket_ms)
            .field("buckets", &self.buckets.len())
            .finish()
    }
}

impl Default for LinkRateEstimator {
    fn default() -> Self {
        Self::new(WINDOW_BUCKET_MS, WINDOW_BUCKETS)
    }
}

impl LinkRateEstimator {
    pub fn new(bucket_ms: u64, bucket_count: usize) -> Self {
        let bucket_ms = bucket_ms.max(1);
        let bucket_count = bucket_count.max(1);
        Self {
            buckets: (0..bucket_count).map(|_| LinkBucket::default()).collect(),
            bucket_ms,
        }
    }

    fn claim(&self, now_ms: u64) -> &LinkBucket {
        let epoch = now_ms / self.bucket_ms;
        let bucket = &self.buckets[(epoch % self.buckets.len() as u64) as usize];
        let mut current = bucket.epoch.load(Ordering::Relaxed);
        loop {
            if current == epoch {
                return bucket;
            }
            match bucket.epoch.compare_exchange_weak(
                current,
                epoch,
                Ordering::Relaxed,
                Ordering::Relaxed,
            ) {
                Ok(_) => {
                    bucket.reset();
                    return bucket;
                }
                Err(observed) => current = observed,
            }
        }
    }

    /// Records bytes written and the time spent writing them.
    pub fn record(&self, now_ms: u64, bytes: u64, busy_us: u64) {
        if bytes == 0 {
            return;
        }
        let bucket = self.claim(now_ms);
        bucket.bytes.fetch_add(bytes, Ordering::Relaxed);
        bucket.busy_us.fetch_add(busy_us, Ordering::Relaxed);
    }

    /// Effective rate over the window, falling back to [`DEFAULT_LINK_RATE_BPS`].
    pub fn sample(&self, now_ms: u64) -> LinkRateSample {
        let current = now_ms / self.bucket_ms;
        let span = self.buckets.len() as u64;
        let mut bytes = 0u64;
        let mut busy_us = 0u64;
        for bucket in &self.buckets {
            let epoch = bucket.epoch.load(Ordering::Relaxed);
            if epoch > current || current - epoch >= span {
                continue;
            }
            bytes += bucket.bytes.load(Ordering::Relaxed);
            busy_us += bucket.busy_us.load(Ordering::Relaxed);
        }

        if busy_us < LINK_RATE_MIN_BUSY_US {
            return LinkRateSample {
                bytes,
                busy_us,
                ..LinkRateSample::default()
            };
        }

        let rate = (bytes as f64 * 8_000_000.0 / busy_us as f64).min(MAX_LINK_RATE_BPS);
        LinkRateSample {
            rate_bps: rate,
            measured: true,
            bytes,
            busy_us,
        }
    }
}

// ============================================================================
// Snapshots
// ============================================================================

/// Statistics snapshot for a single traffic direction.
///
/// Every derived value is scoped to this direction only. Uplink and downlink run as
/// independent tasks, so a consumer MUST NOT sum `*_penalty_ms` across directions;
/// only byte counters are additive.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct DirectionStatsSnapshot {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    /// Flushed batches on this host's writer side, or received chunks on its reader
    /// side for this direction. Any non-zero value means the direction carries traffic.
    pub batches: u64,

    /// Lifetime queuing time this direction's bytes spent in the time-slice aggregator.
    pub batching_delay_us: u64,
    /// Lifetime CPU time spent compressing this direction (writer side of this host).
    pub compression_time_us: u64,
    /// Lifetime CPU time spent decompressing this direction (reader side of this host).
    pub decompression_time_us: u64,

    pub link_rate_bps: f64,
    pub transfer_gain_ms: f64,
    pub batching_penalty_ms: f64,
    pub compression_penalty_ms: f64,
    /// Net of the three items above. Negative means this direction's bytes are worse
    /// off than if they had been sent uncompressed over the same link.
    pub net_gain_ms: f64,

    /// Per-sample batching queue delays, microseconds (lifetime).
    pub batching_delay: Quantiles,
    /// Per-sample compression (writer side) or decompression (reader side) durations.
    /// A given host performs only one of the two per direction, so this distribution
    /// is not a mix of both.
    pub compression_time: Quantiles,

    pub window: DirectionWindowSnapshot,
}

/// Detailed snapshot of traffic, link accounting and distributions.
///
/// Byte counters are additive across directions; `net_gain_ms` is deliberately not.
/// See [`DirectionStatsSnapshot`] and the module docs for the parallelism rules.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct OptimizerStatsSnapshot {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub urgent_batches: u64,
    pub timer_batches: u64,
    pub threshold_batches: u64,
    pub explicit_batches: u64,

    pub link_rate_bps: f64,
    pub link_rate_measured: bool,
    pub link_rate_bytes: u64,
    pub link_rate_busy_us: u64,

    /// Bytes saved converted with the measured link rate (additive across directions).
    pub transfer_gain_ms: f64,
    /// Conservative floor: the smallest net gain across directions carrying traffic.
    /// Directions run in parallel, so this is deliberately not a sum.
    pub net_gain_ms: f64,

    /// Informational resource totals; never used for cross-direction arithmetic.
    pub batching_delay_us: u64,
    pub compression_time_us: u64,
    pub decompression_time_us: u64,

    pub uplink: DirectionStatsSnapshot,
    pub downlink: DirectionStatsSnapshot,

    pub window: WindowSnapshot,
}

// ============================================================================
// Counters
// ============================================================================

/// Lock-free per-direction traffic, cost and distribution counters.
#[derive(Debug, Default)]
pub struct DirectionStats {
    pub raw_bytes: AtomicU64,
    pub wire_bytes: AtomicU64,
    pub batches: AtomicU64,
    pub compression_time_us: AtomicU64,
    pub decompression_time_us: AtomicU64,
    pub batching_delay_us: AtomicU64,
    pub batching_hist: ValueHistogram,
    pub compression_hist: ValueHistogram,
    pub window: SlidingWindow,
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

    /// Window counters still inside the window at `now_ms`.
    pub fn window_totals(&self, now_ms: u64) -> WindowTotals {
        self.window.totals(now_ms)
    }

    /// Snapshot of this direction; `totals` is passed in so the caller computes the
    /// window exactly once for both directions.
    pub fn snapshot(
        &self,
        totals: WindowTotals,
        window_ms: u64,
        link_rate_bps: f64,
    ) -> DirectionStatsSnapshot {
        let raw = self.raw_bytes.load(Ordering::Relaxed);
        let wire = self.wire_bytes.load(Ordering::Relaxed);
        let saved = raw.saturating_sub(wire);
        let comp_us = self.compression_time_us.load(Ordering::Relaxed);
        let decomp_us = self.decompression_time_us.load(Ordering::Relaxed);
        let delay_us = self.batching_delay_us.load(Ordering::Relaxed);

        let transfer_gain_ms = bytes_to_ms(saved, link_rate_bps);
        let batching_penalty_ms = delay_us as f64 / 1000.0;
        let compression_penalty_ms = (comp_us + decomp_us) as f64 / 1000.0;

        let window_totals = totals.snapshot(window_ms, link_rate_bps);
        let window_batching_penalty_ms = totals.batching_delay_us as f64 / 1000.0;
        let window_compression_penalty_ms =
            (totals.compression_time_us + totals.decompression_time_us) as f64 / 1000.0;

        DirectionStatsSnapshot {
            raw_bytes: raw,
            wire_bytes: wire,
            saved_bytes: saved,
            saved_ratio: ratio(saved, raw),
            batches: self.batches.load(Ordering::Relaxed),
            batching_delay_us: delay_us,
            compression_time_us: comp_us,
            decompression_time_us: decomp_us,
            link_rate_bps,
            transfer_gain_ms,
            batching_penalty_ms,
            compression_penalty_ms,
            net_gain_ms: transfer_gain_ms - batching_penalty_ms - compression_penalty_ms,
            batching_delay: self.batching_hist.quantiles(),
            compression_time: self.compression_hist.quantiles(),
            window: DirectionWindowSnapshot {
                window_ms: window_totals.window_ms,
                raw_bytes: window_totals.raw_bytes,
                wire_bytes: window_totals.wire_bytes,
                saved_bytes: window_totals.saved_bytes,
                saved_ratio: window_totals.saved_ratio,
                batches: window_totals.batches,
                transfer_gain_ms: window_totals.transfer_gain_ms,
                batching_penalty_ms: window_batching_penalty_ms,
                compression_penalty_ms: window_compression_penalty_ms,
                net_gain_ms: window_totals.transfer_gain_ms
                    - window_batching_penalty_ms
                    - window_compression_penalty_ms,
            },
        }
    }
}

/// Lock-free traffic, cost and link-rate counters shared by writers and readers.
#[derive(Debug, Default)]
pub struct OptimizerStats {
    pub raw_bytes: AtomicU64,
    pub wire_bytes: AtomicU64,
    pub urgent_batches: AtomicU64,
    pub timer_batches: AtomicU64,
    pub threshold_batches: AtomicU64,
    pub explicit_batches: AtomicU64,

    pub uplink: DirectionStats,
    pub downlink: DirectionStats,

    pub compression_time_us: AtomicU64,
    pub decompression_time_us: AtomicU64,
    pub batching_delay_us: AtomicU64,

    pub link: LinkRateEstimator,
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

    /// Records raw (pre-compression) bytes for `dir`.
    ///
    /// This is the single raw accounting point for a direction: either the reader
    /// records decompressed output, or the egress transform records what it wrote.
    pub fn add_direction_raw_bytes(&self, dir: TrafficDirection, bytes: u64, now_ms: u64) {
        self.add_raw_bytes(bytes);
        let sample = WindowSample {
            raw_bytes: bytes,
            ..WindowSample::default()
        };
        match dir {
            TrafficDirection::Uplink => {
                self.uplink.add_raw_bytes(bytes);
                self.uplink.window.record(now_ms, sample);
            }
            TrafficDirection::Downlink => {
                self.downlink.add_raw_bytes(bytes);
                self.downlink.window.record(now_ms, sample);
            }
        }
    }

    /// Records one flushed and compressed batch on the writer side of `dir`.
    pub fn record_batch(
        &self,
        dir: TrafficDirection,
        framed_bytes: u64,
        batching_delay_us: u64,
        compression_us: u64,
        now_ms: u64,
    ) {
        self.batching_delay_us
            .fetch_add(batching_delay_us, Ordering::Relaxed);
        self.compression_time_us
            .fetch_add(compression_us, Ordering::Relaxed);

        let direction = match dir {
            TrafficDirection::Uplink => &self.uplink,
            TrafficDirection::Downlink => &self.downlink,
        };
        direction.batches.fetch_add(1, Ordering::Relaxed);
        direction.wire_bytes.fetch_add(framed_bytes, Ordering::Relaxed);
        direction
            .compression_time_us
            .fetch_add(compression_us, Ordering::Relaxed);
        direction
            .batching_delay_us
            .fetch_add(batching_delay_us, Ordering::Relaxed);
        direction.batching_hist.record(batching_delay_us);
        direction.compression_hist.record(compression_us);
        self.add_wire_bytes(framed_bytes);
        direction.window.record(
            now_ms,
            WindowSample {
                wire_bytes: framed_bytes,
                batches: 1,
                batching_delay_us,
                compression_time_us: compression_us,
                ..WindowSample::default()
            },
        );
    }

    /// Records one received and decompressed chunk on the reader side of `dir`.
    pub fn record_chunk(&self, dir: TrafficDirection, framed_bytes: u64, decompression_us: u64, now_ms: u64) {
        self.decompression_time_us
            .fetch_add(decompression_us, Ordering::Relaxed);

        let direction = match dir {
            TrafficDirection::Uplink => &self.uplink,
            TrafficDirection::Downlink => &self.downlink,
        };
        direction.batches.fetch_add(1, Ordering::Relaxed);
        direction.wire_bytes.fetch_add(framed_bytes, Ordering::Relaxed);
        direction
            .decompression_time_us
            .fetch_add(decompression_us, Ordering::Relaxed);
        direction.compression_hist.record(decompression_us);
        self.add_wire_bytes(framed_bytes);
        direction.window.record(
            now_ms,
            WindowSample {
                wire_bytes: framed_bytes,
                batches: 1,
                decompression_time_us: decompression_us,
                ..WindowSample::default()
            },
        );
    }

    /// Records one observed drain of the tunnel link.
    pub fn record_link_sample(&self, bytes: u64, busy_us: u64) {
        self.record_link_sample_at(unix_ms(), bytes, busy_us);
    }

    /// Records one observed drain at an explicit timestamp.
    pub fn record_link_sample_at(&self, now_ms: u64, bytes: u64, busy_us: u64) {
        self.link.record(now_ms, bytes, busy_us);
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

    pub fn inc_explicit(&self) {
        self.explicit_batches.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> OptimizerStatsSnapshot {
        self.snapshot_at(unix_ms())
    }

    pub fn snapshot_at(&self, now_ms: u64) -> OptimizerStatsSnapshot {
        let link = self.link.sample(now_ms);
        let window_ms = self.uplink.window.window_ms();
        let up_totals = self.uplink.window_totals(now_ms);
        let down_totals = self.downlink.window_totals(now_ms);
        let uplink = self.uplink.snapshot(up_totals, window_ms, link.rate_bps);
        let downlink = self.downlink.snapshot(down_totals, window_ms, link.rate_bps);

        let raw = self.raw_bytes.load(Ordering::Relaxed);
        let wire = self.wire_bytes.load(Ordering::Relaxed);
        let saved = raw.saturating_sub(wire);

        let net_gain_ms = [uplink.net_gain_ms, downlink.net_gain_ms]
            .into_iter()
            .zip([uplink.batches, downlink.batches])
            .filter(|(_, batches)| *batches > 0)
            .map(|(net, _)| net)
            .fold(f64::INFINITY, f64::min);
        let net_gain_ms = if net_gain_ms.is_finite() { net_gain_ms } else { 0.0 };

        OptimizerStatsSnapshot {
            raw_bytes: raw,
            wire_bytes: wire,
            saved_bytes: saved,
            saved_ratio: ratio(saved, raw),
            urgent_batches: self.urgent_batches.load(Ordering::Relaxed),
            timer_batches: self.timer_batches.load(Ordering::Relaxed),
            threshold_batches: self.threshold_batches.load(Ordering::Relaxed),
            explicit_batches: self.explicit_batches.load(Ordering::Relaxed),
            link_rate_bps: link.rate_bps,
            link_rate_measured: link.measured,
            link_rate_bytes: link.bytes,
            link_rate_busy_us: link.busy_us,
            transfer_gain_ms: bytes_to_ms(saved, link.rate_bps),
            net_gain_ms,
            batching_delay_us: self.batching_delay_us.load(Ordering::Relaxed),
            compression_time_us: self.compression_time_us.load(Ordering::Relaxed),
            decompression_time_us: self.decompression_time_us.load(Ordering::Relaxed),
            uplink,
            downlink,
            window: up_totals
                .merged(down_totals)
                .snapshot(window_ms, link.rate_bps),
        }
    }
}

pub type SharedOptimizerStats = Arc<OptimizerStats>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn histogram_buckets_round_trip() {
        for value in [0u64, 1, 2, 3, 4, 5, 7, 8, 15, 16, 1000, 1_000_000, 1 << 40] {
            let index = bucket_index(value);
            assert!(index < HIST_BUCKETS, "index {index} out of range for {value}");
            let lower = bucket_value(index);
            assert!(lower <= value, "bucket {index} lower bound {lower} > {value}");
            assert_eq!(bucket_index(lower), index, "bucket value must map back");
            // Every value in the bucket is below the next bucket's lower bound.
            assert!(
                bucket_value(index + 1) > value,
                "bucket {index} covers {lower} but next lower bound is not above {value}"
            );
        }
    }

    #[test]
    fn histogram_quantiles_track_distribution() {
        let hist = ValueHistogram::new();
        for value in 1..=100u64 {
            hist.record(value);
        }
        let q = hist.quantiles();
        assert_eq!(hist.count(), 100);
        assert!(q.p50_us <= 50 && q.p50_us >= 25, "p50 {}", q.p50_us);
        assert!(q.p90_us <= 90 && q.p90_us >= 60, "p90 {}", q.p90_us);
        assert!(q.p99_us <= 100 && q.p99_us >= 90, "p99 {}", q.p99_us);
        assert!(q.max_us <= 100 && q.max_us >= 90, "max {}", q.max_us);
    }

    #[test]
    fn histogram_empty_is_zeroed() {
        assert_eq!(ValueHistogram::new().quantiles(), Quantiles::default());
    }

    #[test]
    fn window_ages_out_samples() {
        let window = SlidingWindow::new(1_000, 10); // 10s
        window.record(
            1_000,
            WindowSample {
                raw_bytes: 500,
                batches: 1,
                ..WindowSample::default()
            },
        );
        assert_eq!(window.totals(1_500).raw_bytes, 500);
        // Still inside the 10s window at t+9s, gone at t+10s.
        assert_eq!(window.totals(10_000).raw_bytes, 500);
        assert_eq!(window.totals(11_500).raw_bytes, 0);
    }

    #[test]
    fn window_buckets_are_reused_by_epoch() {
        let window = SlidingWindow::new(1_000, 4);
        window.record(
            1_000,
            WindowSample {
                wire_bytes: 10,
                ..WindowSample::default()
            },
        );
        // Same slot, 4 epochs later: the old sample must not leak into the new window.
        window.record(
            5_000,
            WindowSample {
                wire_bytes: 7,
                ..WindowSample::default()
            },
        );
        assert_eq!(window.totals(5_000).wire_bytes, 7);
    }

    #[test]
    fn link_rate_uses_drain_weighted_samples() {
        let link = LinkRateEstimator::default();
        // 1 MB moved in 100 ms of actual drain time → ~80 Mbps.
        link.record(1_000, 1_000_000, 100_000);

        let sample = link.sample(1_000);
        assert!(sample.measured);
        assert!(
            (sample.rate_bps - 80_000_000.0).abs() < 1.0,
            "rate {}",
            sample.rate_bps
        );
    }

    #[test]
    fn link_rate_falls_back_without_drain_time() {
        let link = LinkRateEstimator::default();
        // Buffered writes: bytes moved but essentially no drain time observed.
        link.record(1_000, 256 * 1024, 7);

        let sample = link.sample(1_000);
        assert!(!sample.measured);
        assert_eq!(sample.rate_bps, DEFAULT_LINK_RATE_BPS);
    }

    #[test]
    fn link_rate_ignores_expired_samples() {
        let link = LinkRateEstimator::new(1_000, 10);
        link.record(1_000, 1_000_000, 100_000);
        assert!(link.sample(1_500).measured);
        assert!(!link.sample(20_000).measured);
    }

    #[test]
    fn penalties_are_per_direction_and_net_never_sums_them() {
        let stats = OptimizerStats::new();
        let now = 1_000;

        // Uplink: heavily batched and expensive, downlink: cheap.
        stats.record_batch(TrafficDirection::Uplink, 1_000, 20_000, 10_000, now);
        stats.record_batch(TrafficDirection::Downlink, 1_000, 1_000, 500, now);
        stats.add_direction_raw_bytes(TrafficDirection::Uplink, 8_000, now);
        stats.add_direction_raw_bytes(TrafficDirection::Downlink, 2_000, now);
        stats.record_link_sample_at(now, 20_000, 10_000); // 16 Mbps measured

        let snap = stats.snapshot_at(now);
        let up = &snap.uplink;
        let down = &snap.downlink;

        assert!(snap.link_rate_measured);
        assert!((snap.link_rate_bps - 16_000_000.0).abs() < 1.0);
        assert_eq!(up.raw_bytes, 8_000);
        assert_eq!(up.wire_bytes, 1_000);
        assert_eq!(up.batching_penalty_ms, 20.0);
        assert_eq!(up.compression_penalty_ms, 10.0);
        assert_eq!(down.batching_penalty_ms, 1.0);
        assert_eq!(down.compression_penalty_ms, 0.5);

        assert!((up.net_gain_ms - (up.transfer_gain_ms - 30.0)).abs() < 1e-9);
        assert!((down.net_gain_ms - (down.transfer_gain_ms - 1.5)).abs() < 1e-9);

        // Per-sample distributions are populated from the batch paths.
        assert!(
            (16_000..=20_000).contains(&up.batching_delay.p99_us),
            "p99 {}",
            up.batching_delay.p99_us
        );
        assert!(up.compression_time.p50_us >= 8_000 && up.compression_time.p50_us <= 10_000);
        assert_eq!(down.batching_delay.p50_us, down.batching_delay.max_us);
        assert!(
            (750..=1_000).contains(&down.batching_delay.max_us),
            "max {}",
            down.batching_delay.max_us
        );

        // The aggregate must not add the two parallel pipelines together.
        let summed = up.net_gain_ms + down.net_gain_ms;
        assert_eq!(snap.net_gain_ms, up.net_gain_ms.min(down.net_gain_ms));
        assert_ne!(snap.net_gain_ms, summed, "aggregate net must not be a sum");

        // Bytes remain additive.
        assert_eq!(snap.raw_bytes, 10_000);
        assert_eq!(snap.wire_bytes, 2_000);
        assert_eq!(snap.transfer_gain_ms, bytes_to_ms(8_000, snap.link_rate_bps));
    }

    #[test]
    fn aggregate_net_ignores_idle_directions() {
        let stats = OptimizerStats::new();
        let now = 1_000;
        stats.record_batch(TrafficDirection::Uplink, 1_000, 0, 0, now);
        stats.add_direction_raw_bytes(TrafficDirection::Uplink, 4_000, now);
        let snap = stats.snapshot_at(now);
        assert_eq!(snap.downlink.batches, 0);
        assert_eq!(snap.net_gain_ms, snap.uplink.net_gain_ms);
    }

    #[test]
    fn window_totals_split_by_direction_and_merge_for_aggregate() {
        let stats = OptimizerStats::new();
        let now = 1_000;
        stats.add_direction_raw_bytes(TrafficDirection::Uplink, 1_000, now);
        stats.record_batch(TrafficDirection::Uplink, 200, 5_000, 1_000, now);
        stats.add_direction_raw_bytes(TrafficDirection::Downlink, 4_000, now);
        stats.record_chunk(TrafficDirection::Downlink, 400, 2_000, now);

        let snap = stats.snapshot_at(now + 500);
        assert_eq!(snap.uplink.window.raw_bytes, 1_000);
        assert_eq!(snap.uplink.window.wire_bytes, 200);
        assert_eq!(snap.uplink.window.batching_penalty_ms, 5.0);
        assert_eq!(snap.downlink.window.raw_bytes, 4_000);
        assert_eq!(snap.downlink.window.compression_penalty_ms, 2.0);
        assert_eq!(snap.window.raw_bytes, 5_000);
        assert_eq!(snap.window.wire_bytes, 600);
        assert_eq!(snap.window.batches, 2);
    }
}
