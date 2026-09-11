use std::{
    path::Path,
    sync::{Arc, Mutex},
};

use anyhow::Context;
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};

use crate::prism::admin::ClientProfile;
use crate::prism::secrets;
use crate::prism::tunnel::optimizer::OptimizerStatsSnapshot;

// ============================================================================
// Data Models
// ============================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClientConfigState {
    pub profile_name: String,
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub listen_addr: String,
    pub fake_lan_broadcast: bool,
    pub auto_connect_panel: bool,
    pub auto_connect: bool,
    pub management_url: String,
    pub token_id: String,
    pub token_type: String,
    pub user_id: String,
    pub username: String,
    pub expires_at: Option<u64>,
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
            auto_connect: true,
            management_url: String::new(),
            token_id: String::new(),
            token_type: String::new(),
            user_id: String::new(),
            username: String::new(),
            expires_at: None,
        }
    }
}

/// Partial update for `/client/config` and Tauri `client_save_config`.
/// Missing fields leave the stored value unchanged.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ClientConfigPatch {
    #[serde(default)]
    pub profile_name: Option<String>,
    #[serde(default)]
    pub server_addr: Option<String>,
    #[serde(default)]
    pub transport: Option<String>,
    #[serde(default)]
    pub auth_token: Option<String>,
    #[serde(default)]
    pub listen_addr: Option<String>,
    #[serde(default)]
    pub fake_lan_broadcast: Option<bool>,
    #[serde(default)]
    pub auto_connect_panel: Option<bool>,
    #[serde(default)]
    pub auto_connect: Option<bool>,
    #[serde(default)]
    pub management_url: Option<String>,
    #[serde(default)]
    pub token_id: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub expires_at: Option<u64>,
}

impl ClientConfigPatch {
    pub fn is_empty(&self) -> bool {
        self.profile_name.is_none()
            && self.server_addr.is_none()
            && self.transport.is_none()
            && self.auth_token.is_none()
            && self.listen_addr.is_none()
            && self.fake_lan_broadcast.is_none()
            && self.auto_connect_panel.is_none()
            && self.auto_connect.is_none()
            && self.management_url.is_none()
            && self.token_id.is_none()
            && self.token_type.is_none()
            && self.user_id.is_none()
            && self.username.is_none()
            && self.expires_at.is_none()
    }
}

impl ClientConfigState {
    pub fn apply_patch(&mut self, patch: &ClientConfigPatch) {
        if let Some(v) = patch.profile_name.as_ref().filter(|s| !s.is_empty()) {
            self.profile_name = v.clone();
        }
        if let Some(v) = &patch.server_addr {
            self.server_addr = v.clone();
        }
        if let Some(v) = &patch.transport {
            self.transport = v.clone();
        }
        if let Some(v) = patch.auth_token.as_ref().filter(|s| !s.trim().is_empty()) {
            self.auth_token = v.clone();
        }
        if let Some(v) = &patch.listen_addr {
            self.listen_addr = v.clone();
        }
        if let Some(v) = patch.fake_lan_broadcast {
            self.fake_lan_broadcast = v;
        }
        if let Some(v) = patch.auto_connect_panel {
            self.auto_connect_panel = v;
        }
        if let Some(v) = patch.auto_connect {
            self.auto_connect = v;
        }
        if let Some(v) = &patch.management_url {
            self.management_url = v.clone();
        }
        if let Some(v) = &patch.token_id {
            self.token_id = v.clone();
        }
        if let Some(v) = &patch.token_type {
            self.token_type = v.clone();
        }
        if let Some(v) = &patch.user_id {
            self.user_id = v.clone();
        }
        if let Some(v) = &patch.username {
            self.username = v.clone();
        }
        if patch.expires_at.is_some() {
            self.expires_at = patch.expires_at;
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct TunnelCredential {
    pub profile_id: String,
    pub server_addr: String,
    pub token_id: String,
    pub token_type: String,
    pub user_id: String,
    pub username: String,
    pub issued_at: u64,
    pub expires_at: Option<u64>,
    pub token: String,
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
    #[serde(default)]
    pub device_id: String,
}

// ============================================================================
// Storage Engine
// ============================================================================

pub struct StorageEngine {
    conn: Arc<Mutex<Connection>>,
    use_keyring: bool,
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
            );

            CREATE TABLE IF NOT EXISTS middleware_configs (
                name TEXT PRIMARY KEY,
                config_json TEXT NOT NULL,
                updated_at INTEGER NOT NULL DEFAULT (unixepoch())
            );

            CREATE TABLE IF NOT EXISTS client_credentials (
                profile_id TEXT PRIMARY KEY,
                server_addr TEXT NOT NULL,
                token_id TEXT NOT NULL DEFAULT '',
                token_type TEXT NOT NULL DEFAULT 'static',
                user_id TEXT NOT NULL DEFAULT '',
                username TEXT NOT NULL DEFAULT '',
                issued_at INTEGER NOT NULL DEFAULT 0,
                expires_at INTEGER,
                token_blob TEXT NOT NULL DEFAULT '',
                keyring_ok INTEGER NOT NULL DEFAULT 0
            );",
        )?;

        let engine = Self {
            conn: Arc::new(Mutex::new(conn)),
            use_keyring: !cfg!(test),
        };
        engine.migrate_legacy_plaintext_tokens()?;
        Ok(engine)
    }

