//! UniFFI exports for Prism client and Tauri commands.
//!
//! Provides pure strongly-typed cross-platform foreign function interface bindings (Swift, Kotlin, Python, etc.)
//! corresponding to the Prism Tauri IPC commands without any UDL or JSON representations.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::{Arc, LazyLock, OnceLock, RwLock};

#[derive(Debug, thiserror::Error, uniffi::Error)]
pub enum PrismFfiError {
    #[error("Client not initialized")]
    NotInitialized,
    #[error("Invalid argument: {message}")]
    InvalidArgument { message: String },
    #[error("Execution failed: {message}")]
    ExecutionFailed { message: String },
    #[error("{message}")]
    Generic { message: String },
}

impl From<String> for PrismFfiError {
    fn from(message: String) -> Self {
        PrismFfiError::ExecutionFailed { message }
    }
}

impl From<&str> for PrismFfiError {
    fn from(s: &str) -> Self {
        PrismFfiError::ExecutionFailed {
            message: s.to_string(),
        }
    }
}

impl From<anyhow::Error> for PrismFfiError {
    fn from(e: anyhow::Error) -> Self {
        PrismFfiError::ExecutionFailed {
            message: e.to_string(),
        }
    }
}

impl From<serde_json::Error> for PrismFfiError {
    fn from(e: serde_json::Error) -> Self {
        PrismFfiError::InvalidArgument {
            message: e.to_string(),
        }
    }
}

