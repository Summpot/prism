use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::prism::admin::ClientProfile;
use crate::prism::tunnel::optimizer::OptimizerStatsSnapshot;

// ============================================================================
// Data Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClientConfigState {
    pub profile_name: String,
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub listen_addr: String,
    pub fake_lan_broadcast: bool,
    pub auto_connect_panel: bool,
}

impl Default for ClientConfigState {
    fn default() -> Self {
        Self {
            profile_name: "Default Realm".into(),
            server_addr: "127.0.0.1:7000".into(),
            transport: "quic".into(),
            auth_token: "".into(),
            listen_addr: "127.0.0.1:25565".into(),
            fake_lan_broadcast: true,
            auto_connect_panel: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct ClientCumulativeStats {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub sessions_count: u64,
    pub last_session_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClientConfigResponse {
    pub active_profile_id: Option<String>,
    pub active_config: ClientConfigState,
    pub profiles: Vec<ClientProfile>,
    pub cumulative_stats: OptimizerStatsSnapshot,
}

// ============================================================================
// Storage Engine
// ============================================================================

pub struct StorageEngine {
    conn: Arc<Mutex<Connection>>,
}

impl std::fmt::Debug for StorageEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageEngine").finish_non_exhaustive()
    }
}

impl StorageEngine {
    /// Opens or creates the SQLite database at the specified file path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let conn = Connection::open(path)
            .with_context(|| format!("failed to open or create sqlite at {}", path.display()))?;

        // Configure connection for high-concurrency, low-latency daemon operations
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA busy_timeout = 5000;
             PRAGMA synchronous = NORMAL;
             PRAGMA foreign_keys = ON;",
        )?;

        // Create structured tables
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS client_profiles (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL,
                server_addr TEXT NOT NULL,
                transport TEXT NOT NULL,
                auth_token TEXT NOT NULL DEFAULT '',
                listen_addr TEXT NOT NULL,
                fake_lan_broadcast INTEGER NOT NULL DEFAULT 1,
                display_order INTEGER NOT NULL DEFAULT 0,
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS client_state (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS cumulative_stats (
                scope TEXT PRIMARY KEY,
                raw_bytes INTEGER NOT NULL DEFAULT 0,
                wire_bytes INTEGER NOT NULL DEFAULT 0,
                saved_bytes INTEGER NOT NULL DEFAULT 0,
                saved_ratio REAL NOT NULL DEFAULT 0.0,
                sessions_count INTEGER NOT NULL DEFAULT 0,
                last_session_at INTEGER NOT NULL DEFAULT 0
            );",
        )?;

        Ok(Self {
            conn: Arc::new(Mutex::new(conn)),
        })
    }

    // ========================================================================
    // Client Profiles Operations
    // ========================================================================

    /// Loads all saved client profiles ordered by display order and creation.
    pub fn load_profiles(&self) -> anyhow::Result<Vec<ClientProfile>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT id, name, server_addr, transport, auth_token, listen_addr, fake_lan_broadcast
             FROM client_profiles
             ORDER BY display_order ASC, updated_at ASC",
        )?;

        let rows = stmt.query_map([], |row| {
            let fake_lan: i64 = row.get(6)?;
            Ok(ClientProfile {
                id: row.get(0)?,
                name: row.get(1)?,
                server_addr: row.get(2)?,
                transport: row.get(3)?,
                auth_token: row.get(4)?,
                listen_addr: row.get(5)?,
                fake_lan_broadcast: fake_lan != 0,
            })
        })?;

