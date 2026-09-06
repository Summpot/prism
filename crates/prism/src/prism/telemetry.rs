use std::{
    collections::HashMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use serde::{Deserialize, Serialize};

use crate::prism::tunnel::optimizer::{
    OptimizerStats, OptimizerStatsSnapshot, SharedOptimizerStats,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionInfo {
    pub id: String,
    pub client: String,
    pub host: String,
    pub upstream: String,
    pub started_at_unix_ms: u64,
    #[serde(default)]
    pub raw_bytes: u64,
    #[serde(default)]
    pub wire_bytes: u64,
    #[serde(default)]
    pub uplink_raw_bytes: u64,
    #[serde(default)]
    pub uplink_wire_bytes: u64,
    #[serde(default)]
    pub downlink_raw_bytes: u64,
    #[serde(default)]
    pub downlink_wire_bytes: u64,
    #[serde(default)]
    pub est_latency_improvement_ms: f64,
    #[serde(default)]
    pub est_latency_degradation_ms: f64,
}

impl SessionInfo {
    #[allow(dead_code)]
    pub fn new(id: String, client: String, host: String, upstream: String) -> Self {
        Self {
            id,
            client,
            host,
            upstream,
            started_at_unix_ms: now_unix_ms(),
            raw_bytes: 0,
            wire_bytes: 0,
            uplink_raw_bytes: 0,
            uplink_wire_bytes: 0,
            downlink_raw_bytes: 0,
            downlink_wire_bytes: 0,
            est_latency_improvement_ms: 0.0,
            est_latency_degradation_ms: 0.0,
        }
    }
}

#[derive(Debug)]
pub struct SessionRegistry {
    sessions: DashMap<String, (SessionInfo, Option<SharedOptimizerStats>)>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub fn add(&self, s: SessionInfo) {
        self.sessions.insert(s.id.clone(), (s, None));
    }

    #[allow(dead_code)]
    pub fn add_with_stats(&self, s: SessionInfo, stats: SharedOptimizerStats) {
        self.sessions.insert(s.id.clone(), (s, Some(stats)));
    }

    pub fn remove(&self, id: &str) {
        self.sessions.remove(id);
    }

    pub fn snapshot(&self) -> Vec<SessionInfo> {
        let mut out = Vec::with_capacity(self.sessions.len());
        for s in self.sessions.iter() {
            let (mut info, stats) = s.value().clone();
            if let Some(stats) = stats {
                let snap = stats.snapshot();
                info.raw_bytes = snap.raw_bytes;
                info.wire_bytes = snap.wire_bytes;
                info.uplink_raw_bytes = snap.uplink.raw_bytes;
                info.uplink_wire_bytes = snap.uplink.wire_bytes;
                info.downlink_raw_bytes = snap.downlink.raw_bytes;
                info.downlink_wire_bytes = snap.downlink.wire_bytes;
                info.est_latency_improvement_ms = snap.est_transfer_time_saved_ms;
                info.est_latency_degradation_ms = snap.est_processing_time_ms;
            }
            out.push(info);
        }
        out.sort_by(|a, b| a.started_at_unix_ms.cmp(&b.started_at_unix_ms));
        out
    }
}

/// Registry of global and per-service optimization statistics.
#[derive(Debug, Default)]
pub struct OptimizerStatsRegistry {
    global: SharedOptimizerStats,
    services: DashMap<String, SharedOptimizerStats>,
}

impl OptimizerStatsRegistry {
    pub fn new() -> Self {
        Self {
            global: Arc::new(OptimizerStats::new()),
            services: DashMap::new(),
        }
    }

    pub fn global(&self) -> SharedOptimizerStats {
        self.global.clone()
    }

    pub fn service(&self, name: &str) -> SharedOptimizerStats {
        let key = name.trim().to_ascii_lowercase();
        self.services
            .entry(key)
            .or_insert_with(|| Arc::new(OptimizerStats::new()))
            .clone()
    }

    pub fn snapshot(
        &self,
    ) -> (
        OptimizerStatsSnapshot,
        HashMap<String, OptimizerStatsSnapshot>,
    ) {
        let global_snap = self.global.snapshot();
        let mut services_snap = HashMap::with_capacity(self.services.len());
        for entry in self.services.iter() {
            services_snap.insert(entry.key().clone(), entry.value().snapshot());
        }
        (global_snap, services_snap)
    }
}

pub type SharedOptimizerRegistry = Arc<OptimizerStatsRegistry>;

pub fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

pub fn new_session_id() -> String {
    static SEQ: AtomicU64 = AtomicU64::new(1);
    let n = SEQ.fetch_add(1, Ordering::Relaxed);
    format!("s{n}")
}

#[derive(Debug, Clone)]
pub struct ReloadSignal {
    // Monotonic counter; increment indicates a reload request.
    pub seq: u64,
}

impl ReloadSignal {
    pub fn new() -> Self {
        Self { seq: 0 }
    }

    pub fn next(&mut self) {
        self.seq = self.seq.wrapping_add(1);
    }
}

pub type SharedSessions = Arc<SessionRegistry>;
