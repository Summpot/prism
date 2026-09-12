use std::{collections::HashMap, net::SocketAddr, path::PathBuf, sync::Arc, time::Duration};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tower_http::cors::CorsLayer;

use async_trait::async_trait;

use crate::prism::auth::AuthIdentity;
use crate::prism::control::{
    self, AdminCallContext, AdminControl, AdminError, AdminEventWatches, AdminMethod, AdminPayload,
    AuthSessionSnapshot, FEATURE_AUTH, FEATURE_EVENTS, FEATURE_MANAGED, FEATURE_MIDDLEWARE,
    FEATURE_PANEL, FEATURE_RPC, MiddlewareItem,
};
use crate::prism::telemetry;
use crate::prism::{managed, tunnel};

#[derive(Clone, Debug, Default)]
pub struct AdminAuth {
    pub panel_token: Option<String>,
    pub worker_token: Option<String>,
}

#[derive(Clone)]
pub struct AdminState {
    pub sessions: telemetry::SharedSessions,
    pub optimizer: telemetry::SharedOptimizerRegistry,
    pub config_path: PathBuf,
    pub reload_tx: watch::Sender<telemetry::ReloadSignal>,
    pub tunnel: Option<Arc<tunnel::manager::Manager>>,
    pub auth: AdminAuth,
    pub management: Option<Arc<managed::ManagementPlane>>,
    pub worker: Option<Arc<managed::WorkerAgent>>,
    pub client: Option<Arc<tunnel::client::ClientController>>,
    pub auth_manager: Option<Arc<crate::prism::auth::AuthManager>>,
    pub storage: Option<Arc<crate::prism::storage::StorageEngine>>,
}

#[allow(dead_code)]
pub async fn serve(addr: SocketAddr, state: AdminState) -> anyhow::Result<()> {
    let (tx, rx) = watch::channel(false);
    let _tx = tx;
    serve_with_shutdown(addr, state, rx).await
}

pub async fn serve_listener_with_shutdown(
    listener: tokio::net::TcpListener,
    state: AdminState,
    shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let app = build_router(state);
    let addr = listener.local_addr()?;
    tracing::info!(admin_addr = %addr, "admin: listening");

    axum::serve(listener, app)
        .with_graceful_shutdown(wait_shutdown(shutdown))
        .await?;

    Ok(())
}

pub async fn serve_with_shutdown(
    addr: SocketAddr,
    state: AdminState,
    shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let listener = tokio::net::TcpListener::bind(addr).await?;
    serve_listener_with_shutdown(listener, state, shutdown).await
}

#[derive(Debug, Serialize)]
struct RootInfoResponse {
    service: &'static str,
    version: &'static str,
    status: &'static str,
    frontend: &'static str,
    message: &'static str,
}

async fn root_info() -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(RootInfoResponse {
            service: "prism-admin-api",
            version: env!("CARGO_PKG_VERSION"),
            status: "running",
            frontend: "desktop-only",
            message: "Prism web frontend is deprecated. Please use the official Prism desktop application.",
        }),
    )
}

pub(crate) fn build_router(state: AdminState) -> Router {
    let shared = Arc::new(state);
    Router::new()
        .route("/", get(root_info))
        .route("/health", get(health))
        .route("/conns", get(conns))
        .route("/tunnel/services", get(tunnel_services))
        .route("/reload", post(reload))
        .route("/config", get(config))
        .route("/managed/status", get(managed_status))
        .route("/managed/nodes", get(managed_nodes))
        .route("/managed/nodes/{node_id}", get(managed_node))
        .route(
            "/managed/nodes/{node_id}/config",
            get(managed_node_config).put(put_managed_node_config),
        )
        .route("/managed/worker/sync", post(managed_worker_sync))
        .route("/managed/worker/status", get(worker_status))
        .route("/managed/worker/config", put(worker_apply_config))
        .route("/stats/optimizer", get(stats_optimizer))
        .route("/client/status", get(client_status))
        .route("/client/start", post(client_start))
        .route("/client/stop", post(client_stop))
        .route(
            "/client/profiles",
            get(client_get_profiles).post(client_save_profiles),
        )
        .route(
            "/client/config",
            get(client_get_config).post(client_save_config),
        )
        .route("/client/stats", axum::routing::delete(client_reset_stats))
        .route("/client/logs", get(client_logs).delete(client_clear_logs))
        .route("/middlewares", get(list_middlewares))
        .route("/middlewares/{name}/schema", get(get_middleware_schema))
        .route(
            "/middlewares/{name}/config",
            get(get_middleware_config).put(put_middleware_config),
        )
        .route(
            "/middlewares/{name}/config/reset",
            post(reset_middleware_config),
        )
        .route("/middlewares/{name}/data", post(post_middleware_data))
        .route("/auth/providers", get(auth_providers))
        .route("/auth/github/login", get(auth_github_login))
        .route("/auth/github/callback", get(auth_github_callback))
        .route("/auth/github/exchange", post(auth_github_exchange))
        .route("/auth/session", get(auth_session))
        .route(
            "/auth/tokens",
            get(auth_list_tokens).post(auth_create_token),
        )
        .route(
            "/auth/tokens/{token_id}",
            axum::routing::delete(auth_revoke_token),
        )
        .route("/managed/users", get(managed_users))
        .route("/managed/users/{user_id}", put(put_managed_user))
        .with_state(shared)
        .layer(CorsLayer::permissive())
}

async fn wait_shutdown(mut shutdown: watch::Receiver<bool>) {
    if *shutdown.borrow() {
        return;
    }
    while shutdown.changed().await.is_ok() {
        if *shutdown.borrow() {
            break;
        }
    }
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(HealthResponse { ok: true }))
}

async fn conns(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let snap = st.sessions.snapshot();
    (StatusCode::OK, Json(snap))
}

async fn tunnel_services(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let snap = if let Some(mgr) = &st.tunnel {
        mgr.snapshot_services().await
    } else {
        Vec::new()
    };
    (StatusCode::OK, Json(snap))
}

#[derive(Debug, Serialize)]
pub struct OptimizerOverviewResponse {
    pub global: tunnel::optimizer::OptimizerStatsSnapshot,
    pub services: std::collections::HashMap<String, tunnel::optimizer::OptimizerStatsSnapshot>,
}

async fn stats_optimizer(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let (global, services) = st.optimizer.snapshot();
    (
        StatusCode::OK,
        Json(OptimizerOverviewResponse { global, services }),
    )
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StartClientRequest {
    pub server_addr: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default)]
    pub middleware: Option<String>,
    #[serde(default = "default_true")]
    pub fake_lan_broadcast: bool,
    #[serde(default = "default_motd_prefix")]
    pub motd_prefix: String,
    #[serde(default)]
    pub optimizer: Option<crate::prism::config::OptimizerClientConfig>,
    #[serde(default)]
    pub profile_id: Option<String>,
    #[serde(default)]
    pub profile_name: Option<String>,
}