        let mut out = Vec::new();
        for r in rows {
            out.push(r?);
        }
        Ok(out)
    }

    /// Persists the full list of client profiles atomically in a single transaction.
    pub fn save_profiles(&self, profiles: &[ClientProfile]) -> anyhow::Result<()> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM client_profiles", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO client_profiles (id, name, server_addr, transport, auth_token, listen_addr, fake_lan_broadcast, display_order, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, unixepoch())",
            )?;
            for (idx, p) in profiles.iter().enumerate() {
                stmt.execute(params![
                    p.id,
                    p.name,
                    p.server_addr,
                    p.transport,
                    p.auth_token,
                    p.listen_addr,
                    if p.fake_lan_broadcast { 1i64 } else { 0i64 },
                    idx as i64,
                ])?;
            }
        }
        tx.commit()?;
        Ok(())
    }

    // ========================================================================
    // Active Profile & Form Config State Operations
    // ========================================================================

    /// Loads the currently selected active profile ID.
    pub fn load_active_profile_id(&self) -> anyhow::Result<Option<String>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt =
            conn.prepare("SELECT value FROM client_state WHERE key = 'active_profile_id'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            if val.trim().is_empty() {
                Ok(None)
            } else {
                Ok(Some(val))
            }
        } else {
            Ok(None)
        }
    }

    /// Saves the currently selected active profile ID.
    pub fn save_active_profile_id(&self, id: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "INSERT INTO client_state (key, value) VALUES ('active_profile_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![id],
        )?;
        Ok(())
    }

    /// Loads the active form configuration state.
    pub fn load_active_config(&self) -> anyhow::Result<ClientConfigState> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt =
            conn.prepare("SELECT value FROM client_state WHERE key = 'active_config'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let json_str: String = row.get(0)?;
            let cfg: ClientConfigState = serde_json::from_str(&json_str).unwrap_or_default();
            Ok(cfg)
        } else {
            Ok(ClientConfigState::default())
        }
    }

    /// Saves the active form configuration state.
    pub fn save_active_config(&self, config: &ClientConfigState) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(config)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "INSERT INTO client_state (key, value) VALUES ('active_config', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![json_str],
        )?;
        Ok(())
    }

    /// Gets a consolidated snapshot of the client configuration, profiles, and cumulative stats.
    pub fn get_client_config_snapshot(&self) -> ClientConfigResponse {
        let profiles = self.load_profiles().unwrap_or_default();
        let active_profile_id = self.load_active_profile_id().unwrap_or_default();
        let active_config = self.load_active_config().unwrap_or_default();
        let cum = self.load_cumulative_stats().unwrap_or_default();
        let cumulative_stats = OptimizerStatsSnapshot {
            raw_bytes: cum.raw_bytes,
            wire_bytes: cum.wire_bytes,
            saved_bytes: cum.saved_bytes,
            saved_ratio: cum.saved_ratio,
            ..Default::default()
        };
        ClientConfigResponse {
            active_profile_id,
            active_config,
            profiles,
            cumulative_stats,
        }
    }

    // ========================================================================
    // Scoped Cumulative Statistics Operations
    // ========================================================================

    /// Loads client-scoped cumulative lifetime statistics.
    pub fn load_cumulative_stats(&self) -> anyhow::Result<ClientCumulativeStats> {
        self.load_scoped_stats("client")
    }

    /// Loads cumulative lifetime statistics for any specified scope (e.g. 'client', 'connector:<name>', 'server').
    pub fn load_scoped_stats(&self, scope: &str) -> anyhow::Result<ClientCumulativeStats> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt = conn.prepare(
            "SELECT raw_bytes, wire_bytes, saved_bytes, saved_ratio, sessions_count, last_session_at
             FROM cumulative_stats
             WHERE scope = ?1",
        )?;
        let mut rows = stmt.query(params![scope])?;
        if let Some(row) = rows.next()? {
            let raw: i64 = row.get(0)?;
            let wire: i64 = row.get(1)?;
            let saved: i64 = row.get(2)?;
            let ratio: f64 = row.get(3)?;
            let count: i64 = row.get(4)?;
            let last_at: i64 = row.get(5)?;
            Ok(ClientCumulativeStats {
                raw_bytes: raw as u64,
                wire_bytes: wire as u64,
                saved_bytes: saved as u64,
                saved_ratio: ratio,
                sessions_count: count as u64,
                last_session_at: last_at as u64,
            })
        } else {
            Ok(ClientCumulativeStats::default())
        }
    }

    /// Records completed session statistics into client cumulative metrics.
    pub fn record_session_stats(
        &self,
        session: &OptimizerStatsSnapshot,
    ) -> anyhow::Result<ClientCumulativeStats> {
        self.record_scoped_session_stats("client", session)
    }

    /// Records completed session statistics into scoped cumulative metrics.
    pub fn record_scoped_session_stats(
        &self,
        scope: &str,
        session: &OptimizerStatsSnapshot,
    ) -> anyhow::Result<ClientCumulativeStats> {
        let prev = self.load_scoped_stats(scope)?;
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let raw_bytes = prev.raw_bytes.saturating_add(session.raw_bytes);
        let wire_bytes = prev.wire_bytes.saturating_add(session.wire_bytes);
        let saved_bytes = if raw_bytes >= wire_bytes {
            raw_bytes - wire_bytes
        } else {
            0
        };
        let saved_ratio = if raw_bytes > 0 && wire_bytes <= raw_bytes {
            (raw_bytes - wire_bytes) as f64 / raw_bytes as f64
        } else {
            0.0
        };
        let sessions_count = prev.sessions_count.saturating_add(1);
        let last_session_at = now;

        let next = ClientCumulativeStats {
            raw_bytes,
            wire_bytes,
            saved_bytes,
            saved_ratio,
            sessions_count,
            last_session_at,
        };

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "INSERT INTO cumulative_stats (scope, raw_bytes, wire_bytes, saved_bytes, saved_ratio, sessions_count, last_session_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
             ON CONFLICT(scope) DO UPDATE SET
                raw_bytes = excluded.raw_bytes,
                wire_bytes = excluded.wire_bytes,
                saved_bytes = excluded.saved_bytes,
                saved_ratio = excluded.saved_ratio,
                sessions_count = excluded.sessions_count,
                last_session_at = excluded.last_session_at",
            params![
                scope,
                raw_bytes as i64,
                wire_bytes as i64,
                saved_bytes as i64,
                saved_ratio,
                sessions_count as i64,
                last_session_at as i64,
            ],
        )?;

        Ok(next)
    }

    /// Resets client cumulative statistics back to zero.
    pub fn reset_cumulative_stats(&self) -> anyhow::Result<()> {
        self.reset_scoped_stats("client")
    }

    /// Resets cumulative statistics for a specific scope back to zero.
    pub fn reset_scoped_stats(&self, scope: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "DELETE FROM cumulative_stats WHERE scope = ?1",
            params![scope],
        )?;
        Ok(())
    }
}