    fn now_unix_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    fn infer_token_type(token: &str, explicit: &str) -> String {
        let explicit = explicit.trim();
        if !explicit.is_empty() {
            return explicit.to_string();
        }
        let token = token.trim();
        if token.starts_with("prism_cl_") || token.starts_with("prism_adm_") {
            "oauth_pat".into()
        } else if token.is_empty() {
            String::new()
        } else {
            "static".into()
        }
    }

    fn credential_expired(expires_at: Option<u64>) -> bool {
        match expires_at {
            Some(exp) if exp > 0 => Self::now_unix_ms() > exp,
            _ => false,
        }
    }

    fn migrate_legacy_plaintext_tokens(&self) -> anyhow::Result<()> {
        let profiles = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
            let mut stmt = conn.prepare(
                "SELECT id, server_addr, auth_token FROM client_profiles WHERE auth_token != ''",
            )?;
            let rows = stmt.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?;
            let mut out = Vec::new();
            for r in rows {
                out.push(r?);
            }
            out
        };

        for (id, server_addr, token) in profiles {
            let cred = TunnelCredential {
                profile_id: id.clone(),
                server_addr,
                token_id: String::new(),
                token_type: Self::infer_token_type(&token, ""),
                user_id: String::new(),
                username: String::new(),
                issued_at: Self::now_unix_ms(),
                expires_at: None,
                token,
            };
            self.upsert_credential(&cred)?;
            let conn = self
                .conn
                .lock()
                .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
            conn.execute(
                "UPDATE client_profiles SET auth_token = '' WHERE id = ?1",
                params![id],
            )?;
        }

        let mut active = self.load_active_config_raw()?;
        if !active.auth_token.trim().is_empty() {
            if let Some(id) = self.load_active_profile_id()? {
                let cred = TunnelCredential {
                    profile_id: id,
                    server_addr: active.server_addr.clone(),
                    token_id: active.token_id.clone(),
                    token_type: Self::infer_token_type(&active.auth_token, &active.token_type),
                    user_id: active.user_id.clone(),
                    username: active.username.clone(),
                    issued_at: Self::now_unix_ms(),
                    expires_at: active.expires_at,
                    token: active.auth_token.clone(),
                };
                self.upsert_credential(&cred)?;
            }
            active.auth_token.clear();
            self.write_active_config_json(&active)?;
        }