fn default_transport() -> String {
    "quic".to_string()
}
fn default_listen_addr() -> String {
    "127.0.0.1:25565".to_string()
}
fn default_true() -> bool {
    true
}
fn default_motd_prefix() -> String {
    "[Prism] ".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClientProfile {
    pub id: String,
    pub name: String,
    pub server_addr: String,
    #[serde(default = "default_transport")]
    pub transport: String,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    #[serde(default = "default_true")]
    pub fake_lan_broadcast: bool,
}

fn profiles_path() -> PathBuf {
    if let Some(proj) = directories::ProjectDirs::from("com", "summpot", "prism") {
        let dir = proj.config_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("profiles.json")
    } else if let Some(proj) = directories::ProjectDirs::from("com", "prism", "prism") {
        let dir = proj.config_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("profiles.json")
    } else {
        PathBuf::from("profiles.json")
    }
}

// ---------------------------------------------------------------------------
// Shared Client Controller & Storage Logic (Used by HTTP API & Tauri Commands)
// ---------------------------------------------------------------------------

pub(crate) async fn do_client_status(
    client: Option<&crate::prism::tunnel::client::ClientController>,
    storage: Option<&crate::prism::storage::StorageEngine>,
) -> serde_json::Value {
    let (active_profile_id, cumulative_stats) = if let Some(storage) = storage {
        (
            storage.load_active_profile_id().ok().flatten(),
            Some(storage.load_cumulative_stats().unwrap_or_default()),
        )
    } else {
        (None, None)
    };

    let mut val = if let Some(client) = client {
        serde_json::to_value(client.status().await).unwrap_or_default()
    } else {
        serde_json::to_value(tunnel::client::ClientStatusSnapshot::default()).unwrap_or_default()
    };

    if let Some(obj) = val.as_object_mut() {
        obj.insert(
            "active_profile_id".into(),
            serde_json::to_value(active_profile_id).unwrap_or(serde_json::Value::Null),
        );
        obj.insert(
            "cumulative_stats".into(),
            serde_json::to_value(cumulative_stats).unwrap_or(serde_json::Value::Null),
        );
    }
    val
}

pub(crate) async fn do_client_start(
    client: &crate::prism::tunnel::client::ClientController,
    storage: Option<&crate::prism::storage::StorageEngine>,
    payload: StartClientRequest,
) -> Result<(), String> {
    if let Some(storage) = storage {
        persist_started_client(storage, &payload);
    }

    let cfg = crate::prism::config::TunnelClientConfig {
        server_addr: payload.server_addr,
        transport: payload.transport,
        auth_token: payload.auth_token,
        listen_addr: payload.listen_addr,
        middleware: payload.middleware,
        fake_lan_broadcast: payload.fake_lan_broadcast,
        motd_prefix: payload.motd_prefix,
        optimizer: payload.optimizer,
        websocket: None,
        doh_servers: Vec::new(),
    };

    client.start(cfg).await.map_err(|err| err.to_string())
}

fn persist_started_client(
    storage: &crate::prism::storage::StorageEngine,
    payload: &StartClientRequest,
) {
    let existing = storage.load_active_config().unwrap_or_default();
    let profile_name = payload
        .profile_name
        .clone()
        .filter(|s| !s.trim().is_empty())
        .unwrap_or_else(|| {
            if existing.profile_name.trim().is_empty() {
                "Default Realm".to_string()
            } else {
                existing.profile_name.clone()
            }
        });
    let profile_id = payload
        .profile_id
        .clone()
        .filter(|s| !s.trim().is_empty())
        .or_else(|| storage.load_active_profile_id().ok().flatten())
        .unwrap_or_else(|| format!("profile-{}", crate::prism::telemetry::now_unix_ms()));

    let form_state = crate::prism::storage::ClientConfigState {
        profile_name: profile_name.clone(),
        server_addr: payload.server_addr.clone(),
        transport: payload.transport.clone(),
        auth_token: payload.auth_token.clone(),
        listen_addr: payload.listen_addr.clone(),
        fake_lan_broadcast: payload.fake_lan_broadcast,
        auto_connect_panel: existing.auto_connect_panel,
        auto_connect: existing.auto_connect,
        management_url: existing.management_url.clone(),
        token_id: existing.token_id.clone(),
        token_type: existing.token_type.clone(),
        user_id: existing.user_id.clone(),
        username: existing.username.clone(),
        expires_at: existing.expires_at,
    };

    let _ = storage.save_active_profile_id(&profile_id);
    let _ = storage.upsert_profile(&ClientProfile {
        id: profile_id,
        name: profile_name,
        server_addr: payload.server_addr.clone(),
        transport: payload.transport.clone(),
        auth_token: payload.auth_token.clone(),
        listen_addr: payload.listen_addr.clone(),
        fake_lan_broadcast: payload.fake_lan_broadcast,
    });
    let _ = storage.save_active_config(&form_state);
}

pub(crate) async fn do_client_stop(
    client: &crate::prism::tunnel::client::ClientController,
    storage: Option<&crate::prism::storage::StorageEngine>,
) -> Result<(), String> {
    if let Some(storage) = storage {
        let snap = client.status().await;
        let _ = storage.record_session_stats(&snap.stats);
    }
    client.stop().await;
    Ok(())
}

pub(crate) fn do_client_get_profiles(
    storage: Option<&crate::prism::storage::StorageEngine>,
) -> Vec<ClientProfile> {
    if let Some(storage) = storage {
        if let Ok(profiles) = storage.load_profiles() {
            return profiles;
        }
    }
    let path = profiles_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(profiles) = serde_json::from_str::<Vec<ClientProfile>>(&data) {
            return profiles;
        }
    }
    Vec::new()
}

pub(crate) fn do_client_save_profiles(
    storage: Option<&crate::prism::storage::StorageEngine>,
    profiles: &[ClientProfile],
) -> Result<(), String> {
    if let Some(storage) = storage {
        return storage.save_profiles(profiles).map_err(|e| e.to_string());
    }
    let path = profiles_path();
    let data = serde_json::to_string_pretty(profiles).map_err(|e| e.to_string())?;
    std::fs::write(&path, data).map_err(|e| e.to_string())?;
    Ok(())
}

pub(crate) fn do_client_get_config(
    storage: Option<&crate::prism::storage::StorageEngine>,
) -> crate::prism::storage::ClientConfigResponse {
    if let Some(storage) = storage {
        return storage.get_client_config_snapshot();
    }
    crate::prism::storage::ClientConfigResponse {
        active_profile_id: None,
        active_config: crate::prism::storage::ClientConfigState::default(),
        profiles: Vec::new(),
        cumulative_stats: tunnel::optimizer::OptimizerStatsSnapshot::default(),
        device_id: String::new(),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SaveConfigRequest {
    #[serde(default)]
    pub active_profile_id: Option<String>,
    #[serde(default)]
    pub active_config: Option<crate::prism::storage::ClientConfigPatch>,
}

pub(crate) fn do_client_save_config(
    storage: Option<&crate::prism::storage::StorageEngine>,
    payload: SaveConfigRequest,
) -> Result<(), String> {
    if let Some(storage) = storage {
        let patch = payload.active_config.unwrap_or_default();
        storage
            .apply_config_patch(payload.active_profile_id.as_deref(), &patch)
            .map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(crate) fn do_client_reset_stats(
    storage: Option<&crate::prism::storage::StorageEngine>,
) -> Result<(), String> {
    if let Some(storage) = storage {
        let _ = storage.reset_cumulative_stats();
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
pub struct ClientLogsQuery {
    pub limit: Option<usize>,
}

pub(crate) async fn do_client_logs(
    client: Option<&crate::prism::tunnel::client::ClientController>,
    limit: usize,
) -> Vec<tunnel::client::ClientLogEntry> {
    if let Some(client) = client {
        client.logs(limit).await
    } else {
        Vec::new()
    }
}

pub(crate) async fn do_client_clear_logs(
    client: Option<&crate::prism::tunnel::client::ClientController>,
) {
    if let Some(client) = client {
        client.clear_logs().await;
    }
}

pub(crate) fn do_list_middlewares() -> Result<Vec<MiddlewareItem>, String> {
    let mut items = Vec::new();
    let engine = wasmtime::Engine::default();
    for (name, wat) in crate::prism::middleware::DEFAULT_MIDDLEWARES {
        let (_, schema) =
            crate::prism::middleware::compile_module_from_wat(&engine, name, wat.as_bytes())
                .map_err(|e| e.to_string())?;
        let mut effective = std::collections::HashMap::new();
        if let Some(ref s) = schema {
            for f in &s.fields {
                effective.insert(f.key.clone(), f.default_value.clone());
            }
        }
        if let Some(overrides) = crate::prism::middleware::get_dynamic_middleware_config(name) {
            for (k, v) in overrides {
                effective.insert(k, v);
            }
        }
        items.push(MiddlewareItem {
            name: name.to_string(),
            schema,
            effective_config: effective,
        });
    }
    Ok(items)
}

pub(crate) fn do_put_middleware_config(
    storage: Option<&crate::prism::storage::StorageEngine>,
    name: &str,
    config: HashMap<String, serde_json::Value>,
) -> Result<HashMap<String, serde_json::Value>, String> {
    let base_name = name.strip_suffix(".wat").unwrap_or(name).trim().to_string();
    if crate::prism::middleware::get_default_middleware_wat(&base_name).is_none() {
        return Err(format!("middleware '{name}' not found"));
    }
    if let Some(storage) = storage {
        storage
            .save_middleware_config(&base_name, &config)
            .map_err(|e| e.to_string())?;
    }
    crate::prism::middleware::set_dynamic_middleware_config(&base_name, config.clone());
    Ok(config)
}

pub(crate) fn do_reset_middleware_config(
    storage: Option<&crate::prism::storage::StorageEngine>,
    name: &str,
) -> Result<(), String> {
    let base_name = name.strip_suffix(".wat").unwrap_or(name).trim();
    if crate::prism::middleware::get_default_middleware_wat(base_name).is_none() {
        return Err(format!("middleware '{name}' not found"));
    }
    if let Some(storage) = storage {
        let _ = storage.delete_middleware_config(base_name);
    }
    crate::prism::middleware::reset_dynamic_middleware_config(base_name);
    if let Some(wat) = crate::prism::middleware::get_default_middleware_wat(base_name) {
        let engine = wasmtime::Engine::default();
        if let Ok((_, Some(schema))) =
            crate::prism::middleware::compile_module_from_wat(&engine, base_name, wat.as_bytes())
        {
            let mut defaults = std::collections::HashMap::new();
            for f in schema.fields {
                defaults.insert(f.key, f.default_value);
            }
            crate::prism::middleware::broadcast_session_config_update(base_name, &defaults);
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AdminHttpRequest {
    #[serde(default)]
    pub base_url: String,
    pub path: String,
    #[serde(default)]
    pub method: String,
    #[serde(default)]
    pub headers: HashMap<String, String>,
    #[serde(default)]
    pub body: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminHttpResponse {
    pub status: u16,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRpcRequest {
    pub method: String,
    #[serde(default)]
    pub payload: serde_json::Value,
    #[serde(default)]
    pub token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminRpcResponse {
    pub ok: bool,
    #[serde(default)]
    pub body: serde_json::Value,
    #[serde(default)]
    pub status: u16,
    #[serde(default)]
    pub code: Option<String>,
    #[serde(default)]
    pub message: Option<String>,
}

fn normalize_http_base(url: &str) -> String {
    url.trim().trim_end_matches('/').to_string()
}

pub(crate) async fn do_admin_request(
    _client: &crate::prism::tunnel::client::ClientController,
    payload: AdminHttpRequest,
) -> Result<AdminHttpResponse, String> {
    let method = payload.method.trim();
    let method = if method.is_empty() { "GET" } else { method };
    let path = if payload.path.starts_with('/') {
        payload.path.clone()
    } else if payload.path.is_empty() {
        "/".to_string()
    } else {
        format!("/{}", payload.path.trim())
    };

    let base = normalize_http_base(&payload.base_url);
    if base.is_empty() {
        return Err("admin request: missing base_url".into());
    }
    let url = format!("{base}{path}");
    let http = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|err| err.to_string())?;
    let parsed_method: reqwest::Method = method.parse().map_err(|err| format!("{err}"))?;
    let mut builder = http.request(parsed_method, &url);
    for (k, v) in &payload.headers {
        builder = builder.header(k.as_str(), v.as_str());
    }
    if let Some(ref body) = payload.body {
        builder = builder.body(body.clone());
    }
    let res = builder.send().await.map_err(|err| err.to_string())?;
    let status = res.status().as_u16();
    let body = res.text().await.map_err(|err| err.to_string())?;
    Ok(AdminHttpResponse { status, body })
}

pub(crate) async fn do_admin_rpc(
    client: &crate::prism::tunnel::client::ClientController,
    payload: AdminRpcRequest,
) -> Result<AdminRpcResponse, String> {
    let method = control::AdminMethod::from_rpc(&payload.method, &payload.payload)
        .map_err(|e| e.to_string())?;
    match client
        .admin_rpc(method, payload.token.as_deref())
        .await
    {
        Ok(body) => Ok(AdminRpcResponse {
            ok: true,
            body: body.to_json_value(),
            status: 200,
            code: None,
            message: None,
        }),
        Err(err) => Ok(AdminRpcResponse {
            ok: false,
            body: serde_json::json!({ "error": err.message }),
            status: err.http_status(),
            code: Some(format!("{:?}", err.code)),
            message: Some(err.message),
        }),
    }
}

// ---------------------------------------------------------------------------
// HTTP Axum Route Handlers
// ---------------------------------------------------------------------------

async fn client_status(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let val = do_client_status(st.client.as_deref(), st.storage.as_deref()).await;
    (StatusCode::OK, Json(val))
}

async fn client_start(
    State(st): State<Arc<AdminState>>,
    Json(payload): Json<StartClientRequest>,
) -> impl IntoResponse {
    let Some(ref client) = st.client else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "client controller not enabled" })),
        );
    };

    match do_client_start(client, st.storage.as_deref(), payload).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn client_stop(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let Some(ref client) = st.client else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "client controller not enabled" })),
        );
    };
    let _ = do_client_stop(client, st.storage.as_deref()).await;
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn client_get_profiles(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let profiles = do_client_get_profiles(st.storage.as_deref());
    (StatusCode::OK, Json(profiles))
}

async fn client_save_profiles(
    State(st): State<Arc<AdminState>>,
    Json(profiles): Json<Vec<ClientProfile>>,
) -> impl IntoResponse {
    match do_client_save_profiles(st.storage.as_deref(), &profiles) {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err })),
        ),
    }
}

