use std::{
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
    time::{SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use dashmap::DashMap;
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use serde::Serialize;

/// Installs a Prometheus recorder for the `metrics` crate and returns a handle used to render
/// the exposition format.
///
/// This should be called once per process at startup.
pub fn init_prometheus() -> anyhow::Result<PrometheusHandle> {
    PrometheusBuilder::new()
        .install_recorder()
        .context("metrics: install Prometheus recorder")
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

pub type SharedPrometheusHandle = Arc<PrometheusHandle>;
