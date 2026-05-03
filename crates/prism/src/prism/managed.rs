use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use anyhow::Context;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use tokio::sync::{Mutex, RwLock, watch};

use crate::prism::{app, config, proxy, router, telemetry};

const MANAGEMENT_STATE_SCHEMA_VERSION: u32 = 1;
const WORKER_STATE_FILE: &str = "managed-worker-state.json";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagementStatusResponse {
    pub state_path: String,
    pub node_count: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedNodeSnapshot {
    pub node_id: String,
    pub connection_mode: Option<config::ManagedConnectionMode>,
    pub agent_url: Option<String>,
    pub desired_revision: u64,
    pub applied_revision: u64,
    pub pending_restart: bool,
    pub restart_reasons: Vec<String>,
    pub last_apply_error: Option<String>,
    pub last_seen_unix_ms: u64,
    pub last_apply_attempt_unix_ms: u64,
    pub last_apply_success_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ManagedNodeConfigResponse {
    pub node: ManagedNodeSnapshot,
    pub desired_config: Option<config::ManagedConfigDocument>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PutManagedNodeConfigRequest {
    pub desired_config: config::ManagedConfigDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSyncRequest {
    pub node_id: String,
    pub connection_mode: config::ManagedConnectionMode,
    pub agent_url: Option<String>,
    #[serde(default)]
    pub applied_revision: u64,
    #[serde(default)]
    pub pending_restart: bool,
    #[serde(default)]
    pub restart_reasons: Vec<String>,
    pub last_apply_error: Option<String>,
    #[serde(default)]
    pub last_apply_attempt_unix_ms: u64,
    #[serde(default)]
    pub last_apply_success_unix_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerSyncResponse {
    pub desired_revision: u64,
    pub desired_config: Option<config::ManagedConfigDocument>,
    pub node: ManagedNodeSnapshot,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerConfigPushRequest {
    pub desired_revision: u64,
    pub desired_config: config::ManagedConfigDocument,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkerStatusSnapshot {
    pub node_id: String,
    pub connection_mode: config::ManagedConnectionMode,
    pub agent_url: Option<String>,
    pub desired_revision: u64,
    pub applied_revision: u64,
    pub pending_restart: bool,
    pub restart_reasons: Vec<String>,
    pub last_apply_error: Option<String>,
    pub last_apply_attempt_unix_ms: u64,
    pub last_apply_success_unix_ms: u64,
}

#[derive(Clone)]
pub struct RuntimeApplyHandles {
    pub middleware_dir: PathBuf,
    pub router: Arc<router::Router>,
    pub runtime: Arc<RwLock<proxy::TcpRuntimeConfig>>,
}

pub struct ManagementPlane {
    state_path: PathBuf,
    panel_token: String,
    worker_token: String,
    state: Mutex<PersistedManagementState>,
}

impl std::fmt::Debug for ManagementPlane {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ManagementPlane")
            .field("state_path", &self.state_path)
            .finish_non_exhaustive()
    }
}

impl ManagementPlane {
    pub fn open(
        workdir: &Path,
        bootstrap: &config::ManagementBootstrapConfig,
    ) -> anyhow::Result<Self> {
        let state_path = resolve_workdir_path(workdir, &bootstrap.state_file);
        let state = load_management_state(&state_path)?;
        Ok(Self {
            state_path,
            panel_token: bootstrap.panel_token.clone(),
            worker_token: bootstrap.worker_token.clone(),
            state: Mutex::new(state),
        })
    }

    pub fn panel_token(&self) -> &str {
        &self.panel_token
    }

    pub fn worker_token(&self) -> &str {
        &self.worker_token
    }

    pub async fn status(&self) -> ManagementStatusResponse {
        let state = self.state.lock().await;
        ManagementStatusResponse {
            state_path: self.state_path.display().to_string(),
            node_count: state.nodes.len(),
        }
    }

    pub async fn list_nodes(&self) -> Vec<ManagedNodeSnapshot> {
        let state = self.state.lock().await;
        state
            .nodes
            .values()
            .map(ManagedNodeRecord::snapshot)
            .collect()
    }

    pub async fn get_node(&self, node_id: &str) -> Option<ManagedNodeSnapshot> {
        let state = self.state.lock().await;
        state
            .nodes
            .get(node_id.trim())
            .map(ManagedNodeRecord::snapshot)
    }

    pub async fn get_node_config(&self, node_id: &str) -> Option<ManagedNodeConfigResponse> {
        let state = self.state.lock().await;
        state
            .nodes
            .get(node_id.trim())
            .map(|node| ManagedNodeConfigResponse {
                node: node.snapshot(),
                desired_config: node.desired_config.clone(),
            })
    }

    pub async fn set_desired_config(
        &self,
        node_id: &str,
        desired_config: config::ManagedConfigDocument,
    ) -> anyhow::Result<ManagedNodeConfigResponse> {
        config::validate_managed_config_document(&desired_config)?;

        let node_id = normalize_node_id(node_id)?;
        let mut state = self.state.lock().await;
        let node = state
            .nodes
            .entry(node_id.clone())
            .or_insert_with(|| ManagedNodeRecord::new(node_id.clone()));

        if node.desired_config.as_ref() != Some(&desired_config) {
            node.desired_revision = node.desired_revision.saturating_add(1).max(1);
            node.desired_config = Some(desired_config);
        }

        let response = ManagedNodeConfigResponse {
            node: node.snapshot(),
            desired_config: node.desired_config.clone(),
        };
        persist_management_state(&self.state_path, &state)?;
        Ok(response)
    }

    pub async fn worker_sync(
        &self,
        request: WorkerSyncRequest,
    ) -> anyhow::Result<WorkerSyncResponse> {
        let node_id = normalize_node_id(&request.node_id)?;
        let mut state = self.state.lock().await;
        let node = state
            .nodes
            .entry(node_id.clone())
            .or_insert_with(|| ManagedNodeRecord::new(node_id));

        node.connection_mode = Some(request.connection_mode);
        node.agent_url = clean_option(request.agent_url);
        node.applied_revision = request.applied_revision;
        node.pending_restart = request.pending_restart;
        node.restart_reasons = request.restart_reasons;
        node.last_apply_error = clean_option(request.last_apply_error);
        node.last_seen_unix_ms = telemetry::now_unix_ms();
        node.last_apply_attempt_unix_ms = request.last_apply_attempt_unix_ms;
        node.last_apply_success_unix_ms = request.last_apply_success_unix_ms;

        let response = WorkerSyncResponse {
            desired_revision: node.desired_revision,
            desired_config: node.desired_config.clone(),
            node: node.snapshot(),
        };

        persist_management_state(&self.state_path, &state)?;
        Ok(response)
    }
}

pub struct WorkerAgent {
    bootstrap: config::WorkerBootstrapConfig,
    state_path: PathBuf,
    state: Mutex<PersistedWorkerState>,
    client: Client,
    runtime: RwLock<Option<RuntimeApplyHandles>>,
}

impl std::fmt::Debug for WorkerAgent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("WorkerAgent")
            .field("node_id", &self.bootstrap.node_id)
            .field("state_path", &self.state_path)
            .finish_non_exhaustive()
    }
}

impl WorkerAgent {
    pub fn open(workdir: &Path, bootstrap: &config::WorkerBootstrapConfig) -> anyhow::Result<Self> {
        let state_path = workdir.join(WORKER_STATE_FILE);
        let state = load_worker_state(&state_path, bootstrap)?;
        let client = Client::builder()
            .timeout(bootstrap.sync_interval.max(Duration::from_secs(2)))
            .build()
            .context("managed: build worker sync client")?;

        Ok(Self {
            bootstrap: bootstrap.clone(),
            state_path,
            state: Mutex::new(state),
            client,
            runtime: RwLock::new(None),
        })
    }

    pub fn auth_token(&self) -> &str {
        &self.bootstrap.auth_token
    }

    pub fn connection_mode(&self) -> config::ManagedConnectionMode {
        self.bootstrap.connection_mode
    }

    pub async fn attach_runtime(&self, handles: RuntimeApplyHandles) {
        *self.runtime.write().await = Some(handles);
    }

    pub async fn startup_config(&self) -> Option<(u64, config::ManagedConfigDocument)> {
        let state = self.state.lock().await;
        if let Some(cfg) = state.desired_config.clone() {
            return Some((state.desired_revision, cfg));
        }
        state
            .applied_config
            .clone()
            .map(|cfg| (state.applied_revision, cfg))
    }

    pub async fn mark_started_with_startup_config(&self) -> anyhow::Result<()> {
        let mut next = self.state.lock().await.clone();
        let now = telemetry::now_unix_ms();

        if let Some(cfg) = next.desired_config.clone() {
            next.applied_revision = next.desired_revision;
            next.applied_config = Some(cfg);
            next.pending_restart = false;
            next.restart_reasons.clear();
            next.last_apply_error = None;
            next.last_apply_attempt_unix_ms = now;
            next.last_apply_success_unix_ms = now;
            self.replace_state(next).await?;
        }

        Ok(())
    }

    pub async fn status_snapshot(&self) -> WorkerStatusSnapshot {
        self.state.lock().await.snapshot()
    }

    pub async fn apply_push(
        &self,
        desired_revision: u64,
        desired_config: config::ManagedConfigDocument,
    ) -> anyhow::Result<WorkerStatusSnapshot> {
        self.accept_desired_config(desired_revision, desired_config)
            .await
    }

    pub async fn sync_once(&self) -> anyhow::Result<Option<WorkerSyncResponse>> {
        if self.bootstrap.connection_mode != config::ManagedConnectionMode::Active {
            return Ok(None);
        }

        let request = {
            let state = self.state.lock().await;
            WorkerSyncRequest {
                node_id: state.node_id.clone(),
                connection_mode: state.connection_mode,
                agent_url: state.agent_url.clone(),
                applied_revision: state.applied_revision,
                pending_restart: state.pending_restart,
                restart_reasons: state.restart_reasons.clone(),
                last_apply_error: state.last_apply_error.clone(),
                last_apply_attempt_unix_ms: state.last_apply_attempt_unix_ms,
                last_apply_success_unix_ms: state.last_apply_success_unix_ms,
            }
        };

        let url = format!("{}/managed/worker/sync", self.bootstrap.management_url);
        let response = self
            .client
            .post(url)
            .bearer_auth(&self.bootstrap.auth_token)
            .json(&request)
            .send()
            .await
            .context("managed: worker sync request failed")?
            .error_for_status()
            .context("managed: worker sync rejected")?;

        let sync: WorkerSyncResponse = response
            .json()
            .await
            .context("managed: decode worker sync response")?;

        if let Some(desired) = sync.desired_config.clone() {
            let _ = self
                .accept_desired_config(sync.desired_revision, desired)
                .await?;
        }

        Ok(Some(sync))
    }

    pub async fn run_active_sync_loop(self: Arc<Self>, mut shutdown: watch::Receiver<bool>) {
        if *shutdown.borrow() {
            return;
        }

        let interval = self.bootstrap.sync_interval.max(Duration::from_millis(250));
        loop {
            tokio::select! {
                _ = shutdown.changed() => {
                    if *shutdown.borrow() {
                        break;
                    }
                }
                _ = tokio::time::sleep(interval) => {
                    if let Err(err) = self.sync_once().await {
                        tracing::warn!(node_id = %self.bootstrap.node_id, err = %err, "managed: worker sync failed");
                    }
                }
            }
        }
    }

    async fn accept_desired_config(
        &self,
        desired_revision: u64,
        desired_config: config::ManagedConfigDocument,
    ) -> anyhow::Result<WorkerStatusSnapshot> {
        let current = self.state.lock().await.clone();

        if desired_revision == 0 {
            return Ok(current.snapshot());
        }

        if desired_revision < current.desired_revision {
            return Ok(current.snapshot());
        }

        if desired_revision == current.desired_revision
            && current.desired_config.as_ref() == Some(&desired_config)
            && (current.applied_revision == desired_revision || current.pending_restart)
        {
            return Ok(current.snapshot());
        }

        let validated = config::validate_managed_config_document(&desired_config)?;
        let runtime = self.runtime.read().await.clone();
        let mut next = current.clone();
        let now = telemetry::now_unix_ms();

        next.desired_revision = desired_revision;
        next.desired_config = Some(desired_config.clone());
        next.last_apply_attempt_unix_ms = now;

        if let Some(runtime) = runtime {
            let applied = if let Some(applied_config) = current.applied_config.as_ref() {
                config::validate_managed_config_document(applied_config)?
            } else {
                config::empty_managed_runtime_config()
            };

            let restart_reasons = config::restart_required_reasons(&applied, &validated);
            if !restart_reasons.is_empty() {
                next.pending_restart = true;
                next.restart_reasons = restart_reasons;
                next.last_apply_error = None;
                self.replace_state(next.clone()).await?;
                return Ok(next.snapshot());
            }

            match app::apply_runtime_config_update(
                &validated,
                &runtime.middleware_dir,
                &runtime.router,
                &runtime.runtime,
            )
            .await
            {
                Ok(()) => {
                    next.applied_revision = desired_revision;
                    next.applied_config = Some(desired_config);
                    next.pending_restart = false;
                    next.restart_reasons.clear();
                    next.last_apply_error = None;
                    next.last_apply_success_unix_ms = now;
                }
                Err(err) => {
                    next.last_apply_error = Some(err.to_string());
                    self.replace_state(next.clone()).await?;
                    return Ok(next.snapshot());
                }
            }
        } else {
            next.pending_restart = false;
            next.restart_reasons.clear();
            next.last_apply_error = None;
        }

        self.replace_state(next.clone()).await?;
        Ok(next.snapshot())
    }

    async fn replace_state(&self, next: PersistedWorkerState) -> anyhow::Result<()> {
        let mut state = self.state.lock().await;
        *state = next;
        persist_worker_state(&self.state_path, &state)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedManagementState {
    #[serde(default = "management_schema_version")]
    schema_version: u32,
    #[serde(default)]
    nodes: BTreeMap<String, ManagedNodeRecord>,
}

impl Default for PersistedManagementState {
    fn default() -> Self {
        Self {
            schema_version: MANAGEMENT_STATE_SCHEMA_VERSION,
            nodes: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ManagedNodeRecord {
    node_id: String,
    connection_mode: Option<config::ManagedConnectionMode>,
    agent_url: Option<String>,
    #[serde(default)]
    desired_revision: u64,
    desired_config: Option<config::ManagedConfigDocument>,
    #[serde(default)]
    applied_revision: u64,
    #[serde(default)]
    pending_restart: bool,
    #[serde(default)]
    restart_reasons: Vec<String>,
    last_apply_error: Option<String>,
    #[serde(default)]
    last_seen_unix_ms: u64,
    #[serde(default)]
    last_apply_attempt_unix_ms: u64,
    #[serde(default)]
    last_apply_success_unix_ms: u64,
}

impl ManagedNodeRecord {
    fn new(node_id: String) -> Self {
        Self {
            node_id,
            connection_mode: None,
            agent_url: None,
            desired_revision: 0,
            desired_config: None,
            applied_revision: 0,
            pending_restart: false,
            restart_reasons: Vec::new(),
            last_apply_error: None,
            last_seen_unix_ms: 0,
            last_apply_attempt_unix_ms: 0,
            last_apply_success_unix_ms: 0,
        }
    }

    fn snapshot(&self) -> ManagedNodeSnapshot {
        ManagedNodeSnapshot {
            node_id: self.node_id.clone(),
            connection_mode: self.connection_mode,
            agent_url: self.agent_url.clone(),
            desired_revision: self.desired_revision,
            applied_revision: self.applied_revision,
            pending_restart: self.pending_restart,
            restart_reasons: self.restart_reasons.clone(),
            last_apply_error: self.last_apply_error.clone(),
            last_seen_unix_ms: self.last_seen_unix_ms,
            last_apply_attempt_unix_ms: self.last_apply_attempt_unix_ms,
            last_apply_success_unix_ms: self.last_apply_success_unix_ms,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct PersistedWorkerState {
    node_id: String,
    connection_mode: config::ManagedConnectionMode,
    agent_url: Option<String>,
    #[serde(default)]
    desired_revision: u64,
    desired_config: Option<config::ManagedConfigDocument>,
    #[serde(default)]
    applied_revision: u64,
    applied_config: Option<config::ManagedConfigDocument>,
    #[serde(default)]
    pending_restart: bool,
    #[serde(default)]
    restart_reasons: Vec<String>,
    last_apply_error: Option<String>,
    #[serde(default)]
    last_apply_attempt_unix_ms: u64,
    #[serde(default)]
    last_apply_success_unix_ms: u64,
}

impl PersistedWorkerState {
    fn from_bootstrap(bootstrap: &config::WorkerBootstrapConfig) -> Self {
        Self {
            node_id: bootstrap.node_id.clone(),
            connection_mode: bootstrap.connection_mode,
            agent_url: clean_option(Some(bootstrap.agent_url.clone())),
            desired_revision: 0,
            desired_config: None,
            applied_revision: 0,
            applied_config: None,
            pending_restart: false,
            restart_reasons: Vec::new(),
            last_apply_error: None,
            last_apply_attempt_unix_ms: 0,
            last_apply_success_unix_ms: 0,
        }
    }

    fn snapshot(&self) -> WorkerStatusSnapshot {
        WorkerStatusSnapshot {
            node_id: self.node_id.clone(),
            connection_mode: self.connection_mode,
            agent_url: self.agent_url.clone(),
            desired_revision: self.desired_revision,
            applied_revision: self.applied_revision,
            pending_restart: self.pending_restart,
            restart_reasons: self.restart_reasons.clone(),
            last_apply_error: self.last_apply_error.clone(),
            last_apply_attempt_unix_ms: self.last_apply_attempt_unix_ms,
            last_apply_success_unix_ms: self.last_apply_success_unix_ms,
        }
    }
}

fn management_schema_version() -> u32 {
    MANAGEMENT_STATE_SCHEMA_VERSION
}

fn resolve_workdir_path(workdir: &Path, value: &str) -> PathBuf {
    let candidate = PathBuf::from(value);
    if candidate.is_relative() {
        workdir.join(candidate)
    } else {
        candidate
    }
}

fn normalize_node_id(node_id: &str) -> anyhow::Result<String> {
    let node_id = node_id.trim();
    if node_id.is_empty() {
        anyhow::bail!("managed: node_id must not be empty");
    }
    Ok(node_id.to_string())
}

fn clean_option(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let value = value.trim().to_string();
        if value.is_empty() { None } else { Some(value) }
    })
}

fn load_management_state(path: &Path) -> anyhow::Result<PersistedManagementState> {
    if !path.exists() {
        return Ok(PersistedManagementState::default());
    }

    let data = fs::read(path).with_context(|| format!("managed: read {}", path.display()))?;
    let mut state: PersistedManagementState = serde_json::from_slice(&data)
        .with_context(|| format!("managed: parse {}", path.display()))?;
    if state.schema_version == 0 {
        state.schema_version = MANAGEMENT_STATE_SCHEMA_VERSION;
    }
    Ok(state)
}

fn persist_management_state(path: &Path, state: &PersistedManagementState) -> anyhow::Result<()> {
    write_json_atomic(path, state)
}

fn load_worker_state(
    path: &Path,
    bootstrap: &config::WorkerBootstrapConfig,
) -> anyhow::Result<PersistedWorkerState> {
    if !path.exists() {
        return Ok(PersistedWorkerState::from_bootstrap(bootstrap));
    }

    let data = fs::read(path).with_context(|| format!("managed: read {}", path.display()))?;
    let mut state: PersistedWorkerState = serde_json::from_slice(&data)
        .with_context(|| format!("managed: parse {}", path.display()))?;
    state.node_id = bootstrap.node_id.clone();
    state.connection_mode = bootstrap.connection_mode;
    state.agent_url = clean_option(Some(bootstrap.agent_url.clone()));
    Ok(state)
}

fn persist_worker_state(path: &Path, state: &PersistedWorkerState) -> anyhow::Result<()> {
    write_json_atomic(path, state)
}

fn write_json_atomic<T: Serialize>(path: &Path, value: &T) -> anyhow::Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("managed: mkdir {}", parent.display()))?;
    }

    let bytes = serde_json::to_vec_pretty(value).context("managed: encode state")?;
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, bytes).with_context(|| format!("managed: write {}", tmp.display()))?;
    fs::rename(&tmp, path)
        .with_context(|| format!("managed: rename {} -> {}", tmp.display(), path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, path::PathBuf, sync::Arc};

    use axum::Router;
    use tokio::sync::watch;

    use super::*;
    use crate::prism::{admin, middleware};

    fn temp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        p.push(format!(
            "prism_managed_test_{name}_{}_{}",
            std::process::id(),
            now
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    fn simple_managed_doc(listen_addr: &str) -> config::ManagedConfigDocument {
        config::ManagedConfigDocument {
            listeners: vec![config::ManagedProxyListenerDocument {
                listen_addr: listen_addr.to_string(),
                protocol: "tcp".to_string(),
                upstream: String::new(),
            }],
            routes: vec![config::ManagedRouteDocument {
                hosts: vec!["play.example.com".to_string()],
                upstreams: vec!["127.0.0.1:25566".to_string()],
                middlewares: vec!["minecraft_handshake".to_string()],
                strategy: "sequential".to_string(),
            }],
            ..Default::default()
        }
    }

    async fn spawn_management_server(
        plane: Arc<ManagementPlane>,
    ) -> (SocketAddr, watch::Sender<bool>) {
        let (reload_tx, _) = watch::channel(telemetry::ReloadSignal::new());
        let (shutdown_tx, shutdown_rx) = watch::channel(false);
        let state = admin::AdminState {
            metrics: Arc::new(telemetry::MetricsRegistry::new()),
            metrics_enabled: false,
            sessions: Arc::new(telemetry::SessionRegistry::new()),
            config_path: PathBuf::from("managed.json"),
            reload_tx,
            tunnel: None,
            auth: admin::AdminAuth {
                panel_token: Some(plane.panel_token().to_string()),
                worker_token: Some(plane.worker_token().to_string()),
            },
            management: Some(plane),
            worker: None,
        };

        let app: Router = admin::build_router(state);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind listener");
        let addr = listener.local_addr().expect("local addr");
        tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let mut shutdown_rx = shutdown_rx;
                    if *shutdown_rx.borrow() {
                        return;
                    }
                    while shutdown_rx.changed().await.is_ok() {
                        if *shutdown_rx.borrow() {
                            break;
                        }
                    }
                })
                .await
                .expect("serve management router");
        });

        (addr, shutdown_tx)
    }

    fn runtime_handles(middleware_dir: &Path) -> RuntimeApplyHandles {
        RuntimeApplyHandles {
            middleware_dir: middleware_dir.to_path_buf(),
            router: Arc::new(router::Router::new(Vec::new())),
            runtime: Arc::new(RwLock::new(proxy::TcpRuntimeConfig {
                max_header_bytes: 64 * 1024,
                handshake_timeout: Duration::from_millis(3000),
                idle_timeout: Duration::from_millis(0),
                upstream_dial_timeout: Duration::from_millis(5000),
                buffer_size: 32 * 1024,
                proxy_protocol_v2: false,
            })),
        }
    }

    #[tokio::test]
    async fn active_worker_sync_tracks_desired_and_applied_revisions() {
        let management_dir = temp_dir("active_sync_management");
        let worker_dir = temp_dir("active_sync_worker");

        let plane = Arc::new(
            ManagementPlane::open(
                &management_dir,
                &config::ManagementBootstrapConfig {
                    state_file: "management-state.json".to_string(),
                    panel_token: "panel-secret".to_string(),
                    worker_token: "worker-secret".to_string(),
                },
            )
            .expect("management plane"),
        );
        plane
            .set_desired_config("node-a", simple_managed_doc(":25565"))
            .await
            .expect("set desired config");

        let (addr, shutdown_tx) = spawn_management_server(plane.clone()).await;

        let worker = WorkerAgent::open(
            &worker_dir,
            &config::WorkerBootstrapConfig {
                node_id: "node-a".to_string(),
                management_url: format!("http://{addr}"),
                auth_token: "worker-secret".to_string(),
                connection_mode: config::ManagedConnectionMode::Active,
                sync_interval: Duration::from_millis(50),
                agent_url: String::new(),
            },
        )
        .expect("worker agent");

        worker.sync_once().await.expect("initial sync");
        let enrolled = plane.get_node("node-a").await.expect("managed node");
        assert_eq!(enrolled.desired_revision, 1);
        assert_eq!(enrolled.applied_revision, 0);
        assert_eq!(
            enrolled.connection_mode,
            Some(config::ManagedConnectionMode::Active)
        );

        let startup = worker.startup_config().await.expect("startup config");
        assert_eq!(startup.0, 1);

        worker
            .mark_started_with_startup_config()
            .await
            .expect("mark started");
        worker.sync_once().await.expect("report applied revision");

        let converged = plane.get_node("node-a").await.expect("managed node");
        assert_eq!(converged.desired_revision, 1);
        assert_eq!(converged.applied_revision, 1);
        assert!(!converged.pending_restart);

        let _ = shutdown_tx.send(true);
        let _ = std::fs::remove_dir_all(&management_dir);
        let _ = std::fs::remove_dir_all(&worker_dir);
    }

    #[tokio::test]
    async fn management_api_rejects_invalid_managed_config() {
        let management_dir = temp_dir("invalid_config");
        let plane = Arc::new(
            ManagementPlane::open(
                &management_dir,
                &config::ManagementBootstrapConfig {
                    state_file: "management-state.json".to_string(),
                    panel_token: "panel-secret".to_string(),
                    worker_token: "worker-secret".to_string(),
                },
            )
            .expect("management plane"),
        );

        let (addr, shutdown_tx) = spawn_management_server(plane).await;
        let client = Client::new();
        let response = client
            .put(format!("http://{addr}/managed/nodes/node-b/config"))
            .bearer_auth("panel-secret")
            .json(&PutManagedNodeConfigRequest {
                desired_config: config::ManagedConfigDocument {
                    routes: vec![config::ManagedRouteDocument {
                        hosts: vec!["play.example.com".to_string()],
                        upstreams: vec!["127.0.0.1:25565".to_string()],
                        middlewares: Vec::new(),
                        strategy: "sequential".to_string(),
                    }],
                    ..Default::default()
                },
            })
            .send()
            .await
            .expect("request");

        assert_eq!(response.status(), reqwest::StatusCode::BAD_REQUEST);

        let _ = shutdown_tx.send(true);
        let _ = std::fs::remove_dir_all(&management_dir);
    }

    #[tokio::test]
    async fn worker_flags_listener_changes_as_restart_required() {
        let worker_dir = temp_dir("restart_required");
        let middleware_dir = worker_dir.join("middlewares");
        middleware::materialize_default_middlewares(&middleware_dir)
            .expect("materialize middlewares");

        let worker = WorkerAgent::open(
            &worker_dir,
            &config::WorkerBootstrapConfig {
                node_id: "node-c".to_string(),
                management_url: String::new(),
                auth_token: "worker-secret".to_string(),
                connection_mode: config::ManagedConnectionMode::Passive,
                sync_interval: Duration::from_millis(50),
                agent_url: String::new(),
            },
        )
        .expect("worker agent");

        let _ = worker
            .apply_push(1, simple_managed_doc(":25565"))
            .await
            .expect("store desired revision");
        worker
            .mark_started_with_startup_config()
            .await
            .expect("mark started");
        worker
            .attach_runtime(runtime_handles(&middleware_dir))
            .await;

        let status = worker
            .apply_push(2, simple_managed_doc(":25566"))
            .await
            .expect("classify restart");

        assert_eq!(status.desired_revision, 2);
        assert_eq!(status.applied_revision, 1);
        assert!(status.pending_restart);
        assert!(
            status
                .restart_reasons
                .iter()
                .any(|reason| reason.contains("listener"))
        );

        let _ = std::fs::remove_dir_all(&worker_dir);
    }
}