        Ok(())
    }

    // ========================================================================
    // Device identity
    // ========================================================================

    /// Returns a stable per-install device id, creating one on first use.
    pub fn load_or_create_device_id(&self) -> anyhow::Result<String> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt = conn.prepare("SELECT value FROM client_state WHERE key = 'device_id'")?;
        let mut rows = stmt.query([])?;
        if let Some(row) = rows.next()? {
            let val: String = row.get(0)?;
            if !val.trim().is_empty() {
                return Ok(val);
            }
        }
        drop(rows);
        drop(stmt);

        let mut bytes = [0u8; 16];
        {
            use rand::{RngExt, rng};
            rng().fill(&mut bytes);
        }
        let mut hex = String::with_capacity(32);
        for b in bytes {
            use std::fmt::Write;
            let _ = write!(hex, "{b:02x}");
        }
        let id = format!("prism_dev_{hex}");
        conn.execute(
            "INSERT INTO client_state (key, value) VALUES ('device_id', ?1)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value",
            params![id],
        )?;
        Ok(id)
    }

    // ========================================================================
    // Tunnel credentials
    // ========================================================================

    /// Inserts or replaces the tunnel credential bound to a profile.
    pub fn upsert_credential(&self, cred: &TunnelCredential) -> anyhow::Result<()> {
        let token = cred.token.trim();
        if cred.profile_id.trim().is_empty() || token.is_empty() {
            return Ok(());
        }

        let mut keyring_ok = false;
        let mut token_blob = String::new();
        if self.use_keyring && secrets::store_tunnel_token(&cred.profile_id, token) {
            keyring_ok = true;
        } else {
            token_blob = token.to_string();
        }

        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "INSERT INTO client_credentials (
                profile_id, server_addr, token_id, token_type, user_id, username,
                issued_at, expires_at, token_blob, keyring_ok
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)
             ON CONFLICT(profile_id) DO UPDATE SET
                server_addr = excluded.server_addr,
                token_id = excluded.token_id,
                token_type = excluded.token_type,
                user_id = excluded.user_id,
                username = excluded.username,
                issued_at = excluded.issued_at,
                expires_at = excluded.expires_at,
                token_blob = excluded.token_blob,
                keyring_ok = excluded.keyring_ok",
            params![
                cred.profile_id,
                cred.server_addr,
                cred.token_id,
                Self::infer_token_type(token, &cred.token_type),
                cred.user_id,
                cred.username,
                cred.issued_at as i64,
                cred.expires_at.map(|v| v as i64),
                token_blob,
                if keyring_ok { 1i64 } else { 0i64 },
            ],
        )?;
        Ok(())
    }

    /// Loads the credential for a profile, including the raw token when available.
    pub fn load_credential(&self, profile_id: &str) -> anyhow::Result<Option<TunnelCredential>> {
        let row = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
            let mut stmt = conn.prepare(
                "SELECT server_addr, token_id, token_type, user_id, username, issued_at, expires_at, token_blob, keyring_ok
                 FROM client_credentials WHERE profile_id = ?1",
            )?;
            let mut rows = stmt.query(params![profile_id])?;
            if let Some(row) = rows.next()? {
                let expires_at: Option<i64> = row.get(6)?;
                Some(TunnelCredential {
                    profile_id: profile_id.to_string(),
                    server_addr: row.get(0)?,
                    token_id: row.get(1)?,
                    token_type: row.get(2)?,
                    user_id: row.get(3)?,
                    username: row.get(4)?,
                    issued_at: row.get::<_, i64>(5)? as u64,
                    expires_at: expires_at.filter(|v| *v > 0).map(|v| v as u64),
                    token: row.get::<_, String>(7)?,
                })
            } else {
                None
            }
        };

        let Some(mut cred) = row else {
            return Ok(None);
        };

        if Self::credential_expired(cred.expires_at) {
            self.delete_credential(profile_id)?;
            return Ok(None);
        }

        if self.use_keyring {
            if let Some(token) = secrets::load_tunnel_token(profile_id) {
                cred.token = token;
            }
        }

        if cred.token.trim().is_empty() {
            return Ok(None);
        }
        Ok(Some(cred))
    }

    /// Deletes the credential (keyring + sqlite) for a profile.
    pub fn delete_credential(&self, profile_id: &str) -> anyhow::Result<()> {
        if self.use_keyring {
            secrets::delete_tunnel_token(profile_id);
        }
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "DELETE FROM client_credentials WHERE profile_id = ?1",
            params![profile_id],
        )?;
        Ok(())
    }

    fn hydrate_profile_token(&self, profile: &mut ClientProfile) {
        match self.load_credential(&profile.id) {
            Ok(Some(cred)) if cred.server_addr == profile.server_addr => {
                profile.auth_token = cred.token;
            }
            _ => {}
        }
    }

    fn credential_from_profile_and_config(
        profile_id: &str,
        profile: Option<&ClientProfile>,
        config: &ClientConfigState,
    ) -> Option<TunnelCredential> {
        let token = config.auth_token.trim();
        let token = if token.is_empty() {
            profile.map(|p| p.auth_token.trim()).unwrap_or("")
        } else {
            token
        };
        if token.is_empty() {
            return None;
        }
        Some(TunnelCredential {
            profile_id: profile_id.to_string(),
            server_addr: if !config.server_addr.trim().is_empty() {
                config.server_addr.clone()
            } else {
                profile.map(|p| p.server_addr.clone()).unwrap_or_default()
            },
            token_id: config.token_id.clone(),
            token_type: Self::infer_token_type(token, &config.token_type),
            user_id: config.user_id.clone(),
            username: config.username.clone(),
            issued_at: Self::now_unix_ms(),
            expires_at: config.expires_at,
            token: token.to_string(),
        })
    }

    /// Inserts or updates a single profile without replacing the rest of the list.
    pub fn upsert_profile(&self, profile: &ClientProfile) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let exists: bool = conn
            .query_row(
                "SELECT 1 FROM client_profiles WHERE id = ?1",
                params![profile.id],
                |_| Ok(true),
            )
            .unwrap_or(false);
        if exists {
            conn.execute(
                "UPDATE client_profiles SET
                    name = ?2,
                    server_addr = ?3,
                    transport = ?4,
                    auth_token = '',
                    listen_addr = ?5,
                    fake_lan_broadcast = ?6,
                    updated_at = unixepoch()
                 WHERE id = ?1",
                params![
                    profile.id,
                    profile.name,
                    profile.server_addr,
                    profile.transport,
                    profile.listen_addr,
                    if profile.fake_lan_broadcast {
                        1i64
                    } else {
                        0i64
                    },
                ],
            )?;
        } else {
            let next_order: i64 = conn
                .query_row(
                    "SELECT IFNULL(MAX(display_order), -1) + 1 FROM client_profiles",
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);
            conn.execute(
                "INSERT INTO client_profiles (
                    id, name, server_addr, transport, auth_token, listen_addr,
                    fake_lan_broadcast, display_order, updated_at
                 ) VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, unixepoch())",
                params![
                    profile.id,
                    profile.name,
                    profile.server_addr,
                    profile.transport,
                    profile.listen_addr,
                    if profile.fake_lan_broadcast {
                        1i64
                    } else {
                        0i64
                    },
                    next_order,
                ],
            )?;
        }
        drop(conn);

        if !profile.auth_token.trim().is_empty() {
            self.upsert_credential(&TunnelCredential {
                profile_id: profile.id.clone(),
                server_addr: profile.server_addr.clone(),
                token_id: String::new(),
                token_type: Self::infer_token_type(&profile.auth_token, ""),
                user_id: String::new(),
                username: String::new(),
                issued_at: Self::now_unix_ms(),
                expires_at: None,
                token: profile.auth_token.clone(),
            })?;
        }
        Ok(())
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
        drop(stmt);
        drop(conn);
        for profile in &mut out {
            self.hydrate_profile_token(profile);
        }
        Ok(out)
    }

    /// Persists the full list of client profiles atomically in a single transaction.
    /// Empty `auth_token` values keep any previously stored credential.
    pub fn save_profiles(&self, profiles: &[ClientProfile]) -> anyhow::Result<()> {
        let existing_ids: Vec<String> = {
            let conn = self
                .conn
                .lock()
                .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
            let mut stmt = conn.prepare("SELECT id FROM client_profiles")?;
            let rows = stmt.query_map([], |row| row.get(0))?;
            let mut ids = Vec::new();
            for r in rows {
                ids.push(r?);
            }
            ids
        };

        let mut conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let tx = conn.transaction()?;
        tx.execute("DELETE FROM client_profiles", [])?;
        {
            let mut stmt = tx.prepare(
                "INSERT INTO client_profiles (id, name, server_addr, transport, auth_token, listen_addr, fake_lan_broadcast, display_order, updated_at)
                 VALUES (?1, ?2, ?3, ?4, '', ?5, ?6, ?7, unixepoch())",
            )?;
            for (idx, p) in profiles.iter().enumerate() {
                stmt.execute(params![
                    p.id,
                    p.name,
                    p.server_addr,
                    p.transport,
                    p.listen_addr,
                    if p.fake_lan_broadcast { 1i64 } else { 0i64 },
                    idx as i64,
                ])?;
            }
        }
        tx.commit()?;
        drop(conn);

        let keep: std::collections::HashSet<&str> =
            profiles.iter().map(|p| p.id.as_str()).collect();
        for id in existing_ids {
            if !keep.contains(id.as_str()) {
                let _ = self.delete_credential(&id);
            }
        }
        for p in profiles {
            if !p.auth_token.trim().is_empty() {
                self.upsert_credential(&TunnelCredential {
                    profile_id: p.id.clone(),
                    server_addr: p.server_addr.clone(),
                    token_id: String::new(),
                    token_type: Self::infer_token_type(&p.auth_token, ""),
                    user_id: String::new(),
                    username: String::new(),
                    issued_at: Self::now_unix_ms(),
                    expires_at: None,
                    token: p.auth_token.clone(),
                })?;
            }
        }
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

    fn load_active_config_raw(&self) -> anyhow::Result<ClientConfigState> {
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

    fn write_active_config_json(&self, config: &ClientConfigState) -> anyhow::Result<()> {
        let mut stored = config.clone();
        stored.auth_token.clear();
        let json_str = serde_json::to_string(&stored)?;
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

    /// Loads the active form configuration state, hydrating the tunnel token from the credential store.
    pub fn load_active_config(&self) -> anyhow::Result<ClientConfigState> {
        let mut cfg = self.load_active_config_raw()?;
        if let Some(id) = self.load_active_profile_id()? {
            if let Ok(Some(cred)) = self.load_credential(&id) {
                if cred.server_addr == cfg.server_addr
                    || cfg.server_addr.trim().is_empty()
                    || cfg.server_addr == ClientConfigState::default().server_addr
                {
                    cfg.auth_token = cred.token;
                    if cfg.token_id.is_empty() {
                        cfg.token_id = cred.token_id;
                    }
                    if cfg.token_type.is_empty() {
                        cfg.token_type = cred.token_type;
                    }
                    if cfg.user_id.is_empty() {
                        cfg.user_id = cred.user_id;
                    }
                    if cfg.username.is_empty() {
                        cfg.username = cred.username;
                    }
                    if cfg.expires_at.is_none() {
                        cfg.expires_at = cred.expires_at;
                    }
                }
            }
        }
        Ok(cfg)
    }

    /// Saves the active form configuration state. Raw tokens are stored via the credential store.
    pub fn save_active_config(&self, config: &ClientConfigState) -> anyhow::Result<()> {
        if let Some(id) = self.load_active_profile_id()?
            && let Some(cred) = Self::credential_from_profile_and_config(&id, None, config)
        {
            self.upsert_credential(&cred)?;
        }
        self.write_active_config_json(config)
    }

    /// Applies a partial config update, preserving unspecified fields and existing credentials.
    pub fn apply_config_patch(
        &self,
        active_profile_id: Option<&str>,
        patch: &ClientConfigPatch,
    ) -> anyhow::Result<ClientConfigState> {
        if let Some(id) = active_profile_id.filter(|s| !s.trim().is_empty()) {
            self.save_active_profile_id(id)?;
            if patch.is_empty() {
                if let Some(profile) = self.load_profiles()?.into_iter().find(|item| item.id == id)
                {
                    let mut cfg = self.load_active_config_raw()?;
                    cfg.profile_name = profile.name;
                    cfg.server_addr = profile.server_addr;
                    cfg.transport = profile.transport;
                    cfg.listen_addr = profile.listen_addr;
                    cfg.fake_lan_broadcast = profile.fake_lan_broadcast;
                    cfg.auth_token = profile.auth_token;
                    self.write_active_config_json(&cfg)?;
                }
                return self.load_active_config();
            }
        }
        let mut cfg = self.load_active_config()?;
        if let Some(id) = self.load_active_profile_id()? {
            if let Some(profile) = self.load_profiles()?.into_iter().find(|item| item.id == id) {
                if cfg.auth_token.trim().is_empty() {
                    cfg.auth_token = profile.auth_token.clone();
                }
                if patch.server_addr.is_none() && cfg.server_addr != profile.server_addr {
                    cfg.profile_name = profile.name;
                    cfg.server_addr = profile.server_addr;
                    if patch.transport.is_none() {
                        cfg.transport = profile.transport;
                    }
                    if patch.listen_addr.is_none() {
                        cfg.listen_addr = profile.listen_addr;
                    }
                    if patch.fake_lan_broadcast.is_none() {
                        cfg.fake_lan_broadcast = profile.fake_lan_broadcast;
                    }
                }
            }
        }
        cfg.apply_patch(patch);

        let mut profile_id = self.load_active_profile_id()?;
        if profile_id.is_none()
            && (!cfg.server_addr.trim().is_empty() || !cfg.auth_token.trim().is_empty())
        {
            let generated = format!("profile-{}", Self::now_unix_ms());
            self.save_active_profile_id(&generated)?;
            profile_id = Some(generated);
        }

        if let Some(id) = profile_id {
            if let Some(cred) = Self::credential_from_profile_and_config(&id, None, &cfg) {
                self.upsert_credential(&cred)?;
            }
            let mut profile = ClientProfile {
                id: id.clone(),
                name: cfg.profile_name.clone(),
                server_addr: cfg.server_addr.clone(),
                transport: cfg.transport.clone(),
                auth_token: cfg.auth_token.clone(),
                listen_addr: cfg.listen_addr.clone(),
                fake_lan_broadcast: cfg.fake_lan_broadcast,
            };
            if profile.auth_token.trim().is_empty() {
                if let Ok(Some(cred)) = self.load_credential(&id) {
                    profile.auth_token = cred.token;
                }
            }
            self.upsert_profile(&profile)?;
        }
        self.write_active_config_json(&cfg)?;
        Ok(cfg)
    }

    /// Gets a consolidated snapshot of the client configuration, profiles, and cumulative stats.
    pub fn get_client_config_snapshot(&self) -> ClientConfigResponse {
        let profiles = self.load_profiles().unwrap_or_default();
        let active_profile_id = self.load_active_profile_id().unwrap_or_default();
        let mut active_config = self.load_active_config().unwrap_or_default();
        if active_config.auth_token.trim().is_empty() {
            if let Some(id) = active_profile_id.as_deref() {
                if let Some(p) = profiles.iter().find(|item| item.id == id) {
                    active_config.auth_token = p.auth_token.clone();
                }
            }
        }
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
            device_id: self.load_or_create_device_id().unwrap_or_default(),
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

    // ========================================================================
    // Middleware Dynamic Configs Operations
    // ========================================================================

    /// Persists dynamic middleware configuration JSON for a named middleware.
    pub fn save_middleware_config(
        &self,
        name: &str,
        config: &std::collections::HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<()> {
        let json_str = serde_json::to_string(config)?;
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "INSERT INTO middleware_configs (name, config_json, updated_at)
             VALUES (?1, ?2, unixepoch())
             ON CONFLICT(name) DO UPDATE SET
                 config_json = excluded.config_json,
                 updated_at = excluded.updated_at",
            params![name.trim().to_ascii_lowercase(), json_str],
        )?;
        Ok(())
    }

    /// Loads the stored dynamic middleware configuration for a named middleware if any.
    pub fn load_middleware_config(
        &self,
        name: &str,
    ) -> anyhow::Result<Option<std::collections::HashMap<String, serde_json::Value>>> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt =
            conn.prepare("SELECT config_json FROM middleware_configs WHERE name = ?1")?;
        let mut rows = stmt.query(params![name.trim().to_ascii_lowercase()])?;
        if let Some(row) = rows.next()? {
            let s: String = row.get(0)?;
            let cfg = serde_json::from_str(&s)?;
            Ok(Some(cfg))
        } else {
            Ok(None)
        }
    }

    /// Loads all stored dynamic middleware configurations across all middlewares.
    pub fn load_all_middleware_configs(
        &self,
    ) -> anyhow::Result<
        std::collections::HashMap<String, std::collections::HashMap<String, serde_json::Value>>,
    > {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        let mut stmt = conn.prepare("SELECT name, config_json FROM middleware_configs")?;
        let rows = stmt.query_map([], |row| {
            let name: String = row.get(0)?;
            let s: String = row.get(1)?;
            Ok((name, s))
        })?;

        let mut out = std::collections::HashMap::new();
        for r in rows {
            let (name, s) = r?;
            if let Ok(cfg) = serde_json::from_str(&s) {
                out.insert(name, cfg);
            }
        }
        Ok(out)
    }

    /// Deletes stored dynamic configuration for a named middleware, resetting it to default.
    pub fn delete_middleware_config(&self, name: &str) -> anyhow::Result<()> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| anyhow::anyhow!("sqlite lock error: {e}"))?;
        conn.execute(
            "DELETE FROM middleware_configs WHERE name = ?1",
            params![name.trim().to_ascii_lowercase()],
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
            ..Default::default()
        };

        storage.save_active_profile_id("test-id-123").unwrap();
        storage
            .upsert_profile(&ClientProfile {
                id: "test-id-123".into(),
                name: cfg.profile_name.clone(),
                server_addr: cfg.server_addr.clone(),
                transport: cfg.transport.clone(),
                auth_token: cfg.auth_token.clone(),
                listen_addr: cfg.listen_addr.clone(),
                fake_lan_broadcast: cfg.fake_lan_broadcast,
            })
            .unwrap();
        storage.save_active_config(&cfg).unwrap();
        let loaded_cfg = storage.load_active_config().unwrap();
        assert_eq!(loaded_cfg.auth_token, "xyz");
        assert_eq!(loaded_cfg.server_addr, cfg.server_addr);
        assert_eq!(loaded_cfg.profile_name, cfg.profile_name);

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

    #[test]
    fn test_middleware_config_storage() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open sqlite");

        let mut config = std::collections::HashMap::new();
        config.insert("recompress-threshold".to_string(), serde_json::json!(512));
        config.insert(
            "discovery-targets".to_string(),
            serde_json::json!("127.0.0.1:4445"),
        );

        storage
            .save_middleware_config("minecraft", &config)
            .expect("save config");

        let loaded = storage
            .load_middleware_config("minecraft")
            .expect("load config")
            .expect("some config");
        assert_eq!(
            loaded.get("recompress-threshold").unwrap(),
            &serde_json::json!(512)
        );

        let all = storage.load_all_middleware_configs().expect("load all");
        assert!(all.contains_key("minecraft"));

        storage
            .delete_middleware_config("minecraft")
            .expect("delete config");
        let after_delete = storage
            .load_middleware_config("minecraft")
            .expect("load config");
        assert!(after_delete.is_none());

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_config_state_deserializes_with_missing_fields() {
        let cfg: ClientConfigState =
            serde_json::from_str(r#"{"server_addr":"relay.example:7000","auth_token":"tok"}"#)
                .unwrap();
        assert_eq!(cfg.server_addr, "relay.example:7000");
        assert_eq!(cfg.auth_token, "tok");
        assert_eq!(cfg.profile_name, "Default Realm");
        assert!(cfg.auto_connect);
        assert!(cfg.auto_connect_panel);
    }

    #[test]
    fn test_credential_roundtrip_and_empty_token_preserves_secret() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open sqlite");

        let profile = ClientProfile {
            id: "p-secret".into(),
            name: "Secret Realm".into(),
            server_addr: "relay.example:7000".into(),
            transport: "quic".into(),
            auth_token: "prism_cl_abc".into(),
            listen_addr: "127.0.0.1:25565".into(),
            fake_lan_broadcast: true,
        };
        storage.upsert_profile(&profile).unwrap();
        storage.save_active_profile_id("p-secret").unwrap();

        let loaded = storage.load_profiles().unwrap();
        assert_eq!(loaded[0].auth_token, "prism_cl_abc");

        let mut stripped = loaded[0].clone();
        stripped.auth_token.clear();
        storage.save_profiles(&[stripped]).unwrap();
        let reloaded = storage.load_profiles().unwrap();
        assert_eq!(reloaded[0].auth_token, "prism_cl_abc");

        let patch = ClientConfigPatch {
            transport: Some("kcp".into()),
            ..Default::default()
        };
        let cfg = storage
            .apply_config_patch(Some("p-secret"), &patch)
            .unwrap();
        assert_eq!(cfg.transport, "kcp");
        assert_eq!(cfg.auth_token, "prism_cl_abc");

        let snap = storage.get_client_config_snapshot();
        assert!(!snap.device_id.is_empty());
        assert!(snap.device_id.starts_with("prism_dev_"));
        assert_eq!(snap.active_config.auth_token, "prism_cl_abc");

        let other = ClientProfile {
            id: "p-other".into(),
            name: "Other".into(),
            server_addr: "other.example:7000".into(),
            transport: "tcp".into(),
            auth_token: "tok-other".into(),
            listen_addr: "127.0.0.1:25566".into(),
            fake_lan_broadcast: false,
        };
        storage.upsert_profile(&other).unwrap();
        let all = storage.load_profiles().unwrap();
        assert_eq!(all.len(), 2);

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_expired_credential_is_dropped() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open sqlite");
        storage
            .upsert_credential(&TunnelCredential {
                profile_id: "p-exp".into(),
                server_addr: "relay.example:7000".into(),
                token_id: "tok_old".into(),
                token_type: "oauth_pat".into(),
                user_id: "gh_1".into(),
                username: "alice".into(),
                issued_at: 1,
                expires_at: Some(1),
                token: "prism_cl_expired".into(),
            })
            .unwrap();
        assert!(storage.load_credential("p-exp").unwrap().is_none());
        let _ = std::fs::remove_file(path);
    }
}
