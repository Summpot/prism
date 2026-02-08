use std::{
    collections::HashMap,
    sync::{
        atomic::{AtomicI64, AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use serde::Serialize;

#[derive(Debug)]
pub struct MetricsCollector {
    active: AtomicI64,
    total: AtomicU64,
    bytes_ingress: AtomicU64,
    bytes_egress: AtomicU64,
    route_hits: DashMap<String, AtomicU64>,
}

impl MetricsCollector {
    pub fn new() -> Self {
        Self {
            active: AtomicI64::new(0),
            total: AtomicU64::new(0),
            bytes_ingress: AtomicU64::new(0),
            bytes_egress: AtomicU64::new(0),
            route_hits: DashMap::new(),
        }
    }

    pub fn inc_active(&self) {
        self.active.fetch_add(1, Ordering::Relaxed);
        self.total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn dec_active(&self) {
        self.active.fetch_sub(1, Ordering::Relaxed);
    }

    pub fn add_bytes(&self, ingress: u64, egress: u64) {
        self.bytes_ingress.fetch_add(ingress, Ordering::Relaxed);
        self.bytes_egress.fetch_add(egress, Ordering::Relaxed);
    }

    pub fn add_route_hit(&self, host: &str) {
        let h = host.trim().to_ascii_lowercase();
        if h.is_empty() {
            return;
        }
        let entry = self
            .route_hits
            .entry(h)
            .or_insert_with(|| AtomicU64::new(0));
        entry.fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let mut hits = HashMap::new();
        for r in self.route_hits.iter() {
            hits.insert(r.key().clone(), r.value().load(Ordering::Relaxed));
        }
        MetricsSnapshot {
            active_connections: self.active.load(Ordering::Relaxed),
            total_connections_handled: self.total.load(Ordering::Relaxed),
            bytes_ingress: self.bytes_ingress.load(Ordering::Relaxed),
            bytes_egress: self.bytes_egress.load(Ordering::Relaxed),
            route_hits: hits,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct MetricsSnapshot {
    pub active_connections: i64,
    pub total_connections_handled: u64,
    pub bytes_ingress: u64,
    pub bytes_egress: u64,
    pub route_hits: HashMap<String, u64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub client: String,
    pub host: String,
    pub upstream: String,
    pub started_at_unix_ms: u64,
}

#[derive(Debug)]
pub struct SessionRegistry {
    sessions: DashMap<String, SessionInfo>,
}

impl SessionRegistry {
    pub fn new() -> Self {
        Self {
            sessions: DashMap::new(),
        }
    }

    pub fn add(&self, s: SessionInfo) {
        self.sessions.insert(s.id.clone(), s);
    }

    pub fn remove(&self, id: &str) {
        self.sessions.remove(id);
    }

    pub fn snapshot(&self) -> Vec<SessionInfo> {
        let mut out = Vec::with_capacity(self.sessions.len());
        for s in self.sessions.iter() {
            out.push(s.value().clone());
        }
        out.sort_by(|a, b| a.started_at_unix_ms.cmp(&b.started_at_unix_ms));
        out
    }
}

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

pub type SharedMetrics = Arc<MetricsCollector>;
pub type SharedSessions = Arc<SessionRegistry>;