async fn client_get_config(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let cfg = do_client_get_config(st.storage.as_deref());
    (StatusCode::OK, Json(cfg))
}

async fn client_save_config(
    State(st): State<Arc<AdminState>>,
    Json(payload): Json<SaveConfigRequest>,
) -> impl IntoResponse {
    let _ = do_client_save_config(st.storage.as_deref(), payload);
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn client_reset_stats(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let _ = do_client_reset_stats(st.storage.as_deref());
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

async fn client_logs(
    State(st): State<Arc<AdminState>>,
    Query(query): Query<ClientLogsQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    let logs = do_client_logs(st.client.as_deref(), limit).await;
    (StatusCode::OK, Json(logs)).into_response()
}

async fn client_clear_logs(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    if let Some(ref client) = st.client {
        do_client_clear_logs(Some(client)).await;
        (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "client controller not enabled" })),
        )
            .into_response()
    }
}

#[derive(Debug, Serialize)]
struct ReloadResponse {
    seq: u64,
}

async fn reload(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_mutation_auth(&headers, &st).await?;

    let mut next = (*st.reload_tx.borrow()).clone();
    next.next();
    let seq = next.seq;
    let _ = st.reload_tx.send(next);

    Ok((StatusCode::OK, Json(ReloadResponse { seq })))
}

#[derive(Debug, Serialize)]
struct ConfigResponse {
    path: String,
}

async fn config(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        Json(ConfigResponse {
            path: st.config_path.display().to_string(),
        }),
    )
}

async fn managed_status(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let management = st
        .management
        .as_ref()
        .ok_or_else(|| ApiError::not_found("management API not enabled"))?;
    Ok((StatusCode::OK, Json(management.status().await)))
}

async fn managed_nodes(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let management = st
        .management
        .as_ref()
        .ok_or_else(|| ApiError::not_found("management API not enabled"))?;
    Ok((StatusCode::OK, Json(management.list_nodes().await)))
}

async fn managed_node(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(node_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let management = st
        .management
        .as_ref()
        .ok_or_else(|| ApiError::not_found("management API not enabled"))?;

    let node = management
        .get_node(&node_id)
        .await
        .ok_or_else(|| ApiError::not_found("managed node not found"))?;
    Ok((StatusCode::OK, Json(node)))
}

async fn managed_node_config(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(node_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let management = st
        .management
        .as_ref()
        .ok_or_else(|| ApiError::not_found("management API not enabled"))?;

    let node = management
        .get_node_config(&node_id)
        .await
        .ok_or_else(|| ApiError::not_found("managed node not found"))?;
    Ok((StatusCode::OK, Json(node)))
}

async fn put_managed_node_config(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(node_id): AxumPath<String>,
    Json(request): Json<managed::PutManagedNodeConfigRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let management = st
        .management
        .as_ref()
        .ok_or_else(|| ApiError::not_found("management API not enabled"))?;

    let response = management
        .set_desired_config(&node_id, request.desired_config)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::OK, Json(response)))
}

async fn managed_worker_sync(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    Json(request): Json<managed::WorkerSyncRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_worker_auth(&headers, &st)?;
    let management = st
        .management
        .as_ref()
        .ok_or_else(|| ApiError::not_found("management worker sync not enabled"))?;

    let response = management
        .worker_sync(request)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::OK, Json(response)))
}

async fn worker_status(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_worker_auth(&headers, &st)?;
    let worker = st
        .worker
        .as_ref()
        .ok_or_else(|| ApiError::not_found("worker agent not enabled"))?;
    Ok((StatusCode::OK, Json(worker.status_snapshot().await)))
}

async fn worker_apply_config(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    Json(request): Json<managed::WorkerConfigPushRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_worker_auth(&headers, &st)?;
    let worker = st
        .worker
        .as_ref()
        .ok_or_else(|| ApiError::not_found("worker agent not enabled"))?;
    let response = worker
        .apply_push(request.desired_revision, request.desired_config)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::OK, Json(response)))
}

#[derive(Debug, serde::Deserialize, serde::Serialize)]
pub struct MiddlewareDataPayload {
    pub port: Option<u16>,
    pub data: String,
}

#[derive(Debug, serde::Serialize)]
pub struct MiddlewareDataResponse {
    pub status: String,
    pub name: String,
    pub bytes_received: usize,
}

async fn post_middleware_data(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(name): AxumPath<String>,
    Json(payload): Json<MiddlewareDataPayload>,
) -> Result<impl IntoResponse, ApiError> {
    require_mutation_auth(&headers, &st).await?;

    use base64::Engine;
    let trimmed = payload.data.trim();
    let raw_bytes = base64::engine::general_purpose::STANDARD
        .decode(trimmed)
        .or_else(|_| base64::engine::general_purpose::STANDARD_NO_PAD.decode(trimmed))
        .map_err(|e| ApiError::bad_request(anyhow::anyhow!("invalid base64 data: {e}")))?;

    let bytes_received = raw_bytes.len();
    crate::prism::middleware::set_injected_middleware_data(&name, payload.port, raw_bytes);

    Ok((
        StatusCode::OK,
        Json(MiddlewareDataResponse {
            status: "ok".to_string(),
            name,
            bytes_received,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct MiddlewareItemResponse {
    pub name: String,
    pub schema: Option<crate::prism::middleware::MiddlewareConfigSchema>,
    pub effective_config: std::collections::HashMap<String, serde_json::Value>,
}

async fn list_middlewares(
    State(_st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    let mut items = Vec::new();
    let engine = wasmtime::Engine::default();

    for (name, wat) in crate::prism::middleware::DEFAULT_MIDDLEWARES {
        let (_, schema) =
            crate::prism::middleware::compile_module_from_wat(&engine, name, wat.as_bytes())
                .map_err(ApiError::bad_request)?;

        let mut effective = std::collections::HashMap::new();
        if let Some(ref s) = schema {
            for f in &s.fields {
                effective.insert(f.key.clone(), f.default_value.clone());
            }
        }
        if let Some(overrides) = crate::prism::middleware::get_dynamic_middleware_config(name) {
            for (k, v) in overrides {
                effective.insert(k, v);
            }
        }

        items.push(MiddlewareItemResponse {
            name: name.to_string(),
            schema,
            effective_config: effective,
        });
    }

    Ok((StatusCode::OK, Json(items)))
}

async fn get_middleware_schema(
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let base_name = name.strip_suffix(".wat").unwrap_or(&name).trim();
    let wat = crate::prism::middleware::get_default_middleware_wat(base_name)
        .ok_or_else(|| ApiError::not_found(&format!("middleware '{name}' not found")))?;

    let engine = wasmtime::Engine::default();
    let (_, schema) =
        crate::prism::middleware::compile_module_from_wat(&engine, base_name, wat.as_bytes())
            .map_err(ApiError::bad_request)?;

    let s = schema.ok_or_else(|| {
        ApiError::not_found(&format!("middleware '{name}' has no component schema"))
    })?;
    Ok((StatusCode::OK, Json(s)))
}

async fn get_middleware_config(
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    let base_name = name.strip_suffix(".wat").unwrap_or(&name).trim();
    let wat = crate::prism::middleware::get_default_middleware_wat(base_name)
        .ok_or_else(|| ApiError::not_found(&format!("middleware '{name}' not found")))?;

    let engine = wasmtime::Engine::default();
    let (_, schema) =
        crate::prism::middleware::compile_module_from_wat(&engine, base_name, wat.as_bytes())
            .map_err(ApiError::bad_request)?;

    let mut effective = std::collections::HashMap::new();
    if let Some(ref s) = schema {
        for f in &s.fields {
            effective.insert(f.key.clone(), f.default_value.clone());
        }
    }
    if let Some(overrides) = crate::prism::middleware::get_dynamic_middleware_config(base_name) {
        for (k, v) in overrides {
            effective.insert(k, v);
        }
    }

    Ok((StatusCode::OK, Json(effective)))
}

async fn put_middleware_config(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(name): AxumPath<String>,
    Json(config): Json<std::collections::HashMap<String, serde_json::Value>>,
) -> Result<impl IntoResponse, ApiError> {
    require_mutation_auth(&headers, &st).await?;
    let base_name = name.strip_suffix(".wat").unwrap_or(&name).trim();

    if let Some(ref storage) = st.storage {
        storage
            .save_middleware_config(base_name, &config)
            .map_err(ApiError::bad_request)?;
    }

    crate::prism::middleware::set_dynamic_middleware_config(base_name, config.clone());

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "name": base_name,
            "config": config,
        })),
    ))
}

async fn reset_middleware_config(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(name): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_mutation_auth(&headers, &st).await?;
    let base_name = name.strip_suffix(".wat").unwrap_or(&name).trim();

    if let Some(ref storage) = st.storage {
        let _ = storage.delete_middleware_config(base_name);
    }

    crate::prism::middleware::reset_dynamic_middleware_config(base_name);

    if let Some(wat) = crate::prism::middleware::get_default_middleware_wat(base_name) {
        let engine = wasmtime::Engine::default();
        if let Ok((_, Some(schema))) =
            crate::prism::middleware::compile_module_from_wat(&engine, base_name, wat.as_bytes())
        {
            let mut defaults = std::collections::HashMap::new();
            for f in schema.fields {
                defaults.insert(f.key, f.default_value);
            }
            crate::prism::middleware::broadcast_session_config_update(base_name, &defaults);
        }
    }

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "status": "ok",
            "name": base_name,
            "reset": true,
        })),
    ))
}

