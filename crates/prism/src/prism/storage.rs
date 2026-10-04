use std::{
    collections::HashMap,
    path::Path,
    sync::Arc,
};

use anyhow::Context;
use redb::{Database, ReadableDatabase, ReadableTable, TableDefinition};
use serde::{Deserialize, Serialize};

use crate::prism::admin::ClientProfile;
use crate::prism::auth::{PersistedAuthState, TokenRecord, UserRecord};
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
    pub auto_check_update: bool,
    pub update_channel: String,
    pub autostart: bool,
    pub silent_autostart: bool,
    pub optimizer_enabled: bool,
    pub optimizer_zstd_level: i32,
    pub optimizer_adaptive_flush: bool,
    pub optimizer_flush_interval_ms: u64,
    pub optimizer_buffer_threshold: usize,
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
            auto_check_update: true,
            update_channel: "release".into(),
            autostart: false,
            silent_autostart: true,
            optimizer_enabled: true,
            optimizer_zstd_level: 3,
            optimizer_adaptive_flush: true,
            optimizer_flush_interval_ms: 20,
            optimizer_buffer_threshold: 64 * 1024,
        }
    }
}

/// Global client settings independent of individual tunnel profiles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default)]
pub struct ClientAppSettings {
    pub active_profile_id: Option<String>,
    pub device_id: String,
    pub auto_connect: bool,
    pub auto_connect_panel: bool,
    pub management_url: String,
    pub auto_check_update: bool,
    pub update_channel: String,
    pub autostart: bool,
    pub silent_autostart: bool,
    pub optimizer_enabled: bool,
    pub optimizer_zstd_level: i32,
    pub optimizer_adaptive_flush: bool,
    pub optimizer_flush_interval_ms: u64,
    pub optimizer_buffer_threshold: usize,
}

impl Default for ClientAppSettings {
    fn default() -> Self {
        Self {
            active_profile_id: None,
            device_id: String::new(),
            auto_connect: true,
            auto_connect_panel: true,
            management_url: String::new(),
            auto_check_update: true,
            update_channel: "release".into(),
            autostart: false,
            silent_autostart: true,
            optimizer_enabled: true,
            optimizer_zstd_level: 3,
            optimizer_adaptive_flush: true,
            optimizer_flush_interval_ms: 20,
            optimizer_buffer_threshold: 64 * 1024,
        }
    }
}