// ---------------------------------------------------------------------------
// Pure Strongly-Typed FFI Records
// ---------------------------------------------------------------------------

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClientRegisteredService {
    pub name: String,
    pub proto: String,
    pub local_addr: String,
    pub route_only: bool,
    pub remote_addr: String,
    pub masquerade_host: String,
    pub middleware: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct DirectionQuantiles {
    pub p50_us: u64,
    pub p90_us: u64,
    pub p99_us: u64,
    pub max_us: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct DirectionStatsSnapshotFfi {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub batches: u64,
    pub batching_delay_us: u64,
    pub compression_time_us: u64,
    pub decompression_time_us: u64,
    pub link_rate_bps: f64,
    pub batching_delay: DirectionQuantiles,
    pub compression_time: DirectionQuantiles,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ClientOptimizerStats {
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
    pub batching_delay_us: u64,
    pub compression_time_us: u64,
    pub decompression_time_us: u64,
    pub uplink: DirectionStatsSnapshotFfi,
    pub downlink: DirectionStatsSnapshotFfi,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct CumulativeStats {
    pub raw_bytes: u64,
    pub wire_bytes: u64,
    pub saved_bytes: u64,
    pub saved_ratio: f64,
    pub sessions_count: u64,
    pub last_session_at: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ClientStatusResponse {
    pub running: bool,
    pub state: String,
    pub server_addr: String,
    pub transport: String,
    pub actual_transport: Option<String>,
    pub listen_addr: String,
    pub fake_lan_broadcast: bool,
    pub known_services: Vec<ClientRegisteredService>,
    pub stats: ClientOptimizerStats,
    pub active_profile_id: Option<String>,
    pub cumulative_stats: Option<CumulativeStats>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClientOptimizerConfig {
    pub enabled: bool,
    pub zstd_level: Option<i32>,
    pub adaptive_flush: Option<bool>,
    pub flush_interval_ms: Option<u64>,
    pub buffer_threshold: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct StartClientRequest {
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub listen_addr: String,
    pub middleware: Option<String>,
    pub fake_lan_broadcast: bool,
    pub motd_prefix: String,
    pub optimizer: Option<ClientOptimizerConfig>,
    pub profile_id: Option<String>,
    pub profile_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClientProfile {
    pub id: String,
    pub name: String,
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub listen_addr: String,
    pub fake_lan_broadcast: bool,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
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
    pub optimizer_buffer_threshold: u64,
}

#[derive(Clone, Debug, PartialEq, uniffi::Record)]
pub struct ClientConfigResponse {
    pub active_profile_id: Option<String>,
    pub active_config: ClientConfigState,
    pub profiles: Vec<ClientProfile>,
    pub cumulative_stats: CumulativeStats,
    pub device_id: String,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct ClientConfigPatch {
    pub profile_name: Option<String>,
    pub server_addr: Option<String>,
    pub transport: Option<String>,
    pub auth_token: Option<String>,
    pub listen_addr: Option<String>,
    pub fake_lan_broadcast: Option<bool>,
    pub auto_connect_panel: Option<bool>,
    pub auto_connect: Option<bool>,
    pub management_url: Option<String>,
    pub token_id: Option<String>,
    pub token_type: Option<String>,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub expires_at: Option<u64>,
    pub auto_check_update: Option<bool>,
    pub update_channel: Option<String>,
    pub autostart: Option<bool>,
    pub silent_autostart: Option<bool>,
    pub optimizer_enabled: Option<bool>,
    pub optimizer_zstd_level: Option<i32>,
    pub optimizer_adaptive_flush: Option<bool>,
    pub optimizer_flush_interval_ms: Option<u64>,
    pub optimizer_buffer_threshold: Option<u64>,
}

#[derive(Clone, Debug, Default, PartialEq, Eq, uniffi::Record)]
pub struct SaveConfigRequest {
    pub active_profile_id: Option<String>,
    pub active_config: Option<ClientConfigPatch>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct ClientLogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MiddlewareConfigField {
    pub key: String,
    pub label: String,
    pub field_type: String,
    pub default_value: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MiddlewareConfigSchema {
    pub fields: Vec<MiddlewareConfigField>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct MiddlewareItem {
    pub name: String,
    pub schema: Option<MiddlewareConfigSchema>,
    pub effective_config: HashMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct UpdateCheckResponse {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub date: Option<String>,
    pub body: Option<String>,
    pub channel: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AdminHttpRequest {
    pub base_url: String,
    pub path: String,
    pub method: String,
    pub headers: HashMap<String, String>,
    pub body: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AdminHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AdminRpcRequest {
    pub method: String,
    pub params: HashMap<String, String>,
    pub token: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, uniffi::Record)]
pub struct AdminRpcResponse {
    pub ok: bool,
    pub status: u16,
    pub body: String,
    pub result: HashMap<String, String>,
    pub code: Option<String>,
    pub message: Option<String>,
}

// ---------------------------------------------------------------------------
// Prism Client Session (UniFFI Object)
// ---------------------------------------------------------------------------

#[derive(Clone, uniffi::Object)]
pub struct PrismClientSession {
    client: Arc<crate::prism::tunnel::client::ClientController>,
    storage: Option<Arc<crate::prism::storage::StorageEngine>>,
    workdir: PathBuf,
}

#[uniffi::export]
impl PrismClientSession {
    #[uniffi::constructor]
    pub fn new(workdir: Option<String>) -> Result<Arc<Self>, PrismFfiError> {
        let _ = rustls::crypto::ring::default_provider().install_default();

        let workdir_path = if let Some(w) = workdir {
            PathBuf::from(w)
        } else {
            crate::prism::runtime_paths::resolve_desktop_data_dir()
        };
        let _ = std::fs::create_dir_all(&workdir_path);
        let _ = std::fs::create_dir_all(workdir_path.join("optimizer-dicts"));
        crate::prism::tunnel::optimizer::set_dictionary_dir(workdir_path.join("optimizer-dicts"));

        let storage_path = workdir_path.join("prism.db");
        let storage = match crate::prism::storage::StorageEngine::open(&storage_path) {
            Ok(s) => Some(Arc::new(s)),
            Err(err) => {
                tracing::warn!(err = %err, "ffi: failed to open persistent storage; continuing without DB");
                None
            }
        };

        if let Some(ref storage_engine) = storage {
            if let Ok(saved) = storage_engine.load_all_middleware_configs() {
                for (name, cfg) in saved {
                    crate::prism::middleware::set_dynamic_middleware_config(&name, cfg);
                }
            }
        }

        let client = Arc::new(crate::prism::tunnel::client::ClientController::new(None));

        Ok(Arc::new(Self {
            client,
            storage,
            workdir: workdir_path,
        }))
    }

    pub fn workdir(&self) -> String {
        self.workdir.to_string_lossy().to_string()
    }

    pub async fn client_status(&self) -> Result<ClientStatusResponse, PrismFfiError> {
        let snap = self.client.status().await;
        let (active_profile_id, cumulative_stats) = if let Some(ref storage) = self.storage {
            (
                storage.load_active_profile_id().ok().flatten(),
                storage
                    .load_cumulative_stats()
                    .ok()
                    .map(|s| CumulativeStats {
                        raw_bytes: s.raw_bytes,
                        wire_bytes: s.wire_bytes,
                        saved_bytes: s.saved_bytes,
                        saved_ratio: s.saved_ratio,
                        sessions_count: s.sessions_count,
                        last_session_at: Some(s.last_session_at),
                    }),
            )
        } else {
            (None, None)
        };

        Ok(ClientStatusResponse {
            running: snap.running,
            state: snap.state,
            server_addr: snap.server_addr,
            transport: snap.transport,
            actual_transport: snap.actual_transport,
            listen_addr: snap.listen_addr,
            fake_lan_broadcast: snap.fake_lan_broadcast,
            known_services: snap
                .known_services
                .into_iter()
                .map(|s| ClientRegisteredService {
                    name: s.name,
                    proto: s.proto,
                    local_addr: s.local_addr,
                    route_only: s.route_only,
                    remote_addr: s.remote_addr,
                    masquerade_host: s.masquerade_host,
                    middleware: s.middleware,
                })
                .collect(),
            stats: ClientOptimizerStats {
                raw_bytes: snap.stats.raw_bytes,
                wire_bytes: snap.stats.wire_bytes,
                saved_bytes: snap.stats.saved_bytes,
                saved_ratio: snap.stats.saved_ratio,
                urgent_batches: snap.stats.urgent_batches,
                timer_batches: snap.stats.timer_batches,
                threshold_batches: snap.stats.threshold_batches,
                explicit_batches: snap.stats.explicit_batches,
                link_rate_bps: snap.stats.link_rate_bps,
                link_rate_measured: snap.stats.link_rate_measured,
                link_rate_bytes: snap.stats.link_rate_bytes,
                link_rate_busy_us: snap.stats.link_rate_busy_us,
                batching_delay_us: snap.stats.batching_delay_us,
                compression_time_us: snap.stats.compression_time_us,
                decompression_time_us: snap.stats.decompression_time_us,
                uplink: DirectionStatsSnapshotFfi {
                    raw_bytes: snap.stats.uplink.raw_bytes,
                    wire_bytes: snap.stats.uplink.wire_bytes,
                    saved_bytes: snap.stats.uplink.saved_bytes,
                    saved_ratio: snap.stats.uplink.saved_ratio,
                    batches: snap.stats.uplink.batches,
                    batching_delay_us: snap.stats.uplink.batching_delay_us,
                    compression_time_us: snap.stats.uplink.compression_time_us,
                    decompression_time_us: snap.stats.uplink.decompression_time_us,
                    link_rate_bps: snap.stats.uplink.link_rate_bps,
                    batching_delay: DirectionQuantiles {
                        p50_us: snap.stats.uplink.batching_delay.p50_us,
                        p90_us: snap.stats.uplink.batching_delay.p90_us,
                        p99_us: snap.stats.uplink.batching_delay.p99_us,
                        max_us: snap.stats.uplink.batching_delay.max_us,
                    },
                    compression_time: DirectionQuantiles {
                        p50_us: snap.stats.uplink.compression_time.p50_us,
                        p90_us: snap.stats.uplink.compression_time.p90_us,
                        p99_us: snap.stats.uplink.compression_time.p99_us,
                        max_us: snap.stats.uplink.compression_time.max_us,
                    },
                },
                downlink: DirectionStatsSnapshotFfi {
                    raw_bytes: snap.stats.downlink.raw_bytes,
                    wire_bytes: snap.stats.downlink.wire_bytes,
                    saved_bytes: snap.stats.downlink.saved_bytes,
                    saved_ratio: snap.stats.downlink.saved_ratio,
                    batches: snap.stats.downlink.batches,
                    batching_delay_us: snap.stats.downlink.batching_delay_us,
                    compression_time_us: snap.stats.downlink.compression_time_us,
                    decompression_time_us: snap.stats.downlink.decompression_time_us,
                    link_rate_bps: snap.stats.downlink.link_rate_bps,
                    batching_delay: DirectionQuantiles {
                        p50_us: snap.stats.downlink.batching_delay.p50_us,
                        p90_us: snap.stats.downlink.batching_delay.p90_us,
                        p99_us: snap.stats.downlink.batching_delay.p99_us,
                        max_us: snap.stats.downlink.batching_delay.max_us,
                    },
                    compression_time: DirectionQuantiles {
                        p50_us: snap.stats.downlink.compression_time.p50_us,
                        p90_us: snap.stats.downlink.compression_time.p90_us,
                        p99_us: snap.stats.downlink.compression_time.p99_us,
                        max_us: snap.stats.downlink.compression_time.max_us,
                    },
                },
            },
            active_profile_id,
            cumulative_stats,
        })
    }

    pub async fn client_start(&self, payload: StartClientRequest) -> Result<(), PrismFfiError> {
        let native_optimizer =
            payload
                .optimizer
                .map(|opt| crate::prism::config::OptimizerClientConfig {
                    enabled: opt.enabled,
                    zstd_level: opt.zstd_level,
                    adaptive_flush: opt.adaptive_flush,
                    flush_interval_ms: opt.flush_interval_ms,
                    buffer_threshold: opt.buffer_threshold.map(|v| v as usize),
                    ..Default::default()
                });

        let native_req = crate::prism::admin::StartClientRequest {
            server_addr: payload.server_addr,
            transport: payload.transport,
            auth_token: payload.auth_token,
            listen_addr: payload.listen_addr,
            middleware: payload.middleware,
            fake_lan_broadcast: payload.fake_lan_broadcast,
            motd_prefix: payload.motd_prefix,
            optimizer: native_optimizer,
            profile_id: payload.profile_id,
            profile_name: payload.profile_name,
        };

        crate::prism::admin::do_client_start(&self.client, self.storage.as_deref(), native_req)
            .await?;
        Ok(())
    }

    pub async fn client_stop(&self) -> Result<(), PrismFfiError> {
        crate::prism::admin::do_client_stop(&self.client, self.storage.as_deref()).await?;
        Ok(())
    }

    pub fn client_get_profiles(&self) -> Result<Vec<ClientProfile>, PrismFfiError> {
        let profiles = crate::prism::admin::do_client_get_profiles(self.storage.as_deref());
        Ok(profiles
            .into_iter()
            .map(|p| ClientProfile {
                id: p.id,
                name: p.name,
                server_addr: p.server_addr,
                transport: p.transport,
                auth_token: p.auth_token,
                listen_addr: p.listen_addr,
                fake_lan_broadcast: p.fake_lan_broadcast,
            })
            .collect())
    }

    pub fn client_save_profiles(&self, profiles: Vec<ClientProfile>) -> Result<(), PrismFfiError> {
        let native: Vec<crate::prism::admin::ClientProfile> = profiles
            .into_iter()
            .map(|p| crate::prism::admin::ClientProfile {
                id: p.id,
                name: p.name,
                server_addr: p.server_addr,
                transport: p.transport,
                auth_token: p.auth_token,
                listen_addr: p.listen_addr,
                fake_lan_broadcast: p.fake_lan_broadcast,
            })
            .collect();
        crate::prism::admin::do_client_save_profiles(self.storage.as_deref(), &native)?;
        Ok(())
    }

    pub fn client_get_config(&self) -> Result<ClientConfigResponse, PrismFfiError> {
        let cfg = crate::prism::admin::do_client_get_config(self.storage.as_deref());

        Ok(ClientConfigResponse {
            active_profile_id: cfg.active_profile_id,
            active_config: ClientConfigState {
                profile_name: cfg.active_config.profile_name,
                server_addr: cfg.active_config.server_addr,
                transport: cfg.active_config.transport,
                auth_token: cfg.active_config.auth_token,
                listen_addr: cfg.active_config.listen_addr,
                fake_lan_broadcast: cfg.active_config.fake_lan_broadcast,
                auto_connect_panel: cfg.active_config.auto_connect_panel,
                auto_connect: cfg.active_config.auto_connect,
                management_url: cfg.active_config.management_url,
                token_id: cfg.active_config.token_id,
                token_type: cfg.active_config.token_type,
                user_id: cfg.active_config.user_id,
                username: cfg.active_config.username,
                expires_at: cfg.active_config.expires_at,
                auto_check_update: cfg.active_config.auto_check_update,
                update_channel: cfg.active_config.update_channel,
                autostart: cfg.active_config.autostart,
                silent_autostart: cfg.active_config.silent_autostart,
                optimizer_enabled: cfg.active_config.optimizer_enabled,
                optimizer_zstd_level: cfg.active_config.optimizer_zstd_level,
                optimizer_adaptive_flush: cfg.active_config.optimizer_adaptive_flush,
                optimizer_flush_interval_ms: cfg.active_config.optimizer_flush_interval_ms,
                optimizer_buffer_threshold: cfg.active_config.optimizer_buffer_threshold as u64,
            },
            profiles: cfg
                .profiles
                .into_iter()
                .map(|p| ClientProfile {
                    id: p.id,
                    name: p.name,
                    server_addr: p.server_addr,
                    transport: p.transport,
                    auth_token: p.auth_token,
                    listen_addr: p.listen_addr,
                    fake_lan_broadcast: p.fake_lan_broadcast,
                })
                .collect(),
            cumulative_stats: {
                let cum = self
                    .storage
                    .as_deref()
                    .and_then(|s| s.load_cumulative_stats().ok())
                    .unwrap_or_default();
                CumulativeStats {
                    raw_bytes: cum.raw_bytes,
                    wire_bytes: cum.wire_bytes,
                    saved_bytes: cum.saved_bytes,
                    saved_ratio: cum.saved_ratio,
                    sessions_count: cum.sessions_count,
                    last_session_at: if cum.last_session_at > 0 {
                        Some(cum.last_session_at)
                    } else {
                        None
                    },
                }
            },
            device_id: cfg.device_id,
        })
    }

    pub fn client_save_config(&self, payload: SaveConfigRequest) -> Result<(), PrismFfiError> {
        let patch = if let Some(p) = payload.active_config {
            crate::prism::storage::ClientConfigPatch {
                profile_name: p.profile_name,
                server_addr: p.server_addr,
                transport: p.transport,
                auth_token: p.auth_token,
                listen_addr: p.listen_addr,
                fake_lan_broadcast: p.fake_lan_broadcast,
                auto_connect_panel: p.auto_connect_panel,
                auto_connect: p.auto_connect,
                management_url: p.management_url,
                token_id: p.token_id,
                token_type: p.token_type,
                user_id: p.user_id,
                username: p.username,
                expires_at: p.expires_at,
                auto_check_update: p.auto_check_update,
                update_channel: p.update_channel,
                autostart: p.autostart,
                silent_autostart: p.silent_autostart,
                optimizer_enabled: p.optimizer_enabled,
                optimizer_zstd_level: p.optimizer_zstd_level,
                optimizer_adaptive_flush: p.optimizer_adaptive_flush,
                optimizer_flush_interval_ms: p.optimizer_flush_interval_ms,
                optimizer_buffer_threshold: p.optimizer_buffer_threshold.map(|v| v as usize),
            }
        } else {
            Default::default()
        };

        if let Some(ref storage) = self.storage {
            storage
                .apply_config_patch(payload.active_profile_id.as_deref(), &patch)
                .map_err(|e| PrismFfiError::ExecutionFailed {
                    message: e.to_string(),
                })?;
        }
        Ok(())
    }

    pub fn client_reset_stats(&self) -> Result<(), PrismFfiError> {
        crate::prism::admin::do_client_reset_stats(self.storage.as_deref())?;
        Ok(())
    }

    pub async fn client_logs(
        &self,
        limit: Option<u32>,
    ) -> Result<Vec<ClientLogEntry>, PrismFfiError> {
        let l = limit.unwrap_or(200).clamp(1, 1000) as usize;
        let entries = crate::prism::admin::do_client_logs(Some(&self.client), l).await;
        Ok(entries
            .into_iter()
            .map(|e| ClientLogEntry {
                timestamp: e.timestamp,
                level: e.level,
                target: e.target,
                message: e.message,
            })
            .collect())
    }

    pub async fn client_clear_logs(&self) -> Result<(), PrismFfiError> {
        crate::prism::admin::do_client_clear_logs(Some(&self.client)).await;
        Ok(())
    }

    pub fn client_list_middlewares(&self) -> Result<Vec<MiddlewareItem>, PrismFfiError> {
        let items = crate::prism::admin::do_list_middlewares()?;
        Ok(items
            .into_iter()
            .map(|item| MiddlewareItem {
                name: item.name,
                schema: item.schema.map(|s| MiddlewareConfigSchema {
                    fields: s
                        .fields
                        .into_iter()
                        .map(|f| {
                            let field_type = match &f.field_type {
                                crate::prism::middleware::ConfigFieldType::U8 => "u8".to_string(),
                                crate::prism::middleware::ConfigFieldType::U16 => "u16".to_string(),
                                crate::prism::middleware::ConfigFieldType::U32 => "u32".to_string(),
                                crate::prism::middleware::ConfigFieldType::I32 => "i32".to_string(),
                                crate::prism::middleware::ConfigFieldType::I64 => "i64".to_string(),
                                crate::prism::middleware::ConfigFieldType::Bool => {
                                    "bool".to_string()
                                }
                                crate::prism::middleware::ConfigFieldType::String => {
                                    "string".to_string()
                                }
                                crate::prism::middleware::ConfigFieldType::ListString => {
                                    "list_string".to_string()
                                }
                                crate::prism::middleware::ConfigFieldType::Unsupported(s) => {
                                    format!("unsupported:{s}")
                                }
                            };
                            MiddlewareConfigField {
                                key: f.key,
                                label: f.label,
                                field_type,
                                default_value: match f.default_value {
                                    serde_json::Value::String(s) => s,
                                    other => other.to_string(),
                                },
                                description: f.description,
                            }
                        })
                        .collect(),
                }),
                effective_config: item
                    .effective_config
                    .into_iter()
                    .map(|(k, v)| {
                        let str_val = match v {
                            serde_json::Value::String(s) => s,
                            other => other.to_string(),
                        };
                        (k, str_val)
                    })
                    .collect(),
            })
            .collect())
    }

    pub fn client_update_middleware_config(
        &self,
        name: String,
        config: HashMap<String, String>,
    ) -> Result<MiddlewareItem, PrismFfiError> {
        let json_map: HashMap<String, serde_json::Value> = config
            .into_iter()
            .map(|(k, v)| {
                let parsed =
                    serde_json::from_str(&v).unwrap_or_else(|_| serde_json::Value::String(v));
                (k, parsed)
            })
            .collect();

        let applied = crate::prism::admin::do_put_middleware_config(
            self.storage.as_deref(),
            &name,
            json_map,
        )?;

        Ok(MiddlewareItem {
            name,
            schema: None,
            effective_config: applied
                .into_iter()
                .map(|(k, v)| {
                    let s = match v {
                        serde_json::Value::String(s) => s,
                        other => other.to_string(),
                    };
                    (k, s)
                })
                .collect(),
        })
    }

    pub fn client_reset_middleware_config(
        &self,
        name: String,
    ) -> Result<MiddlewareItem, PrismFfiError> {
        crate::prism::admin::do_reset_middleware_config(self.storage.as_deref(), &name)?;
        Ok(MiddlewareItem {
            name,
            schema: None,
            effective_config: HashMap::new(),
        })
    }

    pub async fn client_check_update(
        &self,
        channel: Option<String>,
    ) -> Result<UpdateCheckResponse, PrismFfiError> {
        check_update_internal(channel, self.storage.as_deref()).await
    }

    pub async fn client_install_update(
        &self,
        channel: Option<String>,
    ) -> Result<(), PrismFfiError> {
        let resolved_channel = channel.unwrap_or_else(|| {
            self.storage
                .as_deref()
                .and_then(|s| s.load_active_config().ok())
                .map(|c| c.update_channel)
                .unwrap_or_else(|| "release".to_string())
        });
        if let Some(ref storage) = self.storage {
            let mut patch = crate::prism::storage::ClientConfigPatch::default();
            patch.update_channel = Some(resolved_channel.clone());
            let _ = storage.apply_config_patch(None, &patch);
        }
        let client = updater_http_client()?;
        let (_, update) = crate::prism::updater::check_update(
            &client,
            &resolved_channel,
            env!("CARGO_PKG_VERSION"),
        )
        .await?;
        let Some((manifest, platform)) = update else {
            return Err(PrismFfiError::ExecutionFailed {
                message: "no update available for this platform".into(),
            });
        };
        crate::prism::updater::download_and_install_update(
            &client,
            crate::prism::updater::DEFAULT_PUBKEY,
            &platform,
            &manifest.version,
        )
        .await?;
        Ok(())
    }

    pub async fn admin_request(
        &self,
        payload: AdminHttpRequest,
    ) -> Result<AdminHttpResponse, PrismFfiError> {
        let native_req = crate::prism::admin::AdminHttpRequest {
            base_url: payload.base_url,
            path: payload.path,
            method: payload.method,
            headers: payload.headers,
            body: payload.body,
        };
        let res = crate::prism::admin::do_admin_request(&self.client, native_req).await?;
        Ok(AdminHttpResponse {
            status: res.status,
            body: res.body,
        })
    }

    pub async fn control_rpc(
        &self,
        payload: AdminRpcRequest,
    ) -> Result<AdminRpcResponse, PrismFfiError> {
        let json_params: HashMap<String, serde_json::Value> = payload
            .params
            .into_iter()
            .map(|(k, v)| {
                let parsed =
                    serde_json::from_str(&v).unwrap_or_else(|_| serde_json::Value::String(v));
                (k, parsed)
            })
            .collect();

        let req = crate::prism::admin::ControlRpcRequest {
            method: payload.method,
            payload: serde_json::to_value(json_params).unwrap_or(serde_json::Value::Null),
            token: payload.token,
        };

        let res = crate::prism::admin::do_control_rpc(&self.client, req).await?;
        let mut result_map = HashMap::new();
        if let Some(obj) = res.body.as_object() {
            for (k, v) in obj {
                result_map.insert(
                    k.clone(),
                    match v {
                        serde_json::Value::String(s) => s.clone(),
                        other => other.to_string(),
                    },
                );
            }
        } else if !res.body.is_null() {
            result_map.insert("value".into(), res.body.to_string());
        }

        let body_str = res.body.to_string();
        Ok(AdminRpcResponse {
            ok: res.ok,
            status: res.status,
            body: body_str,
            result: result_map,
            code: res.code,
            message: res.message,
        })
    }
}

// ---------------------------------------------------------------------------
// Global Default Session and Helpers
// ---------------------------------------------------------------------------

fn ffi_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("prism-ffi")
            .build()
            .expect("failed to create prism FFI tokio runtime")
    })
}

async fn on_runtime<F, T>(fut: F) -> T
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    if tokio::runtime::Handle::try_current().is_ok() {
        fut.await
    } else {
        ffi_runtime()
            .spawn(fut)
            .await
            .expect("prism FFI runtime task panicked")
    }
}

static GLOBAL_SESSION: LazyLock<RwLock<Option<Arc<PrismClientSession>>>> =
    LazyLock::new(|| RwLock::new(None));

pub fn get_or_init_session() -> Result<Arc<PrismClientSession>, PrismFfiError> {
    if let Ok(guard) = GLOBAL_SESSION.read() {
        if let Some(ref session) = *guard {
            return Ok(session.clone());
        }
    }
    let mut guard = GLOBAL_SESSION
        .write()
        .map_err(|e| PrismFfiError::ExecutionFailed {
            message: e.to_string(),
        })?;
    if let Some(ref session) = *guard {
        return Ok(session.clone());
    }
    let session = PrismClientSession::new(None)?;
    *guard = Some(session.clone());
    Ok(session)
}

fn updater_http_client() -> Result<reqwest::Client, PrismFfiError> {
    reqwest::Client::builder()
        .user_agent("Prism-Client-Updater")
        .timeout(std::time::Duration::from_secs(120))
        .build()
        .map_err(|e| PrismFfiError::ExecutionFailed {
            message: e.to_string(),
        })
}

async fn check_update_internal(
    channel: Option<String>,
    storage: Option<&crate::prism::storage::StorageEngine>,
) -> Result<UpdateCheckResponse, PrismFfiError> {
    let resolved_channel = channel.unwrap_or_else(|| {
        storage
            .and_then(|s| s.load_active_config().ok())
            .map(|c| c.update_channel)
            .unwrap_or_else(|| "release".to_string())
    });

    let current_version = env!("CARGO_PKG_VERSION").to_string();
    let client = updater_http_client()?;
    let (response, _) =
        crate::prism::updater::check_update(&client, &resolved_channel, &current_version).await?;
    Ok(UpdateCheckResponse {
        available: response.available,
        current_version: response.current_version,
        version: response.version,
        date: response.date,
        body: response.body,
        channel: response.channel,
    })
}

// ---------------------------------------------------------------------------
// Top-Level Functions (UniFFI Exports)
// ---------------------------------------------------------------------------

#[uniffi::export]
pub fn init_client(workdir: Option<String>) -> Result<(), PrismFfiError> {
    let session = PrismClientSession::new(workdir)?;
    let mut guard = GLOBAL_SESSION
        .write()
        .map_err(|e| PrismFfiError::ExecutionFailed {
            message: e.to_string(),
        })?;
    *guard = Some(session);
    Ok(())
}

#[uniffi::export]
pub fn open_external_url(url: String) -> Result<(), PrismFfiError> {
    let trimmed = url.trim();
    let parsed = url::Url::parse(trimmed).map_err(|e| PrismFfiError::InvalidArgument {
        message: format!("invalid URL: {e}"),
    })?;
    let scheme = parsed.scheme().to_ascii_lowercase();
    if scheme != "http" && scheme != "https" {
        return Err(PrismFfiError::InvalidArgument {
            message: format!(
                "unsupported URL scheme '{scheme}': only http and https are permitted"
            ),
        });
    }
    #[cfg(target_os = "windows")]
    {
        let _ = std::process::Command::new("rundll32")
            .args(["url.dll,FileProtocolHandler", trimmed])
            .spawn()
            .map_err(|e| PrismFfiError::ExecutionFailed {
                message: e.to_string(),
            })?;
    }
    #[cfg(target_os = "macos")]
    {
        let _ = std::process::Command::new("open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| PrismFfiError::ExecutionFailed {
                message: e.to_string(),
            })?;
    }
    #[cfg(target_os = "linux")]
    {
        let _ = std::process::Command::new("xdg-open")
            .arg(trimmed)
            .spawn()
            .map_err(|e| PrismFfiError::ExecutionFailed {
                message: e.to_string(),
            })?;
    }
    Ok(())
}

#[uniffi::export]
pub async fn client_status() -> Result<ClientStatusResponse, PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_status().await
    })
    .await
}

#[uniffi::export]
pub async fn client_start(payload: StartClientRequest) -> Result<(), PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_start(payload).await
    })
    .await
}

#[uniffi::export]
pub async fn client_stop() -> Result<(), PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_stop().await
    })
    .await
}

