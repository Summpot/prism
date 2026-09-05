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

use crate::prism::tunnel::traffic_optimizer::{
    SharedTrafficStats, TrafficStats, TrafficStatsSnapshot,
};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
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
        }
    }
}

#[derive(Debug)]
pub struct SessionRegistry {
    sessions: DashMap<String, (SessionInfo, Option<SharedTrafficStats>)>,
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
    pub fn add_with_stats(&self, s: SessionInfo, stats: SharedTrafficStats) {
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
            }
            out.push(info);
        }
        out.sort_by(|a, b| a.started_at_unix_ms.cmp(&b.started_at_unix_ms));
        out
    }
}

/// Registry of global and per-service traffic optimization statistics.
#[derive(Debug, Default)]
pub struct TrafficStatsRegistry {
    global: SharedTrafficStats,
    services: DashMap<String, SharedTrafficStats>,
}

impl TrafficStatsRegistry {
    pub fn new() -> Self {
        Self {
            global: Arc::new(TrafficStats::new()),
            services: DashMap::new(),
        }
    }

    pub fn global(&self) -> SharedTrafficStats {
        self.global.clone()
    }

    pub fn service(&self, name: &str) -> SharedTrafficStats {
        let key = name.trim().to_ascii_lowercase();
        self.services
            .entry(key)
            .or_insert_with(|| Arc::new(TrafficStats::new()))
            .clone()
    }

    pub fn snapshot(&self) -> (TrafficStatsSnapshot, HashMap<String, TrafficStatsSnapshot>) {
        let global_snap = self.global.snapshot();
        let mut services_snap = HashMap::with_capacity(self.services.len());
        for entry in self.services.iter() {
            services_snap.insert(entry.key().clone(), entry.value().snapshot());
        }
        (global_snap, services_snap)
    }
}

pub type SharedTrafficRegistry = Arc<TrafficStatsRegistry>;

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