/// Partial update for `/client/config` and UniFFI `client_save_config`.
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
    #[serde(default)]
    pub auto_check_update: Option<bool>,
    #[serde(default)]
    pub update_channel: Option<String>,
    #[serde(default)]
    pub autostart: Option<bool>,
    #[serde(default)]
    pub silent_autostart: Option<bool>,
    #[serde(default)]
    pub optimizer_enabled: Option<bool>,
    #[serde(default)]
    pub optimizer_zstd_level: Option<i32>,
    #[serde(default)]
    pub optimizer_adaptive_flush: Option<bool>,
    #[serde(default)]
    pub optimizer_flush_interval_ms: Option<u64>,
    #[serde(default)]
    pub optimizer_buffer_threshold: Option<usize>,
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
            && self.auto_check_update.is_none()
            && self.update_channel.is_none()
            && self.autostart.is_none()
            && self.silent_autostart.is_none()
            && self.optimizer_enabled.is_none()
            && self.optimizer_zstd_level.is_none()
            && self.optimizer_adaptive_flush.is_none()
            && self.optimizer_flush_interval_ms.is_none()
            && self.optimizer_buffer_threshold.is_none()
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
            if !v.starts_with("***") {
                self.auth_token = v.clone();
            }
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
        if let Some(v) = patch.auto_check_update {
            self.auto_check_update = v;
        }
        if let Some(v) = patch.update_channel.as_ref().filter(|s| !s.trim().is_empty()) {
            self.update_channel = v.clone();
        }
        if let Some(v) = patch.autostart {
            self.autostart = v;
        }
        if let Some(v) = patch.silent_autostart {
            self.silent_autostart = v;
        }
        if let Some(v) = patch.optimizer_enabled {
            self.optimizer_enabled = v;
        }
        if let Some(v) = patch.optimizer_zstd_level {
            self.optimizer_zstd_level = v.clamp(1, 22);
        }
        if let Some(v) = patch.optimizer_adaptive_flush {
            self.optimizer_adaptive_flush = v;
        }
        if let Some(v) = patch.optimizer_flush_interval_ms {
            self.optimizer_flush_interval_ms = v.clamp(1, 1000);
        }
        if let Some(v) = patch.optimizer_buffer_threshold {
            self.optimizer_buffer_threshold = v.clamp(1024, 16 * 1024 * 1024);
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
// redb Table Definitions
// ============================================================================

pub const TABLE_PROFILES: TableDefinition<&str, &[u8]> = TableDefinition::new("profiles");
pub const TABLE_SETTINGS: TableDefinition<&str, &str> = TableDefinition::new("settings");
pub const TABLE_CUMULATIVE_STATS: TableDefinition<&str, &[u8]> = TableDefinition::new("cumulative_stats");
pub const TABLE_MIDDLEWARE_CONFIGS: TableDefinition<&str, &[u8]> = TableDefinition::new("middleware_configs");
pub const TABLE_AUTH_USERS: TableDefinition<&str, &[u8]> = TableDefinition::new("auth_users");
pub const TABLE_AUTH_TOKENS: TableDefinition<&str, &[u8]> = TableDefinition::new("auth_tokens");
pub const TABLE_AUTH_META: TableDefinition<&str, &str> = TableDefinition::new("auth_meta");

// ============================================================================
// Storage Engine
// ============================================================================

#[derive(Clone)]
pub struct StorageEngine {
    db: Arc<Database>,
}

impl std::fmt::Debug for StorageEngine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("StorageEngine").finish_non_exhaustive()
    }
}