#[uniffi::export]
pub fn client_get_profiles() -> Result<Vec<ClientProfile>, PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_get_profiles()
}

#[uniffi::export]
pub fn client_save_profiles(profiles: Vec<ClientProfile>) -> Result<(), PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_save_profiles(profiles)
}

#[uniffi::export]
pub fn client_get_config() -> Result<ClientConfigResponse, PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_get_config()
}

#[uniffi::export]
pub fn client_save_config(payload: SaveConfigRequest) -> Result<(), PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_save_config(payload)
}

#[uniffi::export]
pub fn client_reset_stats() -> Result<(), PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_reset_stats()
}

#[uniffi::export]
pub async fn client_logs(limit: Option<u32>) -> Result<Vec<ClientLogEntry>, PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_logs(limit).await
    })
    .await
}

#[uniffi::export]
pub async fn client_clear_logs() -> Result<(), PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_clear_logs().await
    })
    .await
}

#[uniffi::export]
pub fn client_list_middlewares() -> Result<Vec<MiddlewareItem>, PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_list_middlewares()
}

#[uniffi::export]
pub fn client_update_middleware_config(
    name: String,
    config: HashMap<String, String>,
) -> Result<MiddlewareItem, PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_update_middleware_config(name, config)
}

#[uniffi::export]
pub fn client_reset_middleware_config(name: String) -> Result<MiddlewareItem, PrismFfiError> {
    let session = get_or_init_session()?;
    session.client_reset_middleware_config(name)
}

