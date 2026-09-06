use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, Query, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use rust_embed::Embed;
use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tower_http::cors::CorsLayer;

use crate::prism::telemetry;
use crate::prism::{managed, tunnel};

#[derive(Embed)]
#[folder = "frontend-dist/"]
struct FrontendAssets;

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
    pub serve_frontend: bool,
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

pub(crate) fn build_router(state: AdminState) -> Router {
    let serve_frontend_enabled = state.serve_frontend;
    let shared = Arc::new(state);
    let mut router = Router::new()
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
        .route("/client/logs", get(client_logs).delete(client_clear_logs))
        .route("/middlewares/{name}/data", post(post_middleware_data))
        .route("/auth/providers", get(auth_providers))
        .route("/auth/device/code", post(auth_device_code))
        .route("/auth/device/poll", post(auth_device_poll))
        .route("/auth/github/login", get(auth_github_login))
        .route("/auth/github/callback", get(auth_github_callback))
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
        .route("/managed/users/{user_id}", put(put_managed_user));

    if serve_frontend_enabled {
        router = router.fallback(serve_frontend);
    }

    router.with_state(shared).layer(CorsLayer::permissive())
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

async fn serve_frontend(uri: Uri) -> impl IntoResponse {
    let path = uri.path().trim_start_matches('/');

    // Try the exact path first.
    if !path.is_empty() {
        if let Some(file) = FrontendAssets::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            return (
                StatusCode::OK,
                [(header::CONTENT_TYPE, mime.as_ref().to_string())],
                file.data.into_owned(),
            )
                .into_response();
        }
    }

    // SPA fallback: serve _shell.html for any unmatched route.
    if let Some(file) = FrontendAssets::get("_shell.html") {
        return (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "text/html; charset=utf-8".to_string())],
            file.data.into_owned(),
        )
            .into_response();
    }

    // No frontend assets embedded.
    (StatusCode::NOT_FOUND, "not found").into_response()
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

#[derive(Debug, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    if let Some(proj) = directories::ProjectDirs::from("com", "prism", "prism") {
        let dir = proj.config_dir();
        let _ = std::fs::create_dir_all(dir);
        dir.join("profiles.json")
    } else {
        PathBuf::from("profiles.json")
    }
}

async fn client_status(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    if let Some(ref client) = st.client {
        let status = client.status().await;
        (
            StatusCode::OK,
            Json(serde_json::to_value(status).unwrap_or_default()),
        )
            .into_response()
    } else {
        (
            StatusCode::OK,
            Json(
                serde_json::to_value(tunnel::client::ClientStatusSnapshot::default())
                    .unwrap_or_default(),
            ),
        )
            .into_response()
    }
}

async fn client_start(
    State(st): State<Arc<AdminState>>,
    Json(payload): Json<StartClientRequest>,
) -> impl IntoResponse {
    let Some(ref client) = st.client else {
        return (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "client controller not enabled" })),
        )
            .into_response();
    };

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
    };

    match client.start(cfg).await {
        Ok(()) => (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response(),
        Err(err) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": err.to_string() })),
        )
            .into_response(),
    }
}

async fn client_stop(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    if let Some(ref client) = st.client {
        client.stop().await;
        (StatusCode::OK, Json(serde_json::json!({ "ok": true }))).into_response()
    } else {
        (
            StatusCode::BAD_REQUEST,
            Json(serde_json::json!({ "error": "client controller not enabled" })),
        )
            .into_response()
    }
}

async fn client_get_profiles() -> impl IntoResponse {
    let path = profiles_path();
    if let Ok(data) = std::fs::read_to_string(&path) {
        if let Ok(profiles) = serde_json::from_str::<Vec<ClientProfile>>(&data) {
            return (StatusCode::OK, Json(profiles));
        }
    }
    (StatusCode::OK, Json(Vec::<ClientProfile>::new()))
}

async fn client_save_profiles(Json(profiles): Json<Vec<ClientProfile>>) -> impl IntoResponse {
    let path = profiles_path();
    if let Ok(data) = serde_json::to_string_pretty(&profiles) {
        if let Err(err) = std::fs::write(&path, data) {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": err.to_string() })),
            );
        }
    }
    (StatusCode::OK, Json(serde_json::json!({ "ok": true })))
}