#[derive(Debug, Serialize)]
struct ErrorResponse {
    error: String,
}

struct ApiError {
    status: StatusCode,
    message: String,
}

impl ApiError {
    fn unauthorized(message: &str) -> Self {
        Self {
            status: StatusCode::UNAUTHORIZED,
            message: message.to_string(),
        }
    }

    fn not_found(message: &str) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            message: message.to_string(),
        }
    }

    fn bad_request(err: anyhow::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message: err.to_string(),
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (
            self.status,
            Json(ErrorResponse {
                error: self.message,
            }),
        )
            .into_response()
    }
}

async fn require_mutation_auth(headers: &HeaderMap, st: &AdminState) -> Result<(), ApiError> {
    if let Some(token) = extract_bearer_token(headers) {
        if let Some(ref am) = st.auth_manager {
            if let Some(ident) = am.verify_token(&token).await {
                if ident.is_admin {
                    return Ok(());
                }
            }
        }
        if let Some(expected) = st
            .auth
            .panel_token
            .as_ref()
            .or(st.auth.worker_token.as_ref())
        {
            if token.trim() == expected.trim() {
                return Ok(());
            }
        }
        Err(ApiError::unauthorized("invalid bearer token"))
    } else if st.auth.panel_token.is_none()
        && st.auth.worker_token.is_none()
        && st.auth_manager.is_none()
    {
        Ok(())
    } else {
        Err(ApiError::unauthorized("missing Authorization header"))
    }
}

async fn require_panel_auth(headers: &HeaderMap, st: &AdminState) -> Result<(), ApiError> {
    let token = extract_bearer_token(headers)
        .ok_or_else(|| ApiError::unauthorized("missing Authorization header"))?;

    if let Some(ref am) = st.auth_manager {
        if let Some(ident) = am.verify_token(&token).await {
            if ident.is_admin {
                return Ok(());
            }
        }
    }

    if let Some(expected) = st.auth.panel_token.as_ref() {
        if token.trim() == expected.trim() {
            return Ok(());
        }
    }

    Err(ApiError::unauthorized("panel authentication required"))
}

fn require_worker_auth(headers: &HeaderMap, st: &AdminState) -> Result<(), ApiError> {
    let token = st
        .auth
        .worker_token
        .as_ref()
        .ok_or_else(|| ApiError::not_found("worker auth not configured"))?;
    require_bearer(headers, token)
}

fn extract_bearer_token(headers: &HeaderMap) -> Option<String> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value.strip_prefix("Bearer ")?;
    Some(token.trim().to_string())
}

fn require_bearer(headers: &HeaderMap, expected: &str) -> Result<(), ApiError> {
    let Some(token) = extract_bearer_token(headers) else {
        return Err(ApiError::unauthorized("missing Authorization header"));
    };
    if token.trim() != expected.trim() {
        return Err(ApiError::unauthorized("invalid bearer token"));
    }
    Ok(())
}

#[derive(Debug, Serialize)]
pub struct AuthProvidersResponse {
    pub github_enabled: bool,
    pub github_client_id: Option<String>,
    pub mode: String,
    pub providers: Vec<String>,
}

async fn auth_providers(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let (github_enabled, github_client_id, mode, providers) = if let Some(ref am) = st.auth_manager
    {
        let gh = am.github_config();
        let gh_enabled = gh.is_some();
        let gh_client_id = gh.map(|g| g.client_id.clone());
        let mode_str = am.auth_mode().to_string();

        let mut providers = Vec::new();
        if gh_enabled && mode_str != "token" {
            providers.push("github".to_string());
        }

        (gh_enabled, gh_client_id, mode_str, providers)
    } else {
        (false, None, "token".to_string(), Vec::new())
    };

    (
        StatusCode::OK,
        Json(AuthProvidersResponse {
            github_enabled,
            github_client_id,
            mode,
            providers,
        }),
    )
}

#[derive(Debug, Serialize)]
pub struct GitHubLoginResponse {
    pub url: String,
}

async fn auth_github_login(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("auth manager not configured")))?;
    let gh = am
        .github_config()
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("GitHub OAuth not enabled")))?;

    let mut url = format!(
        "https://github.com/login/oauth/authorize?client_id={}&scope=read:user",
        gh.client_id
    );
    if let Some(ref r) = gh.redirect_uri {
        url.push_str(&format!("&redirect_uri={r}"));
    }

    let is_html = headers
        .get(axum::http::header::ACCEPT)
        .and_then(|h| h.to_str().ok())
        .map(|v| v.contains("text/html"))
        .unwrap_or(false);

    if is_html {
        Ok(axum::response::Redirect::temporary(&url).into_response())
    } else {
        Ok((StatusCode::OK, Json(GitHubLoginResponse { url })).into_response())
    }
}

#[derive(Debug, Deserialize)]
pub struct GitHubCallbackQuery {
    pub code: Option<String>,
    pub state: Option<String>,
    pub error: Option<String>,
    pub error_description: Option<String>,
}

