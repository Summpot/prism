use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use axum::{
    extract::State,
    http::StatusCode,
    response::IntoResponse,
    routing::{get, post},
    Json, Router,
};
use serde::Serialize;
use tokio::sync::watch;
use tower_http::{cors::CorsLayer, trace::TraceLayer};

use crate::prism::telemetry;
use crate::prism::tunnel;

#[derive(Clone)]
pub struct AdminState {
    pub metrics: telemetry::SharedMetrics,
    pub sessions: telemetry::SharedSessions,
    pub config_path: PathBuf,
    pub reload_tx: watch::Sender<telemetry::ReloadSignal>,
    pub tunnel: Option<Arc<tunnel::manager::Manager>>,
}

pub async fn serve(addr: SocketAddr, state: AdminState) -> anyhow::Result<()> {
    let shared = Arc::new(state);

    let app = Router::new()
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .route("/conns", get(conns))
        .route("/tunnel/services", get(tunnel_services))
        .route("/reload", post(reload))
        .route("/config", get(config))
        .with_state(shared)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http());

    tracing::info!(admin_addr = %addr, "admin: listening");

    let listener = tokio::net::TcpListener::bind(addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

#[derive(Debug, Serialize)]
struct HealthResponse {
    ok: bool,
}

async fn health() -> impl IntoResponse {
    (StatusCode::OK, Json(HealthResponse { ok: true }))
}

async fn metrics(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let snap = st.metrics.snapshot();
    (StatusCode::OK, Json(snap))
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

async fn reload(State(st): State<Arc<AdminState>>) -> impl IntoResponse {
    let mut next = (*st.reload_tx.borrow()).clone();
    next.next();
    let seq = next.seq;

    // Best-effort: if receivers are gone, still return OK.
    let _ = st.reload_tx.send(next);

    (StatusCode::OK, Json(ReloadResponse { seq }))
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