#[derive(Debug, Deserialize)]
pub struct ClientLogsQuery {
    pub limit: Option<usize>,
}

async fn client_logs(
    State(st): State<Arc<AdminState>>,
    Query(query): Query<ClientLogsQuery>,
) -> impl IntoResponse {
    let limit = query.limit.unwrap_or(200).clamp(1, 1000);
    if let Some(ref client) = st.client {
        let logs = client.logs(limit).await;
        (StatusCode::OK, Json(logs)).into_response()
    } else {
        (
            StatusCode::OK,
            Json(Vec::<tunnel::client::ClientLogEntry>::new()),
        )
            .into_response()
    }
}

async fn client_clear_logs(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    if let Some(ref client) = st.client {
        client.clear_logs().await;
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
}

async fn auth_providers(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let (github_enabled, github_client_id, mode) = if let Some(ref am) = st.auth_manager {
        if let Some(gh) = am.github_config() {
            (true, Some(gh.client_id.clone()), "hybrid".to_string())
        } else {
            (false, None, "token".to_string())
        }
    } else {
        (false, None, "token".to_string())
    };

    (
        StatusCode::OK,
        Json(AuthProvidersResponse {
            github_enabled,
            github_client_id,
            mode,
        }),
    )
}

async fn auth_device_code(
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("auth manager not configured")))?;
    let resp = am
        .request_device_code()
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::OK, Json(resp)))
}

#[derive(Debug, Deserialize)]
pub struct DevicePollRequest {
    pub device_code: String,
}

async fn auth_device_poll(
    State(st): State<Arc<AdminState>>,
    Json(payload): Json<DevicePollRequest>,
) -> Result<impl IntoResponse, ApiError> {
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("auth manager not configured")))?;
    let result = am
        .poll_device_code(&payload.device_code)
        .await
        .map_err(ApiError::bad_request)?;
    Ok((StatusCode::OK, Json(result)))
}

async fn auth_github_login(
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
    Ok(axum::response::Redirect::temporary(&url))
}

#[derive(Debug, Deserialize)]
pub struct GitHubCallbackQuery {
    pub code: Option<String>,
    pub error: Option<String>,
}

async fn auth_github_callback(
    State(st): State<Arc<AdminState>>,
    axum::extract::Query(query): axum::extract::Query<GitHubCallbackQuery>,
) -> Result<impl IntoResponse, ApiError> {
    let am = st
        .auth_manager
        .as_ref()
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("auth manager not configured")))?;

    if let Some(err) = query.error {
        return Err(ApiError::unauthorized(&format!(
            "GitHub login error: {err}"
        )));
    }
    let code = query
        .code
        .ok_or_else(|| ApiError::bad_request(anyhow::anyhow!("missing code parameter")))?;

    let (user, raw_token, _) = am
        .exchange_web_code(&code)
        .await
        .map_err(ApiError::bad_request)?;

    let redirect_url = format!(
        "/login#token={}&user_id={}&username={}&role={:?}",
        raw_token, user.id, user.username, user.role
    );
    Ok(axum::response::Redirect::temporary(&redirect_url))
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
            },
            management: None,
            worker: None,
            client: None,
            auth_manager: None,
            serve_frontend: false,
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
        global_stats.add_raw_bytes(1000);
        global_stats.add_wire_bytes(200);
        global_stats.inc_urgent();
        global_stats.inc_timer();

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
            serve_frontend: false,
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
            serve_frontend: false,
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
        assert!(logs.is_empty());

        let _ = shutdown_tx.send(true);
    }

    #[tokio::test]
    async fn test_serve_frontend_flag() {
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
            serve_frontend: false,
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
            .get(format!("http://{addr}/client"))
            .send()
            .await
            .unwrap();

        // When serve_frontend is false, non-API route returns 404
        assert_eq!(resp.status(), reqwest::StatusCode::NOT_FOUND);

        let _ = shutdown_tx.send(true);
    }
}