impl StorageEngine {
    /// Opens or creates the redb database at the specified file path.
    pub fn open(path: &Path) -> anyhow::Result<Self> {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(0o700));
            }
        }

        let db = if path.exists() {
            Database::open(path)
                .with_context(|| format!("failed to open redb at {}", path.display()))?
        } else {
            Database::create(path)
                .with_context(|| format!("failed to create redb at {}", path.display()))?
        };

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600));
        }

        // Initialize all tables on startup
        let write_txn = db.begin_write()?;
        {
            let _ = write_txn.open_table(TABLE_PROFILES)?;
            let _ = write_txn.open_table(TABLE_SETTINGS)?;
            let _ = write_txn.open_table(TABLE_CUMULATIVE_STATS)?;
            let _ = write_txn.open_table(TABLE_MIDDLEWARE_CONFIGS)?;
            let _ = write_txn.open_table(TABLE_AUTH_USERS)?;
            let _ = write_txn.open_table(TABLE_AUTH_TOKENS)?;
            let _ = write_txn.open_table(TABLE_AUTH_META)?;
        }
        write_txn.commit()?;

        Ok(Self { db: Arc::new(db) })
    }

    pub fn db(&self) -> Arc<Database> {
        self.db.clone()
    }

    fn now_unix_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64
    }

    // ========================================================================
    // Device Identity
    // ========================================================================

    pub fn load_or_create_device_id(&self) -> anyhow::Result<String> {
        {
            let read_txn = self.db.begin_read()?;
            let table = read_txn.open_table(TABLE_SETTINGS)?;
            if let Some(guard) = table.get("device_id")? {
                let val = guard.value().trim();
                if !val.is_empty() {
                    return Ok(val.to_string());
                }
            }
        }

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

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SETTINGS)?;
            table.insert("device_id", id.as_str())?;
        }
        write_txn.commit()?;

        Ok(id)
    }

    // ========================================================================
    // Profiles
    // ========================================================================

    pub fn load_profiles(&self) -> anyhow::Result<Vec<ClientProfile>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_PROFILES)?;
        let mut out = Vec::new();
        for item in table.iter()? {
            let (_, v_guard) = item?;
            if let Ok(profile) = serde_json::from_slice::<ClientProfile>(v_guard.value()) {
                out.push(profile);
            }
        }
        Ok(out)
    }

    pub fn load_profile(&self, id: &str) -> anyhow::Result<Option<ClientProfile>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_PROFILES)?;
        if let Some(guard) = table.get(id)? {
            let profile = serde_json::from_slice::<ClientProfile>(guard.value())?;
            return Ok(Some(profile));
        }
        Ok(None)
    }

    pub fn save_profiles(&self, profiles: &[ClientProfile]) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PROFILES)?;
            let existing_keys: Vec<String> = table
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for k in existing_keys {
                table.remove(k.as_str())?;
            }
            for p in profiles {
                let bytes = serde_json::to_vec(p)?;
                table.insert(p.id.as_str(), bytes.as_slice())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn upsert_profile(&self, profile: &ClientProfile) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PROFILES)?;
            let bytes = serde_json::to_vec(profile)?;
            table.insert(profile.id.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn delete_profile(&self, id: &str) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_PROFILES)?;
            table.remove(id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ========================================================================
    // Active Profile & Config
    // ========================================================================

    pub fn load_active_profile_id(&self) -> anyhow::Result<Option<String>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_SETTINGS)?;
        if let Some(guard) = table.get("active_profile_id")? {
            let val = guard.value().trim();
            if !val.is_empty() {
                return Ok(Some(val.to_string()));
            }
        }
        Ok(None)
    }

    pub fn save_active_profile_id(&self, id: &str) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SETTINGS)?;
            table.insert("active_profile_id", id)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_app_settings(&self) -> anyhow::Result<ClientAppSettings> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_SETTINGS)?;
        if let Some(guard) = table.get("app_settings")? {
            if let Ok(settings) = serde_json::from_str::<ClientAppSettings>(guard.value()) {
                return Ok(settings);
            }
        }
        Ok(ClientAppSettings::default())
    }

    pub fn save_app_settings(&self, settings: &ClientAppSettings) -> anyhow::Result<()> {
        let json = serde_json::to_string(settings)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_SETTINGS)?;
            table.insert("app_settings", json.as_str())?;
            if let Some(ref pid) = settings.active_profile_id {
                table.insert("active_profile_id", pid.as_str())?;
            }
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_active_config(&self) -> anyhow::Result<ClientConfigState> {
        let active_id = self.load_active_profile_id()?;
        let profiles = self.load_profiles().unwrap_or_default();
        let target_profile = if let Some(ref aid) = active_id {
            profiles.iter().find(|p| &p.id == aid).cloned()
        } else {
            profiles.first().cloned()
        };

        let app_settings = self.load_app_settings().unwrap_or_default();

        let mut state = ClientConfigState::default();
        state.active_profile_id_apply(&app_settings);

        if let Some(p) = target_profile {
            state.profile_name = p.name;
            state.server_addr = p.server_addr;
            state.transport = p.transport;
            state.auth_token = p.auth_token;
            state.listen_addr = p.listen_addr;
            state.fake_lan_broadcast = p.fake_lan_broadcast;
        }

        Ok(state)
    }

    pub fn save_active_config(&self, config: &ClientConfigState) -> anyhow::Result<()> {
        let active_id = self
            .load_active_profile_id()?
            .unwrap_or_else(|| "default".to_string());

        let profile = ClientProfile {
            id: active_id.clone(),
            name: config.profile_name.clone(),
            server_addr: config.server_addr.clone(),
            transport: config.transport.clone(),
            auth_token: config.auth_token.clone(),
            listen_addr: config.listen_addr.clone(),
            fake_lan_broadcast: config.fake_lan_broadcast,
        };
        self.upsert_profile(&profile)?;
        self.save_active_profile_id(&active_id)?;

        let mut settings = self.load_app_settings().unwrap_or_default();
        settings.active_profile_id = Some(active_id);
        settings.auto_connect = config.auto_connect;
        settings.auto_connect_panel = config.auto_connect_panel;
        settings.management_url = config.management_url.clone();
        settings.auto_check_update = config.auto_check_update;
        settings.update_channel = config.update_channel.clone();
        settings.autostart = config.autostart;
        settings.silent_autostart = config.silent_autostart;
        settings.optimizer_enabled = config.optimizer_enabled;
        settings.optimizer_zstd_level = config.optimizer_zstd_level;
        settings.optimizer_adaptive_flush = config.optimizer_adaptive_flush;
        settings.optimizer_flush_interval_ms = config.optimizer_flush_interval_ms;
        settings.optimizer_buffer_threshold = config.optimizer_buffer_threshold;
        self.save_app_settings(&settings)?;

        Ok(())
    }

    pub fn apply_config_patch(
        &self,
        profile_id: Option<&str>,
        patch: &ClientConfigPatch,
    ) -> anyhow::Result<()> {
        if let Some(pid) = profile_id {
            self.save_active_profile_id(pid)?;
        }
        let mut active = self.load_active_config()?;
        active.apply_patch(patch);
        self.save_active_config(&active)?;
        Ok(())
    }

    pub fn get_client_config_snapshot(&self) -> ClientConfigResponse {
        let active_profile_id = self.load_active_profile_id().ok().flatten();
        let active_config = self.load_active_config().unwrap_or_default();
        let profiles = self.load_profiles().unwrap_or_default();
        let cum = self.load_cumulative_stats().unwrap_or_default();
        let device_id = self
            .load_or_create_device_id()
            .unwrap_or_else(|_| "prism_dev_unknown".to_string());

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
            device_id,
        }
    }

    // ========================================================================
    // Legacy Credential Compatibility Shims
    // ========================================================================

    pub fn load_credential(&self, profile_id: &str) -> anyhow::Result<Option<TunnelCredential>> {
        if let Ok(Some(p)) = self.load_profile(profile_id) {
            if !p.auth_token.is_empty() {
                return Ok(Some(TunnelCredential {
                    profile_id: p.id,
                    server_addr: p.server_addr,
                    token: p.auth_token,
                    ..Default::default()
                }));
            }
        }
        Ok(None)
    }

    pub fn upsert_credential(&self, cred: &TunnelCredential) -> anyhow::Result<()> {
        if let Ok(Some(mut p)) = self.load_profile(&cred.profile_id) {
            p.auth_token = cred.token.clone();
            self.upsert_profile(&p)?;
        }
        Ok(())
    }

    pub fn delete_credential(&self, profile_id: &str) -> anyhow::Result<()> {
        if let Ok(Some(mut p)) = self.load_profile(profile_id) {
            p.auth_token.clear();
            self.upsert_profile(&p)?;
        }
        Ok(())
    }

    // ========================================================================
    // Cumulative Statistics
    // ========================================================================

    pub fn load_cumulative_stats(&self) -> anyhow::Result<ClientCumulativeStats> {
        self.load_scoped_stats("default")
    }

    pub fn load_scoped_stats(&self, scope: &str) -> anyhow::Result<ClientCumulativeStats> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_CUMULATIVE_STATS)?;
        if let Some(guard) = table.get(scope)? {
            if let Ok(stats) = serde_json::from_slice::<ClientCumulativeStats>(guard.value()) {
                return Ok(stats);
            }
        }
        Ok(ClientCumulativeStats::default())
    }

    pub fn record_session_stats(
        &self,
        delta: &OptimizerStatsSnapshot,
    ) -> anyhow::Result<ClientCumulativeStats> {
        self.record_scoped_session_stats("default", delta)
    }

    pub fn record_scoped_session_stats(
        &self,
        scope: &str,
        delta: &OptimizerStatsSnapshot,
    ) -> anyhow::Result<ClientCumulativeStats> {
        let mut cur = self.load_scoped_stats(scope).unwrap_or_default();
        cur.raw_bytes = cur.raw_bytes.saturating_add(delta.raw_bytes);
        cur.wire_bytes = cur.wire_bytes.saturating_add(delta.wire_bytes);
        cur.saved_bytes = cur.saved_bytes.saturating_add(delta.saved_bytes);
        cur.sessions_count = cur.sessions_count.saturating_add(1);
        cur.last_session_at = Self::now_unix_ms();
        if cur.raw_bytes > 0 {
            let saved = cur.raw_bytes.saturating_sub(cur.wire_bytes) as f64;
            cur.saved_ratio = (saved / cur.raw_bytes as f64).clamp(0.0, 1.0);
        } else {
            cur.saved_ratio = 0.0;
        }

        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CUMULATIVE_STATS)?;
            let bytes = serde_json::to_vec(&cur)?;
            table.insert(scope, bytes.as_slice())?;
        }
        write_txn.commit()?;

        Ok(cur)
    }

    pub fn reset_cumulative_stats(&self) -> anyhow::Result<()> {
        self.reset_scoped_stats("default")
    }

    pub fn reset_scoped_stats(&self, scope: &str) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_CUMULATIVE_STATS)?;
            table.remove(scope)?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ========================================================================
    // Middleware Configs
    // ========================================================================

    pub fn save_middleware_config(
        &self,
        name: &str,
        config: &HashMap<String, serde_json::Value>,
    ) -> anyhow::Result<()> {
        let key = name.trim().to_ascii_lowercase();
        let bytes = serde_json::to_vec(config)?;
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_MIDDLEWARE_CONFIGS)?;
            table.insert(key.as_str(), bytes.as_slice())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    pub fn load_middleware_config(
        &self,
        name: &str,
    ) -> anyhow::Result<Option<HashMap<String, serde_json::Value>>> {
        let key = name.trim().to_ascii_lowercase();
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_MIDDLEWARE_CONFIGS)?;
        if let Some(guard) = table.get(key.as_str())? {
            let map = serde_json::from_slice::<HashMap<String, serde_json::Value>>(guard.value())?;
            return Ok(Some(map));
        }
        Ok(None)
    }

    pub fn load_all_middleware_configs(
        &self,
    ) -> anyhow::Result<HashMap<String, HashMap<String, serde_json::Value>>> {
        let read_txn = self.db.begin_read()?;
        let table = read_txn.open_table(TABLE_MIDDLEWARE_CONFIGS)?;
        let mut out = HashMap::new();
        for item in table.iter()? {
            let (k_guard, v_guard) = item?;
            if let Ok(map) = serde_json::from_slice::<HashMap<String, serde_json::Value>>(v_guard.value()) {
                out.insert(k_guard.value().to_string(), map);
            }
        }
        Ok(out)
    }

    pub fn delete_middleware_config(&self, name: &str) -> anyhow::Result<()> {
        let key = name.trim().to_ascii_lowercase();
        let write_txn = self.db.begin_write()?;
        {
            let mut table = write_txn.open_table(TABLE_MIDDLEWARE_CONFIGS)?;
            table.remove(key.as_str())?;
        }
        write_txn.commit()?;
        Ok(())
    }

    // ========================================================================
    // Server Auth Persistence
    // ========================================================================

    pub fn load_auth_state(&self) -> anyhow::Result<Option<PersistedAuthState>> {
        let read_txn = self.db.begin_read()?;
        let meta_tbl = read_txn.open_table(TABLE_AUTH_META)?;
        let initialized = meta_tbl
            .get("initialized")?
            .map(|v| v.value() == "1")
            .unwrap_or(false);

        if !initialized {
            return Ok(None);
        }

        let mut state = PersistedAuthState::default();
        let users_tbl = read_txn.open_table(TABLE_AUTH_USERS)?;
        for item in users_tbl.iter()? {
            let (k, v) = item?;
            if let Ok(user) = serde_json::from_slice::<UserRecord>(v.value()) {
                state.users.insert(k.value().to_string(), user);
            }
        }

        let tokens_tbl = read_txn.open_table(TABLE_AUTH_TOKENS)?;
        for item in tokens_tbl.iter()? {
            let (k, v) = item?;
            if let Ok(token) = serde_json::from_slice::<TokenRecord>(v.value()) {
                state.tokens.insert(k.value().to_string(), token);
            }
        }

        Ok(Some(state))
    }

    pub fn save_auth_state(&self, state: &PersistedAuthState) -> anyhow::Result<()> {
        let write_txn = self.db.begin_write()?;
        {
            let mut users_tbl = write_txn.open_table(TABLE_AUTH_USERS)?;
            let user_keys: Vec<String> = users_tbl
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for k in user_keys {
                users_tbl.remove(k.as_str())?;
            }
            for (id, user) in &state.users {
                let json = serde_json::to_vec(user)?;
                users_tbl.insert(id.as_str(), json.as_slice())?;
            }
        }
        {
            let mut tokens_tbl = write_txn.open_table(TABLE_AUTH_TOKENS)?;
            let token_keys: Vec<String> = tokens_tbl
                .iter()?
                .filter_map(|r| r.ok().map(|(k, _)| k.value().to_string()))
                .collect();
            for k in token_keys {
                tokens_tbl.remove(k.as_str())?;
            }
            for (hash, token) in &state.tokens {
                let json = serde_json::to_vec(token)?;
                tokens_tbl.insert(hash.as_str(), json.as_slice())?;
            }
        }
        {
            let mut meta_tbl = write_txn.open_table(TABLE_AUTH_META)?;
            meta_tbl.insert("initialized", "1")?;
        }
        write_txn.commit()?;
        Ok(())
    }
}