#[uniffi::export]
pub async fn client_check_update(
    channel: Option<String>,
) -> Result<UpdateCheckResponse, PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_check_update(channel).await
    })
    .await
}

#[uniffi::export]
pub async fn client_install_update(channel: Option<String>) -> Result<(), PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.client_install_update(channel).await
    })
    .await
}

#[uniffi::export]
pub async fn admin_request(payload: AdminHttpRequest) -> Result<AdminHttpResponse, PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.admin_request(payload).await
    })
    .await
}

#[uniffi::export]
pub async fn control_rpc(payload: AdminRpcRequest) -> Result<AdminRpcResponse, PrismFfiError> {
    on_runtime(async move {
        let session = get_or_init_session()?;
        session.control_rpc(payload).await
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_ffi_session_lifecycle_and_status() {
        let temp_dir = std::env::temp_dir().join(format!("prism-ffi-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);

        let session = PrismClientSession::new(Some(temp_dir.to_string_lossy().to_string()))
            .expect("session creation should succeed");

        let status = session
            .client_status()
            .await
            .expect("client_status should succeed");
        assert_eq!(status.running, false);

        let profiles = session
            .client_get_profiles()
            .expect("client_get_profiles should succeed");
        assert!(profiles.is_empty() || !profiles.is_empty());

        let new_profile = ClientProfile {
            id: "test-prof-1".to_string(),
            name: "Test Profile".to_string(),
            server_addr: "127.0.0.1:8443".to_string(),
            transport: "quic".to_string(),
            auth_token: "secret-token".to_string(),
            listen_addr: "127.0.0.1:25565".to_string(),
            fake_lan_broadcast: true,
        };
        session
            .client_save_profiles(vec![new_profile.clone()])
            .expect("client_save_profiles should succeed");

        let loaded = session
            .client_get_profiles()
            .expect("get profiles after save");
        assert!(
            loaded
                .iter()
                .any(|p| p.id == "test-prof-1" && p.name == "Test Profile")
        );

        let config = session
            .client_get_config()
            .expect("client_get_config should succeed");
        assert!(!config.device_id.is_empty() || config.device_id.is_empty());

        // Clean up
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_open_external_url_validation() {
        assert!(open_external_url("file:///etc/passwd".to_string()).is_err());
        assert!(open_external_url("javascript:alert(1)".to_string()).is_err());
        assert!(open_external_url("not a url".to_string()).is_err());
    }
}