// ============================================================================
// Unit Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path() -> std::path::PathBuf {
        let n = rand::random::<u64>();
        std::env::temp_dir().join(format!("prism_test_{n}.db"))
    }

    #[test]
    fn test_storage_open_and_profiles_roundtrip() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open sqlite");

        let initial = storage.load_profiles().unwrap();
        assert!(initial.is_empty());

        let profiles = vec![
            ClientProfile {
                id: "p1".into(),
                name: "Server One".into(),
                server_addr: "1.1.1.1:7000".into(),
                transport: "quic".into(),
                auth_token: "tok1".into(),
                listen_addr: "127.0.0.1:25565".into(),
                fake_lan_broadcast: true,
            },
            ClientProfile {
                id: "p2".into(),
                name: "Server Two".into(),
                server_addr: "2.2.2.2:7000".into(),
                transport: "kcp".into(),
                auth_token: "".into(),
                listen_addr: "127.0.0.1:25566".into(),
                fake_lan_broadcast: false,
            },
        ];

        storage.save_profiles(&profiles).unwrap();
        let loaded = storage.load_profiles().unwrap();
        assert_eq!(loaded, profiles);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_storage_active_config_and_profile_id() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open sqlite");

        assert_eq!(storage.load_active_profile_id().unwrap(), None);

        storage.save_active_profile_id("test-id-123").unwrap();
        assert_eq!(
            storage.load_active_profile_id().unwrap().as_deref(),
            Some("test-id-123")
        );

        let cfg = ClientConfigState {
            profile_name: "My Custom Realm".into(),
            server_addr: "play.custom.gg:7000".into(),
            transport: "kcp".into(),
            auth_token: "xyz".into(),
            listen_addr: "0.0.0.0:25565".into(),
            fake_lan_broadcast: false,
            auto_connect_panel: true,
        };

        storage.save_active_config(&cfg).unwrap();
        let loaded_cfg = storage.load_active_config().unwrap();
        assert_eq!(loaded_cfg, cfg);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_storage_cumulative_stats_accumulation() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open sqlite");

        let init = storage.load_cumulative_stats().unwrap();
        assert_eq!(init.raw_bytes, 0);
        assert_eq!(init.sessions_count, 0);

        let delta1 = OptimizerStatsSnapshot {
            raw_bytes: 1000,
            wire_bytes: 600,
            saved_ratio: 0.4,
            ..Default::default()
        };

        let cum1 = storage.record_session_stats(&delta1).unwrap();
        assert_eq!(cum1.raw_bytes, 1000);
        assert_eq!(cum1.wire_bytes, 600);
        assert_eq!(cum1.saved_bytes, 400);
        assert!((cum1.saved_ratio - 0.4).abs() < 1e-4);
        assert_eq!(cum1.sessions_count, 1);

        let delta2 = OptimizerStatsSnapshot {
            raw_bytes: 2000,
            wire_bytes: 1200,
            saved_ratio: 0.4,
            ..Default::default()
        };

        let cum2 = storage.record_session_stats(&delta2).unwrap();
        assert_eq!(cum2.raw_bytes, 3000);
        assert_eq!(cum2.wire_bytes, 1800);
        assert_eq!(cum2.saved_bytes, 1200);
        assert!((cum2.saved_ratio - 0.4).abs() < 1e-4);
        assert_eq!(cum2.sessions_count, 2);

        storage.reset_cumulative_stats().unwrap();
        let cum_reset = storage.load_cumulative_stats().unwrap();
        assert_eq!(cum_reset.raw_bytes, 0);

        let _ = std::fs::remove_file(path);
    }
}
