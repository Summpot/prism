use std::{
    collections::BTreeMap,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use dashmap::DashMap;
use duckdb::{Connection, params};
use serde::Serialize;
use tokio::sync::watch;

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
    pub store: Option<MetricsStoreSnapshot>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MetricsStoreSnapshot {
    pub backend: String,
    pub path: String,
    pub flush_interval_ms: u64,
    pub last_flush_unix_ms: u64,
    pub last_error: String,
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
        self.snapshot_with_store(None)
    }

    pub fn snapshot_with_store(&self, store: Option<MetricsStoreSnapshot>) -> MetricsSnapshot {
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
            store,
        }
    }
}

#[derive(Debug)]
pub struct DuckdbMetricsStore {
    path: PathBuf,
    flush_interval: Duration,
    last_flush_unix_ms: AtomicU64,
    last_error: std::sync::RwLock<String>,
}

impl DuckdbMetricsStore {
    pub fn open(path: PathBuf, flush_interval: Duration) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("metrics: mkdir {}", parent.display()))?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("metrics: open DuckDB {}", path.display()))?;
        init_duckdb_schema(&conn)?;

        Ok(Self {
            path,
            flush_interval,
            last_flush_unix_ms: AtomicU64::new(0),
            last_error: std::sync::RwLock::new(String::new()),
        })
    }

    #[cfg(test)]
    pub fn open_in_memory(flush_interval: Duration) -> anyhow::Result<Self> {
        let conn = Connection::open_in_memory().context("metrics: open in-memory DuckDB")?;
        init_duckdb_schema(&conn)?;
        Ok(Self {
            path: PathBuf::from(":memory:"),
            flush_interval,
            last_flush_unix_ms: AtomicU64::new(0),
            last_error: std::sync::RwLock::new(String::new()),
        })
    }

    pub fn flush_interval(&self) -> Duration {
        self.flush_interval
    }

    pub fn snapshot(&self) -> MetricsStoreSnapshot {
        MetricsStoreSnapshot {
            backend: "duckdb".to_string(),
            path: self.path.display().to_string(),
            flush_interval_ms: self.flush_interval.as_millis() as u64,
            last_flush_unix_ms: self.last_flush_unix_ms.load(Ordering::Relaxed),
            last_error: self
                .last_error
                .read()
                .map(|s| s.clone())
                .unwrap_or_default(),
        }
    }

    pub fn flush_snapshot(&self, snapshot: &MetricsSnapshot) -> anyhow::Result<()> {
        let conn = Connection::open(&self.path)
            .with_context(|| format!("metrics: open DuckDB {}", self.path.display()))?;
        init_duckdb_schema(&conn)?;
        write_duckdb_snapshot(&conn, snapshot)?;
        self.last_flush_unix_ms
            .store(now_unix_ms(), Ordering::Relaxed);
        self.set_last_error(String::new());
        Ok(())
    }

    fn set_last_error(&self, value: String) {
        if let Ok(mut last_error) = self.last_error.write() {
            *last_error = value;
        }
    }
}

pub async fn run_duckdb_metrics_flush_loop(
    metrics: SharedMetrics,
    store: Arc<DuckdbMetricsStore>,
    mut shutdown: watch::Receiver<bool>,
) {
    let mut tick = tokio::time::interval(store.flush_interval());
    loop {
        tokio::select! {
            _ = shutdown.changed() => {
                if *shutdown.borrow() {
                    let snap = metrics.snapshot();
                    if let Err(err) = store.flush_snapshot(&snap) {
                        store.set_last_error(err.to_string());
                    }
                    break;
                }
            }
            _ = tick.tick() => {
                let snap = metrics.snapshot();
                if let Err(err) = store.flush_snapshot(&snap) {
                    let msg = err.to_string();
                    store.set_last_error(msg.clone());
                    tracing::warn!(err = %msg, "metrics: DuckDB flush failed");
                }
            }
        }
    }
}

fn init_duckdb_schema(conn: &Connection) -> anyhow::Result<()> {
    conn.execute_batch(
        r#"
CREATE TABLE IF NOT EXISTS metrics_snapshots (
    ts_unix_ms UBIGINT,
    active_connections UBIGINT,
    connections_total UBIGINT,
    bytes_ingress_total UBIGINT,
    bytes_egress_total UBIGINT
);

CREATE TABLE IF NOT EXISTS metrics_route_hits (
    ts_unix_ms UBIGINT,
    host TEXT,
    hits_total UBIGINT
);
"#,
    )
    .context("metrics: init DuckDB schema")
}

fn write_duckdb_snapshot(conn: &Connection, snapshot: &MetricsSnapshot) -> anyhow::Result<()> {
    let ts = now_unix_ms() as i64;
    conn.execute(
        r#"
INSERT INTO metrics_snapshots (
    ts_unix_ms,
    active_connections,
    connections_total,
    bytes_ingress_total,
    bytes_egress_total
) VALUES (?, ?, ?, ?, ?)
"#,
        params![
            ts,
            snapshot.active_connections as i64,
            snapshot.connections_total as i64,
            snapshot.bytes_ingress_total as i64,
            snapshot.bytes_egress_total as i64
        ],
    )
    .context("metrics: insert snapshot")?;

    for (host, hits_total) in &snapshot.route_hits_total {
        conn.execute(
            r#"
INSERT INTO metrics_route_hits (
    ts_unix_ms,
    host,
    hits_total
) VALUES (?, ?, ?)
"#,
            params![ts, host, *hits_total as i64],
        )
        .with_context(|| format!("metrics: insert route hits for {host}"))?;
    }

    Ok(())
}

pub fn resolve_metrics_duckdb_path(workdir: &Path, configured_path: &str) -> PathBuf {
    let p = PathBuf::from(configured_path.trim());
    if p.as_os_str().is_empty() {
        workdir.join("metrics.duckdb")
    } else if p.is_relative() {
        workdir.join(p)
    } else {
        p
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

    #[test]
    fn duckdb_metrics_store_writes_snapshot() {
        let store = DuckdbMetricsStore::open_in_memory(Duration::from_secs(1)).expect("store");
        let metrics = MetricsRegistry::new();

        metrics.connection_opened();
        metrics.record_route_hit("play.example.com");
        let snap = metrics.snapshot();

        store.flush_snapshot(&snap).expect("flush");
        let store_snap = store.snapshot();
        assert_eq!(store_snap.backend, "duckdb");
        assert!(store_snap.last_flush_unix_ms > 0);
        assert!(store_snap.last_error.is_empty());
    }

    #[test]
    fn metrics_duckdb_path_resolves_relative_to_workdir() {
        let workdir = PathBuf::from("C:/tmp/prism-work");
        assert_eq!(
            resolve_metrics_duckdb_path(&workdir, ""),
            workdir.join("metrics.duckdb")
        );
        assert_eq!(
            resolve_metrics_duckdb_path(&workdir, "local.duckdb"),
            workdir.join("local.duckdb")
        );
    }
}
