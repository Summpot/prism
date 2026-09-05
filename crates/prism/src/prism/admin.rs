use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    Json, Router,
    extract::{Path as AxumPath, State},
    http::{HeaderMap, StatusCode, Uri, header},
    response::IntoResponse,
    routing::{get, post, put},
};
use rust_embed::Embed;
use serde::Serialize;
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
    pub config_path: PathBuf,
    pub reload_tx: watch::Sender<telemetry::ReloadSignal>,
    pub tunnel: Option<Arc<tunnel::manager::Manager>>,
    pub auth: AdminAuth,
    pub management: Option<Arc<managed::ManagementPlane>>,
    pub worker: Option<Arc<managed::WorkerAgent>>,
}

#[allow(dead_code)]
pub async fn serve(addr: SocketAddr, state: AdminState) -> anyhow::Result<()> {
    let (tx, rx) = watch::channel(false);
    let _tx = tx;
    serve_with_shutdown(addr, state, rx).await
}

pub async fn serve_with_shutdown(
    addr: SocketAddr,
    state: AdminState,
    shutdown: watch::Receiver<bool>,
) -> anyhow::Result<()> {
    let app = build_router(state);

    tracing::info!(admin_addr = %addr, "admin: listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app)
        .with_graceful_shutdown(wait_shutdown(shutdown))
        .await?;

    Ok(())
}

pub(crate) fn build_router(state: AdminState) -> Router {
    let shared = Arc::new(state);
    Router::new()
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
        .route("/middlewares/{name}/data", post(post_middleware_data))
        .fallback(serve_frontend)
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
struct ReloadResponse {
    seq: u64,
}

async fn reload(
    headers: HeaderMap,
    State(st): State<Arc<AdminState>>,
) -> Result<impl IntoResponse, ApiError> {
    require_mutation_auth(&headers, &st)?;

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
    require_panel_auth(&headers, &st)?;
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
    require_panel_auth(&headers, &st)?;
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
    require_panel_auth(&headers, &st)?;
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
    require_panel_auth(&headers, &st)?;
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
    require_panel_auth(&headers, &st)?;
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
    require_mutation_auth(&headers, &st)?;

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

fn require_mutation_auth(headers: &HeaderMap, st: &AdminState) -> Result<(), ApiError> {
    if let Some(token) = st
        .auth
        .panel_token
        .as_ref()
        .or(st.auth.worker_token.as_ref())
    {
        require_bearer(headers, token)
    } else {
        Ok(())
    }
}

fn require_panel_auth(headers: &HeaderMap, st: &AdminState) -> Result<(), ApiError> {
    let token = st
        .auth
        .panel_token
        .as_ref()
        .ok_or_else(|| ApiError::not_found("panel auth not configured"))?;
    require_bearer(headers, token)
}

fn require_worker_auth(headers: &HeaderMap, st: &AdminState) -> Result<(), ApiError> {
    let token = st
        .auth
        .worker_token
        .as_ref()
        .ok_or_else(|| ApiError::not_found("worker auth not configured"))?;
    require_bearer(headers, token)
}

fn require_bearer(headers: &HeaderMap, expected: &str) -> Result<(), ApiError> {
    let Some(value) = headers.get(header::AUTHORIZATION) else {
        return Err(ApiError::unauthorized("missing Authorization header"));
    };
    let value = value
        .to_str()
        .map_err(|_| ApiError::unauthorized("invalid Authorization header"))?;
    let Some(token) = value.strip_prefix("Bearer ") else {
        return Err(ApiError::unauthorized("expected Bearer token"));
    };
    if token.trim() != expected {
        return Err(ApiError::unauthorized("invalid bearer token"));
    }
    Ok(())
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
            config_path: PathBuf::from("prism.toml"),
            reload_tx,
            tunnel: None,
            auth: AdminAuth {
                panel_token: Some("secret123".to_string()),
                worker_token: None,
            },
            management: None,
            worker: None,
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
}