async fn auth_github_callback(Query(query): Query<GitHubCallbackQuery>) -> impl IntoResponse {
    let content = if let Some(code) = query.code.as_deref() {
        let clean_code = code.trim();
        let state_param = query
            .state
            .as_deref()
            .map(|s| format!("&state={}", s.trim()))
            .unwrap_or_default();
        let deep_link = format!("prism://auth/callback?code={clean_code}{state_param}");
        format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Prism - GitHub 授权完成</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif;
            background: #090d16;
            color: #e2e8f0;
            display: flex;
            align-items: center;
            justify-content: center;
            min-height: 100vh;
            margin: 0;
            padding: 20px;
            box-sizing: border-box;
        }}
        .card {{
            background: #111827;
            border: 1px solid #1f2937;
            border-radius: 16px;
            padding: 32px;
            max-width: 480px;
            width: 100%;
            text-align: center;
            box-shadow: 0 20px 25px -5px rgba(0, 0, 0, 0.5);
        }}
        .icon {{
            width: 48px;
            height: 48px;
            margin: 0 auto 16px;
            background: #0284c7;
            border-radius: 50%;
            display: flex;
            align-items: center;
            justify-content: center;
            color: white;
            font-size: 24px;
            font-weight: bold;
        }}
        h2 {{ margin: 0 0 12px; font-size: 20px; font-weight: 600; color: #f8fafc; }}
        p {{ margin: 0 0 24px; font-size: 14px; color: #94a3b8; line-height: 1.5; }}
        .btn {{
            display: inline-block;
            background: #0284c7;
            color: white;
            padding: 10px 20px;
            border-radius: 8px;
            text-decoration: none;
            font-size: 14px;
            font-weight: 500;
            margin: 4px;
            cursor: pointer;
            border: none;
            transition: background 0.2s;
        }}
        .btn:hover {{ background: #0369a1; }}
        .btn-secondary {{
            background: #1f2937;
            color: #cbd5e1;
        }}
        .btn-secondary:hover {{ background: #374151; }}
        .code-box {{
            margin-top: 20px;
            padding: 12px;
            background: #030712;
            border: 1px solid #1f2937;
            border-radius: 8px;
            font-family: monospace;
            font-size: 13px;
            word-break: break-all;
            color: #38bdf8;
            user-select: all;
        }}
        .tip {{
            margin-top: 16px;
            font-size: 12px;
            color: #64748b;
        }}
    </style>
</head>
<body>
    <div class="card">
        <div class="icon">✓</div>
        <h2>GitHub 授权成功</h2>
        <p>正在自动唤起 Prism 客户端完成登录...</p>
        <div>
            <a id="deep-link-btn" href="{deep_link}" class="btn">打开 Prism 客户端</a>
            <button id="copy-btn" class="btn btn-secondary" onclick="copyCode()">复制授权码</button>
        </div>
        <div class="code-box" id="code-display">{clean_code}</div>
        <div class="tip">如未自动打开客户端，可点击上方按钮唤起，或复制授权码粘贴到客户端</div>
    </div>
    <script>
        window.location.href = "{deep_link}";
        function copyCode() {{
            navigator.clipboard.writeText("{clean_code}").then(function() {{
                var btn = document.getElementById("copy-btn");
                btn.innerText = "已复制！";
                setTimeout(function() {{ btn.innerText = "复制授权码"; }}, 2000);
            }});
        }}
    </script>
</body>
</html>"#
        )
    } else {
        let err_msg = query
            .error_description
            .as_deref()
            .or(query.error.as_deref())
            .unwrap_or("未获得有效授权码");
        format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
    <meta charset="UTF-8">
    <title>Prism - 授权失败</title>
    <style>
        body {{ font-family: sans-serif; background: #090d16; color: #e2e8f0; display: flex; align-items: center; justify-content: center; min-height: 100vh; margin: 0; }}
        .card {{ background: #111827; border: 1px solid #ef4444; border-radius: 16px; padding: 32px; max-width: 480px; width: 100%; text-align: center; }}
        h2 {{ color: #ef4444; }}
        p {{ color: #94a3b8; }}
    </style>
</head>
<body>
    <div class="card">
        <h2>GitHub 授权失败</h2>
        <p>{err_msg}</p>
    </div>
</body>
</html>"#
        )
    };

    (
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        content,
    )
}

#[derive(Debug, Deserialize)]
pub struct GitHubExchangeRequest {
    pub code: String,
    #[serde(default)]
    pub device_id: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct GitHubExchangeResponse {
    pub token: String,
    pub user: crate::prism::auth::UserRecord,
    pub token_id: String,
    pub expires_at_unix_ms: Option<u64>,
}

async fn auth_github_exchange(
    State(st): State<Arc<AdminState>>,
    Json(payload): Json<GitHubExchangeRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("auth manager not configured")))?;

    let (user, raw_token, token_record) = am
        .exchange_code(&payload.code, payload.device_id.as_deref())
        .await
        .map_err(ApiError::bad_request)?;

    Ok((
        StatusCode::OK,
        Json(GitHubExchangeResponse {
            token: raw_token,
            user,
            token_id: token_record.id,
            expires_at_unix_ms: token_record.expires_at_unix_ms,
        }),
    ))
}

#[derive(Debug, Serialize)]
pub struct AuthSessionResponse {
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<String>,
    pub service_rules: Vec<String>,
    pub is_admin: bool,
}

async fn auth_session(headers: HeaderMap, State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    if let Some(token) = extract_bearer_token(&headers) {
        if let Some(ref am) = st.auth_manager {
            if let Some(ident) = am.verify_token(&token).await {
                let user = am.get_user(&ident.user_id).await;
                return (
                    StatusCode::OK,
                    Json(AuthSessionResponse {
                        authenticated: true,
                        user_id: Some(ident.user_id),
                        username: Some(ident.username),
                        display_name: user.as_ref().and_then(|u| u.display_name.clone()),
                        avatar_url: user.as_ref().and_then(|u| u.avatar_url.clone()),
                        role: Some(format!("{:?}", ident.role).to_lowercase()),
                        service_rules: ident.service_rules,
                        is_admin: ident.is_admin,
                    }),
                );
            }
        }
        if let Some(ref panel_token) = st.auth.panel_token {
            if token.trim() == panel_token.trim() {
                return (
                    StatusCode::OK,
                    Json(AuthSessionResponse {
                        authenticated: true,
                        user_id: Some("panel_admin".to_string()),
                        username: Some("Panel Admin".to_string()),
                        display_name: Some("System Admin".to_string()),
                        avatar_url: None,
                        role: Some("admin".to_string()),
                        service_rules: vec!["*".to_string()],
                        is_admin: true,
                    }),
                );
            }
        }
    }

    (
        StatusCode::OK,
        Json(AuthSessionResponse {
            authenticated: false,
            user_id: None,
            username: None,
            display_name: None,
            avatar_url: None,
            role: None,
            service_rules: vec![],
            is_admin: false,
        }),
    )
}

async fn auth_list_tokens(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::not_found("auth manager not configured"))?;
    let tokens = am.list_tokens(None).await;
    Ok((StatusCode::OK, Json(tokens)))
}

#[derive(Debug, Deserialize)]
pub struct CreateTokenRequest {
    pub user_id: String,
    pub name: String,
    pub expires_in_days: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct CreateTokenResponse {
    pub raw_token: String,
    pub token: crate::prism::auth::TokenRecord,
}

async fn auth_create_token(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    Json(payload): Json<CreateTokenRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::not_found("auth manager not configured"))?;
    let (raw_token, token) = am
        .create_client_token(&payload.user_id, &payload.name, payload.expires_in_days)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((
        StatusCode::OK,
        Json(CreateTokenResponse { raw_token, token }),
    ))
}

async fn auth_revoke_token(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(token_id): AxumPath<String>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::not_found("auth manager not configured"))?;
    let revoked = am.revoke_token(&token_id).await;
    Ok((
        StatusCode::OK,
        Json(serde_json::json!({ "revoked": revoked })),
    ))
}

async fn managed_users(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::not_found("auth manager not configured"))?;
    let users = am.list_users().await;
    Ok((StatusCode::OK, Json(users)))
}

#[derive(Debug, Deserialize)]
pub struct UpdateUserRequest {
    pub role: crate::prism::auth::UserRole,
    #[serde(default)]
    pub service_rules: Vec<String>,
}

async fn put_managed_user(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
    AxumPath(user_id): AxumPath<String>,
    Json(payload): Json<UpdateUserRequest>,
) -> Result<impl IntoResponse, ApiError> {
    require_panel_auth(&headers, &st).await?;
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::not_found("auth manager not configured"))?;

    let mut user = am
        .get_user(&user_id)
        .await
        .ok_or_else(|| ApiError::not_found("user not found"))?;

    user.role = payload.role;
    user.service_rules = payload.service_rules;
    am.upsert_user(user.clone())
        .await
        .map_err(ApiError::bad_request)?;

    Ok((StatusCode::OK, Json(user)))
}

#[async_trait]
impl AdminControl for AdminState {
    fn features(&self) -> u64 {
        let mut bits =
            FEATURE_RPC | FEATURE_EVENTS | FEATURE_PANEL | FEATURE_MIDDLEWARE | FEATURE_AUTH;
        if self.management.is_some() {
            bits |= FEATURE_MANAGED;
        }
        bits
    }

    fn auth_enabled(&self) -> bool {
        self.auth_manager.is_some() || self.auth.panel_token.is_some()
    }

    fn event_watches(&self) -> AdminEventWatches {
        AdminEventWatches {
            sessions: Some(self.sessions.clone()),
            optimizer: Some(self.optimizer.clone()),
            manager: self.tunnel.clone(),
            reload: Some(self.reload_tx.subscribe()),
        }
    }

    async fn authenticate(&self, token: &str) -> Result<AuthIdentity, AdminError> {
        let token = token.trim();
        if token.is_empty() {
            return Err(AdminError::unauthorized("missing token"));
        }
        if let Some(ref am) = self.auth_manager
            && let Some(ident) = am.verify_token(token).await
        {
            return Ok(ident);
        }
        if let Some(expected) = self.auth.panel_token.as_ref()
            && token == expected.trim()
        {
            return Ok(panel_admin_identity());
        }
        if !self.auth_enabled() {
            return Ok(panel_admin_identity());
        }
        Err(AdminError::unauthorized("invalid bearer token"))
    }

    async fn dispatch(
        &self,
        method: AdminMethod,
        ctx: &AdminCallContext,
    ) -> Result<AdminPayload, AdminError> {
        self.dispatch_method(method, ctx).await
    }
}

fn panel_admin_identity() -> AuthIdentity {
    AuthIdentity {
        user_id: "panel_admin".into(),
        username: "Panel Admin".into(),
        role: crate::prism::auth::UserRole::Admin,
        service_rules: vec!["*".into()],
        is_admin: true,
    }
}

impl AdminState {
    async fn dispatch_method(
        &self,
        method: AdminMethod,
        ctx: &AdminCallContext,
    ) -> Result<AdminPayload, AdminError> {
        if !ctx.has_feature(method.feature()) {
            return Err(AdminError::unavailable("method not negotiated"));
        }
        match method {
            AdminMethod::Health => Ok(AdminPayload::Health { ok: true }),
            AdminMethod::ConfigPath => Ok(AdminPayload::ConfigPath {
                path: self.config_path.display().to_string(),
            }),
            AdminMethod::Connections => Ok(AdminPayload::Connections(self.sessions.snapshot())),
            AdminMethod::TunnelServices => {
                let snap = if let Some(mgr) = &self.tunnel {
                    mgr.snapshot_services().await
                } else {
                    Vec::new()
                };
                Ok(AdminPayload::TunnelServices(snap))
            }
            AdminMethod::OptimizerStats => {
                let (global, services) = self.optimizer.snapshot();
                Ok(AdminPayload::Optimizer { global, services })
            }
            AdminMethod::Reload => {
                let mut next = (*self.reload_tx.borrow()).clone();
                next.next();
                let seq = next.seq;
                let _ = self.reload_tx.send(next);
                Ok(AdminPayload::Reload { seq })
            }
            AdminMethod::AuthProviders => Ok(self.rpc_auth_providers()),
            AdminMethod::AuthGithubLogin => self.rpc_github_login(),
            AdminMethod::AuthGithubExchange { code, device_id } => {
                self.rpc_github_exchange(&code, device_id.as_deref()).await
            }
            AdminMethod::AuthSession => Ok(self.rpc_auth_session(ctx).await),
            AdminMethod::AuthListTokens => {
                let am = self.auth_manager.as_ref().ok_or_else(|| {
                    AdminError::not_found("auth manager not configured")
                })?;
                Ok(AdminPayload::AuthTokens(am.list_tokens(None).await))
            }
            AdminMethod::AuthCreateToken {
                user_id,
                name,
                expires_in_days,
            } => {
                let am = self.auth_manager.as_ref().ok_or_else(|| {
                    AdminError::not_found("auth manager not configured")
                })?;
                let (raw_token, token) = am
                    .create_client_token(&user_id, &name, expires_in_days)
                    .await
                    .map_err(|e| AdminError::bad_request(e.to_string()))?;
                Ok(AdminPayload::AuthCreateToken { raw_token, token })
            }
            AdminMethod::AuthRevokeToken { token_id } => {
                let am = self.auth_manager.as_ref().ok_or_else(|| {
                    AdminError::not_found("auth manager not configured")
                })?;
                let revoked = am.revoke_token(&token_id).await;
                Ok(AdminPayload::AuthRevokeToken { revoked })
            }
            AdminMethod::ManagedStatus => {
                let management = self.management.as_ref().ok_or_else(|| {
                    AdminError::not_found("management API not enabled")
                })?;
                Ok(AdminPayload::ManagedStatus(management.status().await))
            }
            AdminMethod::ManagedNodes => {
                let management = self.management.as_ref().ok_or_else(|| {
                    AdminError::not_found("management API not enabled")
                })?;
                Ok(AdminPayload::ManagedNodes(management.list_nodes().await))
            }
            AdminMethod::ManagedNode { node_id } => {
                let management = self.management.as_ref().ok_or_else(|| {
                    AdminError::not_found("management API not enabled")
                })?;
                let node = management
                    .get_node(&node_id)
                    .await
                    .ok_or_else(|| AdminError::not_found("managed node not found"))?;
                Ok(AdminPayload::ManagedNode(node))
            }
            AdminMethod::ManagedNodeConfig { node_id } => {
                let management = self.management.as_ref().ok_or_else(|| {
                    AdminError::not_found("management API not enabled")
                })?;
                let node = management
                    .get_node_config(&node_id)
                    .await
                    .ok_or_else(|| AdminError::not_found("managed node not found"))?;
                Ok(AdminPayload::ManagedNodeConfig(node))
            }
            AdminMethod::PutManagedNodeConfig {
                node_id,
                desired_config,
            } => {
                let management = self.management.as_ref().ok_or_else(|| {
                    AdminError::not_found("management API not enabled")
                })?;
                let response = management
                    .set_desired_config(&node_id, desired_config)
                    .await
                    .map_err(|e| AdminError::bad_request(e.to_string()))?;
                Ok(AdminPayload::ManagedNodeConfig(response))
            }
            AdminMethod::ManagedUsers => {
                let am = self.auth_manager.as_ref().ok_or_else(|| {
                    AdminError::not_found("auth manager not configured")
                })?;
                Ok(AdminPayload::ManagedUsers(am.list_users().await))
            }
            AdminMethod::PutManagedUser {
                user_id,
                role,
                service_rules,
            } => {
                let am = self.auth_manager.as_ref().ok_or_else(|| {
                    AdminError::not_found("auth manager not configured")
                })?;
                let mut user = am
                    .get_user(&user_id)
                    .await
                    .ok_or_else(|| AdminError::not_found("user not found"))?;
                user.role = role;
                user.service_rules = service_rules;
                am.upsert_user(user.clone())
                    .await
                    .map_err(|e| AdminError::bad_request(e.to_string()))?;
                Ok(AdminPayload::ManagedUser(user))
            }
            AdminMethod::ListMiddlewares => self.rpc_list_middlewares(),
            AdminMethod::MiddlewareSchema { name } => self.rpc_middleware_schema(&name),
            AdminMethod::MiddlewareConfig { name } => self.rpc_middleware_config(&name),
            AdminMethod::PutMiddlewareConfig { name, config } => {
                let base_name = name.strip_suffix(".wat").unwrap_or(&name).trim().to_string();
                if let Some(ref storage) = self.storage {
                    storage
                        .save_middleware_config(&base_name, &config)
                        .map_err(|e| AdminError::bad_request(e.to_string()))?;
                }
                crate::prism::middleware::set_dynamic_middleware_config(&base_name, config.clone());
                Ok(AdminPayload::MiddlewareConfigUpdated {
                    status: "ok".into(),
                    name: base_name,
                    config,
                })
            }
            AdminMethod::ResetMiddlewareConfig { name } => {
                let base_name = name.strip_suffix(".wat").unwrap_or(&name).trim().to_string();
                if let Some(ref storage) = self.storage {
                    let _ = storage.delete_middleware_config(&base_name);
                }
                crate::prism::middleware::reset_dynamic_middleware_config(&base_name);
                if let Some(wat) = crate::prism::middleware::get_default_middleware_wat(&base_name)
                {
                    let engine = wasmtime::Engine::default();
                    if let Ok((_, Some(schema))) = crate::prism::middleware::compile_module_from_wat(
                        &engine,
                        &base_name,
                        wat.as_bytes(),
                    ) {
                        let mut defaults = std::collections::HashMap::new();
                        for f in schema.fields {
                            defaults.insert(f.key, f.default_value);
                        }
                        crate::prism::middleware::broadcast_session_config_update(
                            &base_name, &defaults,
                        );
                    }
                }
                Ok(AdminPayload::MiddlewareConfigReset {
                    status: "ok".into(),
                    name: base_name,
                    reset: true,
                })
            }
            AdminMethod::Subscribe { .. } | AdminMethod::Authenticate { .. } => {
                Err(AdminError::internal("handled by control channel"))
            }
        }
    }

    fn rpc_auth_providers(&self) -> AdminPayload {
        let (github_enabled, github_client_id, mode, providers) = if let Some(ref am) =
            self.auth_manager
        {
            let gh = am.github_config();
            let gh_enabled = gh.is_some();
            let gh_client_id = gh.map(|g| g.client_id.clone());
            let mode_str = am.auth_mode().to_string();
            let mut providers = Vec::new();
            if gh_enabled && mode_str != "token" {
                providers.push("github".to_string());
            }
            (gh_enabled, gh_client_id, mode_str, providers)
        } else {
            (false, None, "token".to_string(), Vec::new())
        };
        AdminPayload::AuthProviders {
            github_enabled,
            github_client_id,
            mode,
            providers,
        }
    }

    fn rpc_github_login(&self) -> Result<AdminPayload, AdminError> {
        let am = self
            .auth_manager
            .as_ref()
            .ok_or_else(|| AdminError::bad_request("auth manager not configured"))?;
        let gh = am
            .github_config()
            .ok_or_else(|| AdminError::bad_request("GitHub OAuth not enabled"))?;
        let mut url = format!(
            "https://github.com/login/oauth/authorize?client_id={}&scope=read:user",
            gh.client_id
        );
        if let Some(ref r) = gh.redirect_uri {
            url.push_str(&format!("&redirect_uri={r}"));
        }
        Ok(AdminPayload::AuthGithubLogin { url })
    }

    async fn rpc_github_exchange(
        &self,
        code: &str,
        device_id: Option<&str>,
    ) -> Result<AdminPayload, AdminError> {
        let am = self
            .auth_manager
            .as_ref()
            .ok_or_else(|| AdminError::bad_request("auth manager not configured"))?;
        let (user, raw_token, token_record) = am
            .exchange_code(code, device_id)
            .await
            .map_err(|e| AdminError::bad_request(e.to_string()))?;
        Ok(AdminPayload::AuthGithubExchange {
            token: raw_token,
            user,
            token_id: token_record.id,
            expires_at_unix_ms: token_record.expires_at_unix_ms,
        })
    }

    async fn rpc_auth_session(&self, ctx: &AdminCallContext) -> AdminPayload {
        if let Some(ident) = &ctx.identity {
            let user = if let Some(ref am) = self.auth_manager {
                am.get_user(&ident.user_id).await
            } else {
                None
            };
            return AdminPayload::AuthSession(AuthSessionSnapshot {
                authenticated: true,
                user_id: Some(ident.user_id.clone()),
                username: Some(ident.username.clone()),
                display_name: user.as_ref().and_then(|u| u.display_name.clone()),
                avatar_url: user.as_ref().and_then(|u| u.avatar_url.clone()),
                role: Some(format!("{:?}", ident.role).to_lowercase()),
                service_rules: ident.service_rules.clone(),
                is_admin: ident.is_admin,
            });
        }
        AdminPayload::AuthSession(AuthSessionSnapshot::unauthenticated())
    }

    fn rpc_list_middlewares(&self) -> Result<AdminPayload, AdminError> {
        let items = do_list_middlewares().map_err(|e| AdminError::bad_request(e))?;
        Ok(AdminPayload::Middlewares(items))
    }

    fn rpc_middleware_schema(&self, name: &str) -> Result<AdminPayload, AdminError> {
        let base_name = name.strip_suffix(".wat").unwrap_or(name).trim();
        let wat = crate::prism::middleware::get_default_middleware_wat(base_name)
            .ok_or_else(|| AdminError::not_found(format!("middleware '{name}' not found")))?;
        let engine = wasmtime::Engine::default();
        let (_, schema) =
            crate::prism::middleware::compile_module_from_wat(&engine, base_name, wat.as_bytes())
                .map_err(|e| AdminError::bad_request(e.to_string()))?;
        let s = schema.ok_or_else(|| {
            AdminError::not_found(format!("middleware '{name}' has no component schema"))
        })?;
        Ok(AdminPayload::MiddlewareSchema(s))
    }

    fn rpc_middleware_config(&self, name: &str) -> Result<AdminPayload, AdminError> {
        let base_name = name.strip_suffix(".wat").unwrap_or(name).trim();
        let wat = crate::prism::middleware::get_default_middleware_wat(base_name)
            .ok_or_else(|| AdminError::not_found(format!("middleware '{name}' not found")))?;
        let engine = wasmtime::Engine::default();
        let (_, schema) =
            crate::prism::middleware::compile_module_from_wat(&engine, base_name, wat.as_bytes())
                .map_err(|e| AdminError::bad_request(e.to_string()))?;
        let mut effective = std::collections::HashMap::new();
        if let Some(ref s) = schema {
            for f in &s.fields {
                effective.insert(f.key.clone(), f.default_value.clone());
            }
        }
        if let Some(overrides) = crate::prism::middleware::get_dynamic_middleware_config(base_name) {
            for (k, v) in overrides {
                effective.insert(k, v);
            }
        }
        Ok(AdminPayload::MiddlewareConfig(effective))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_post_middleware_data_success_and_retrieval() {
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let state = AdminState {
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            optimizer: Arc::new(telemetry::OptimizerStatsRegistry::new()),
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth {
                panel_token: Some("secret123".to_string()),
                worker_token: None,
                ..Default::default()
            },
            management: None,
            worker: None,
            client: None,
            auth_manager: None,
            storage: None,
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let mut rx = shutdown_rx;
                    while rx.changed().await.is_ok() {
                        if *rx.borrow() {
                            break;
                        }
                    }
                })
                .await
                .ok();
        });

        use base64::Engine;
        let test_payload = b"test-rsa-private-key-der-bytes";
        let b64_payload = base64::engine::general_purpose::STANDARD.encode(test_payload);

        let client = reqwest::Client::new();
        let resp = client
            .post(format!("http://{addr}/middlewares/minecraft/data"))
            .header("Authorization", "Bearer secret123")
            .json(&MiddlewareDataPayload {
                port: Some(25565),
                data: b64_payload,
            })
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        let resp_json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(resp_json["status"], "ok");
        assert_eq!(resp_json["name"], "minecraft");
        assert_eq!(resp_json["bytes_received"], test_payload.len());

        // Verify retrieval in middleware store
        let retrieved =
            crate::prism::middleware::get_injected_middleware_data("minecraft", Some(25565));
        assert_eq!(retrieved, Some(test_payload.to_vec()));

        let _ = shutdown_tx.send(true);
    }

    #[tokio::test]
    async fn test_stats_optimizer_endpoint() {
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let optimizer = Arc::new(telemetry::OptimizerStatsRegistry::new());

        // Populate some sample optimizer stats
        let svc_stats = optimizer.service("gto");
        svc_stats.add_raw_bytes(1000);
        svc_stats.add_wire_bytes(200);
        svc_stats.inc_urgent();
        svc_stats.inc_timer();

        let global_stats = optimizer.global();
        global_stats.add_direction_raw_bytes(
            tunnel::optimizer::TrafficDirection::Uplink,
            1000,
            tunnel::optimizer::unix_ms(),
        );
        global_stats.record_batch(
            tunnel::optimizer::TrafficDirection::Uplink,
            200,
            15_000,
            5_000,
            tunnel::optimizer::unix_ms(),
        );

        let state = AdminState {
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            optimizer,
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth::default(),
            management: None,
            worker: None,
            client: None,
            auth_manager: None,
            storage: None,
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(wait_shutdown(shutdown_rx))
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();
        let resp = client
            .get(format!("http://{addr}/stats/optimizer"))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let resp_json: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(resp_json["global"]["raw_bytes"], 1000);
        assert_eq!(resp_json["global"]["wire_bytes"], 200);
        assert_eq!(resp_json["global"]["saved_bytes"], 800);
        assert_eq!(resp_json["global"]["saved_ratio"], 0.8);
        // Itemised accounting: measured link rate, per-direction penalties, net floor.
        assert!(resp_json["global"]["link_rate_bps"].is_number());
        let up = &resp_json["global"]["uplink"];
        assert_eq!(up["batching_penalty_ms"], 15.0);
        assert_eq!(up["compression_penalty_ms"], 5.0);
        let (gain, net) = (
            up["transfer_gain_ms"].as_f64().unwrap(),
            up["net_gain_ms"].as_f64().unwrap(),
        );
        assert!((gain - net - 20.0).abs() < 1e-6, "gain {gain}, net {net}");
        assert_eq!(resp_json["global"]["net_gain_ms"], up["net_gain_ms"]);
        assert!(resp_json["global"]["window"].is_object());
        assert_eq!(resp_json["services"]["gto"]["raw_bytes"], 1000);
        assert_eq!(resp_json["services"]["gto"]["urgent_batches"], 1);

        let _ = shutdown_tx.send(true);
    }

    #[tokio::test]
    async fn test_client_endpoints() {
        crate::prism::logging::init_desktop_or_test_subscriber();
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let client_controller = Arc::new(tunnel::client::ClientController::new(None));

        let state = AdminState {
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            optimizer: Arc::new(telemetry::OptimizerStatsRegistry::new()),
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth::default(),
            management: None,
            worker: None,
            client: Some(client_controller),
            auth_manager: None,
            storage: None,
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(wait_shutdown(shutdown_rx))
                .await
                .unwrap();
        });

        let http = reqwest::Client::new();

        // 1. Check status when idle
        let resp = http
            .get(format!("http://{addr}/client/status"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let status: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(status["running"], false);
        assert_eq!(status["state"], "idle");

        // 2. Start client with dummy port
        let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let free_port = l.local_addr().unwrap().port();
        drop(l);

        let start_req = serde_json::json!({
            "server_addr": "127.0.0.1:9999",
            "transport": "tcp",
            "listen_addr": format!("127.0.0.1:{free_port}"),
            "fake_lan_broadcast": false,
        });
        let resp = http
            .post(format!("http://{addr}/client/start"))
            .json(&start_req)
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        // 3. Check status when running
        let resp = http
            .get(format!("http://{addr}/client/status"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let status: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(status["running"], true);
        assert_eq!(status["server_addr"], "127.0.0.1:9999");

        // 4. Stop client
        let resp = http
            .post(format!("http://{addr}/client/stop"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        let resp = http
            .get(format!("http://{addr}/client/status"))
            .send()
            .await
            .unwrap();
        let status: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(status["running"], false);

        // 5. Test logs endpoint
        let resp = http
            .get(format!("http://{addr}/client/logs?limit=50"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let logs: Vec<serde_json::Value> = resp.json().await.unwrap();
        assert!(!logs.is_empty());

        // 6. Clear logs
        let resp = http
            .delete(format!("http://{addr}/client/logs"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);

        let resp = http
            .get(format!("http://{addr}/client/logs"))
            .send()
            .await
            .unwrap();
        let logs: Vec<serde_json::Value> = resp.json().await.unwrap();
        assert!(
            !logs.iter().any(|l| {
                l.get("message")
                    .and_then(|m| m.as_str())
                    .unwrap_or("")
                    .contains("127.0.0.1:9999")
            }),
            "logs before clear should not be present"
        );

        let _ = shutdown_tx.send(true);
    }

    #[tokio::test]
    async fn test_root_info_and_frontend_deprecation() {
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let state = AdminState {
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            optimizer: Arc::new(telemetry::OptimizerStatsRegistry::new()),
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth::default(),
            management: None,
            worker: None,
            client: None,
            auth_manager: None,
            storage: None,
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(wait_shutdown(shutdown_rx))
                .await
                .unwrap();
        });

        let client = reqwest::Client::new();

        // Root returns JSON info indicating frontend is desktop-only
        let root_resp = client.get(format!("http://{addr}/")).send().await.unwrap();
        assert_eq!(root_resp.status(), reqwest::StatusCode::OK);
        let root_json: serde_json::Value = root_resp.json().await.unwrap();
        assert_eq!(root_json["service"], "prism-admin-api");
        assert_eq!(root_json["frontend"], "desktop-only");

        // Non-API route returns 404 without HTML fallback
        let resp = client
            .get(format!("http://{addr}/client"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

        let _ = shutdown_tx.send(true);
    }

    #[tokio::test]
    async fn test_client_config_and_stats_persistence_endpoints() {
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let client_controller = Arc::new(tunnel::client::ClientController::new(None));

        let rand_id = rand::random::<u64>();
        let db_path = std::env::temp_dir().join(format!("prism_admin_test_{}.db", rand_id));
        let storage = Arc::new(crate::prism::storage::StorageEngine::open(&db_path).unwrap());

        let state = AdminState {
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            optimizer: Arc::new(telemetry::OptimizerStatsRegistry::new()),
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth::default(),
            management: None,
            worker: None,
            client: Some(client_controller),
            auth_manager: None,
            storage: Some(storage.clone()),
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(wait_shutdown(shutdown_rx))
                .await
                .unwrap();
        });

        let http = reqwest::Client::new();

        // 1. Initial /client/config snapshot
        let resp = http
            .get(format!("http://{addr}/client/config"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let cfg_resp: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(cfg_resp["active_profile_id"], serde_json::Value::Null);
        assert_eq!(cfg_resp["profiles"].as_array().unwrap().len(), 0);

        // 2. Start client with specific remote and profile metadata -> should auto-persist
        let start_resp = http
            .post(format!("http://{addr}/client/start"))
            .json(&serde_json::json!({
                "server_addr": "relay.mycustomserver.net:7000",
                "transport": "quic",
                "auth_token": "token123",
                "listen_addr": "127.0.0.1:25565",
                "profile_id": "prof-custom-1",
                "profile_name": "My Custom Realm"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(start_resp.status(), reqwest::StatusCode::OK);

        // 3. Verify /client/config reflects newly persisted active profile and config
        let resp2 = http
            .get(format!("http://{addr}/client/config"))
            .send()
            .await
            .unwrap();
        let cfg_resp2: serde_json::Value = resp2.json().await.unwrap();
        assert_eq!(cfg_resp2["active_profile_id"], "prof-custom-1");
        assert_eq!(
            cfg_resp2["active_config"]["server_addr"],
            "relay.mycustomserver.net:7000"
        );
        assert_eq!(
            cfg_resp2["active_config"]["profile_name"],
            "My Custom Realm"
        );
        let profiles = cfg_resp2["profiles"].as_array().unwrap();
        assert_eq!(profiles.len(), 1);
        assert_eq!(profiles[0]["id"], "prof-custom-1");
        assert_eq!(profiles[0]["auth_token"], "token123");
        assert!(!cfg_resp2["device_id"].as_str().unwrap_or("").is_empty());

        // Starting a second profile must insert rather than skip when others already exist.
        let start2 = http
            .post(format!("http://{addr}/client/start"))
            .json(&serde_json::json!({
                "server_addr": "relay-two.example:7000",
                "transport": "tcp",
                "auth_token": "token-two",
                "listen_addr": "127.0.0.1:25566",
                "profile_id": "prof-custom-2",
                "profile_name": "Second Realm"
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(start2.status(), reqwest::StatusCode::OK);
        let cfg_resp3: serde_json::Value = http
            .get(format!("http://{addr}/client/config"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(cfg_resp3["profiles"].as_array().unwrap().len(), 2);
        assert_eq!(cfg_resp3["active_profile_id"], "prof-custom-2");
        assert_eq!(cfg_resp3["active_config"]["auth_token"], "token-two");

        // Partial config save (no profile_name) must not fail and must keep the token.
        let patch_resp = http
            .post(format!("http://{addr}/client/config"))
            .json(&serde_json::json!({
                "active_profile_id": "prof-custom-2",
                "active_config": {
                    "auth_token": "token-rotated",
                    "transport": "quic"
                }
            }))
            .send()
            .await
            .unwrap();
        assert_eq!(patch_resp.status(), reqwest::StatusCode::OK);
        let cfg_resp4: serde_json::Value = http
            .get(format!("http://{addr}/client/config"))
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(cfg_resp4["active_config"]["auth_token"], "token-rotated");
        assert_eq!(cfg_resp4["active_config"]["profile_name"], "Second Realm");
        assert_eq!(cfg_resp4["active_config"]["transport"], "quic");

        // 4. Check status includes active_profile_id and cumulative_stats
        let status_resp = http
            .get(format!("http://{addr}/client/status"))
            .send()
            .await
            .unwrap();
        let status_json: serde_json::Value = status_resp.json().await.unwrap();
        assert_eq!(status_json["active_profile_id"], "prof-custom-2");
        assert!(status_json.get("cumulative_stats").is_some());

        // 5. Stop client and test /client/stats reset
        let stop_resp = http
            .post(format!("http://{addr}/client/stop"))
            .send()
            .await
            .unwrap();
        assert_eq!(stop_resp.status(), reqwest::StatusCode::OK);

        let reset_resp = http
            .delete(format!("http://{addr}/client/stats"))
            .send()
            .await
            .unwrap();
        assert_eq!(reset_resp.status(), reqwest::StatusCode::OK);

        let _ = shutdown_tx.send(true);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn test_middleware_config_api_endpoints() {
        crate::prism::middleware::reset_dynamic_middleware_config("minecraft");
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);

        let rand_val = rand::random::<u64>();
        let db_path = std::env::temp_dir().join(format!("prism_mw_api_test_{rand_val}.db"));
        let storage = Arc::new(crate::prism::storage::StorageEngine::open(&db_path).unwrap());

        let state = AdminState {
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            optimizer: Arc::new(telemetry::OptimizerStatsRegistry::new()),
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth {
                panel_token: Some("secret123".to_string()),
                worker_token: None,
                ..Default::default()
            },
            management: None,
            worker: None,
            client: None,
            auth_manager: None,
            storage: Some(storage.clone()),
        };

        let app = build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let mut rx = shutdown_rx;
                    while rx.changed().await.is_ok() {
                        if *rx.borrow() {
                            break;
                        }
                    }
                })
                .await
                .ok();
        });

        let http = reqwest::Client::new();

        // 1. GET /middlewares
        let resp = http
            .get(format!("http://{addr}/middlewares"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let items: Vec<serde_json::Value> = resp.json().await.unwrap();
        let mc = items.iter().find(|i| i["name"] == "minecraft").unwrap();
        assert!(mc["schema"].get("fields").is_some());

        // 2. GET /middlewares/minecraft/schema
        let resp = http
            .get(format!("http://{addr}/middlewares/minecraft/schema"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let schema: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(schema["name"], "minecraft");

        // 3. GET /middlewares/minecraft/config
        let resp = http
            .get(format!("http://{addr}/middlewares/minecraft/config"))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let cfg: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(cfg["recompress-threshold"], 256);

        // 4. PUT /middlewares/minecraft/config
        let mut update = std::collections::HashMap::new();
        update.insert("recompress-threshold", serde_json::json!(1024));
        update.insert("deflate-level", serde_json::json!(6));

        let put_resp = http
            .put(format!("http://{addr}/middlewares/minecraft/config"))
            .header("Authorization", "Bearer secret123")
            .json(&update)
            .send()
            .await
            .unwrap();
        assert_eq!(put_resp.status(), reqwest::StatusCode::OK);

        // Verify effective config is updated
        let resp = http
            .get(format!("http://{addr}/middlewares/minecraft/config"))
            .send()
            .await
            .unwrap();
        let updated_cfg: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(updated_cfg["recompress-threshold"], 1024);
        assert_eq!(updated_cfg["deflate-level"], 6);

        // Verify persisted to SQLite DB
        let db_cfg = storage
            .load_middleware_config("minecraft")
            .unwrap()
            .unwrap();
        assert_eq!(
            db_cfg.get("recompress-threshold").unwrap(),
            &serde_json::json!(1024)
        );

        // 5. POST /middlewares/minecraft/config/reset
        let reset_resp = http
            .post(format!("http://{addr}/middlewares/minecraft/config/reset"))
            .header("Authorization", "Bearer secret123")
            .send()
            .await
            .unwrap();
        assert_eq!(reset_resp.status(), reqwest::StatusCode::OK);

        // Verify DB cleared and effective config reset to defaults
        assert!(
            storage
                .load_middleware_config("minecraft")
                .unwrap()
                .is_none()
        );
        let resp = http
            .get(format!("http://{addr}/middlewares/minecraft/config"))
            .send()
            .await
            .unwrap();
        let reset_cfg: serde_json::Value = resp.json().await.unwrap();
        assert_eq!(reset_cfg["recompress-threshold"], 256);

        let _ = shutdown_tx.send(true);
        let _ = std::fs::remove_file(db_path);
    }

    #[tokio::test]
    async fn test_auth_github_callback_endpoint() {
        let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
        let sessions = Arc::new(telemetry::SessionRegistry::new());
        let (reload_tx, _) = tokio::sync::watch::channel(telemetry::ReloadSignal::new());
        let state = AdminState {
            sessions: sessions.clone(),
            optimizer: Arc::new(telemetry::OptimizerStatsRegistry::new()),
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth::default(),
            management: None,
            worker: None,
            client: None,
            auth_manager: None,
            storage: None,
        };

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let app = build_router(state);
            let _ = axum::serve(listener, app.into_make_service())
                .with_graceful_shutdown(wait_shutdown(shutdown_rx))
                .await;
        });

        let http = reqwest::Client::new();
        let resp = http
            .get(format!(
                "http://{addr}/auth/github/callback?code=mock_code_123&state=test_state"
            ))
            .send()
            .await
            .unwrap();

        assert_eq!(resp.status(), reqwest::StatusCode::OK);
        let text = resp.text().await.unwrap();
        assert!(text.contains("prism://auth/callback?code=mock_code_123&state=test_state"));
        assert!(text.contains("打开 Prism 客户端"));
        assert!(text.contains("mock_code_123"));

        let _ = shutdown_tx.send(true);
    }
}
