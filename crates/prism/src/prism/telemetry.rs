use std::{
    collections::BTreeMap,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{SystemTime, UNIX_EPOCH},
};

use dashmap::DashMap;
use serde::Serialize;

#[derive(Debug, Default)]
pub struct MetricsRegistry {
    active_connections: AtomicU64,
    connections_total: AtomicU64,
    bytes_ingress_total: AtomicU64,
    bytes_egress_total: AtomicU64,
    route_hits_total: DashMap<String, AtomicU64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsSnapshot {
    pub active_connections: u64,
    pub connections_total: u64,
    pub bytes_ingress_total: u64,
    pub bytes_egress_total: u64,
    pub route_hits_total: BTreeMap<String, u64>,
}

impl MetricsRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn connection_opened(&self) {
        self.connections_total.fetch_add(1, Ordering::Relaxed);
        self.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn connection_closed(&self) {
        let _ =
            self.active_connections
                .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                    current.checked_sub(1)
                });
    }

    pub fn record_bytes(&self, ingress: u64, egress: u64) {
        self.bytes_ingress_total
            .fetch_add(ingress, Ordering::Relaxed);
        self.bytes_egress_total.fetch_add(egress, Ordering::Relaxed);
    }

    pub fn record_route_hit(&self, host: &str) {
        let host = host.trim().to_ascii_lowercase();
        if host.is_empty() {
            return;
        }
        self.route_hits_total
            .entry(host)
            .or_insert_with(|| AtomicU64::new(0))
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> MetricsSnapshot {
        let route_hits_total = self
            .route_hits_total
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().load(Ordering::Relaxed)))
            .collect();

        MetricsSnapshot {
            active_connections: self.active_connections.load(Ordering::Relaxed),
            connections_total: self.connections_total.load(Ordering::Relaxed),
            bytes_ingress_total: self.bytes_ingress_total.load(Ordering::Relaxed),
            bytes_egress_total: self.bytes_egress_total.load(Ordering::Relaxed),
            route_hits_total,
        }
    }
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

pub type SharedSessions = Arc<SessionRegistry>;

pub type SharedMetrics = Arc<MetricsRegistry>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metrics_registry_records_snapshot() {
        let metrics = MetricsRegistry::new();

        metrics.connection_opened();
        metrics.connection_opened();
        metrics.connection_closed();
        metrics.record_bytes(128, 256);
        metrics.record_route_hit("Play.Example.Com");
        metrics.record_route_hit("play.example.com");

        let snap = metrics.snapshot();
        assert_eq!(snap.active_connections, 1);
        assert_eq!(snap.connections_total, 2);
        assert_eq!(snap.bytes_ingress_total, 128);
        assert_eq!(snap.bytes_egress_total, 256);
        assert_eq!(snap.route_hits_total.get("play.example.com"), Some(&2));
    }
}