trait ClientConfigStateExt {
    fn active_profile_id_apply(&mut self, s: &ClientAppSettings);
}

impl ClientConfigStateExt for ClientConfigState {
    fn active_profile_id_apply(&mut self, s: &ClientAppSettings) {
        self.auto_connect = s.auto_connect;
        self.auto_connect_panel = s.auto_connect_panel;
        self.management_url = s.management_url.clone();
        self.auto_check_update = s.auto_check_update;
        self.update_channel = s.update_channel.clone();
        self.autostart = s.autostart;
        self.silent_autostart = s.silent_autostart;
        self.optimizer_enabled = s.optimizer_enabled;
        self.optimizer_zstd_level = s.optimizer_zstd_level;
        self.optimizer_adaptive_flush = s.optimizer_adaptive_flush;
        self.optimizer_flush_interval_ms = s.optimizer_flush_interval_ms;
        self.optimizer_buffer_threshold = s.optimizer_buffer_threshold;
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
        std::env::temp_dir().join(format!("prism_test_{n}.redb"))
    }

    #[test]
    fn test_storage_open_and_profiles_roundtrip() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open redb");

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
                auth_token: "tok2".into(),
                listen_addr: "127.0.0.1:25566".into(),
                fake_lan_broadcast: false,
            },
        ];

        storage.save_profiles(&profiles).unwrap();
        let loaded = storage.load_profiles().unwrap();
        assert_eq!(loaded.len(), 2);

        let p1 = storage.load_profile("p1").unwrap().unwrap();
        assert_eq!(p1.name, "Server One");
        assert_eq!(p1.auth_token, "tok1");

        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn test_device_id_stable() {
        let path = temp_db_path();
        let storage = StorageEngine::open(&path).expect("open redb");

        let dev1 = storage.load_or_create_device_id().unwrap();
        let dev2 = storage.load_or_create_device_id().unwrap();
        assert_eq!(dev1, dev2);
        assert!(dev1.starts_with("prism_dev_"));

        let _ = std::fs::remove_file(path);
    }
}
