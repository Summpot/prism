use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::prism::auth::{TokenRecord, UserRecord, UserRole};
use crate::prism::config::ManagedConfigDocument;
use crate::prism::managed::{
    ManagedNodeConfigResponse, ManagedNodeSnapshot, ManagementStatusResponse,
};
use crate::prism::middleware::MiddlewareConfigSchema;
use crate::prism::telemetry::SessionInfo;
use crate::prism::tunnel::manager::ServiceSnapshot;
use crate::prism::tunnel::optimizer::OptimizerStatsSnapshot;

pub const ADMIN_PROTO_V1: u16 = 1;
pub const MAX_ADMIN_FRAME_BYTES: usize = 1024 * 1024;

pub const FEATURE_RPC: u64 = 1 << 0;
pub const FEATURE_EVENTS: u64 = 1 << 1;
pub const FEATURE_AUTH: u64 = 1 << 2;
pub const FEATURE_PANEL: u64 = 1 << 3;
pub const FEATURE_MANAGED: u64 = 1 << 4;
pub const FEATURE_MIDDLEWARE: u64 = 1 << 5;

pub const CLIENT_FEATURES: u64 = FEATURE_RPC
    | FEATURE_EVENTS
    | FEATURE_AUTH
    | FEATURE_PANEL
    | FEATURE_MANAGED
    | FEATURE_MIDDLEWARE;

pub const TOPIC_CONNECTIONS: u64 = 1 << 0;
pub const TOPIC_SERVICES: u64 = 1 << 1;
pub const TOPIC_OPTIMIZER: u64 = 1 << 2;
pub const TOPIC_RELOAD: u64 = 1 << 3;

/// Outer postcard envelope. `proto` is inspected before decoding `msg`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Envelope {
    pub proto: u16,
    pub msg: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdminMsg {
    Hello {
        features: u64,
    },
    HelloAck {
        features: u64,
    },
    HelloReject {
        reason: HelloRejectReason,
    },
    Request {
        id: u32,
        method: AdminMethod,
    },
    Response {
        id: u32,
        result: Result<AdminPayload, AdminError>,
    },
    Event {
        seq: u32,
        event: AdminEvent,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum HelloRejectReason {
    UnsupportedVersion { peer: u16, server: u16 },
    Malformed,
    AlreadyNegotiated,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdminMethod {
    Health,
    ConfigPath,
    Connections,
    TunnelServices,
    OptimizerStats,
    Reload,
    AuthProviders,
    AuthGithubLogin,
    AuthGithubExchange {
        code: String,
        device_id: Option<String>,
    },
    AuthSession,
    AuthListTokens,
    AuthCreateToken {
        user_id: String,
        name: String,
        expires_in_days: Option<u64>,
    },
    AuthRevokeToken {
        token_id: String,
    },
    ManagedStatus,
    ManagedNodes,
    ManagedNode {
        node_id: String,
    },
    ManagedNodeConfig {
        node_id: String,
    },
    PutManagedNodeConfig {
        node_id: String,
        desired_config: ManagedConfigDocument,
    },
    ManagedUsers,
    PutManagedUser {
        user_id: String,
        role: UserRole,
        service_rules: Vec<String>,
    },
    ListMiddlewares,
    MiddlewareSchema {
        name: String,
    },
    MiddlewareConfig {
        name: String,
    },
    PutMiddlewareConfig {
        name: String,
        config: HashMap<String, serde_json::Value>,
    },
    ResetMiddlewareConfig {
        name: String,
    },
    Subscribe {
        topics: u64,
    },
    Authenticate {
        token: String,
    },
}

impl AdminMethod {
    pub fn feature(&self) -> u64 {
        match self {
            Self::Health => FEATURE_RPC,
            Self::Subscribe { .. } => FEATURE_EVENTS,
            Self::ConfigPath
            | Self::Connections
            | Self::TunnelServices
            | Self::OptimizerStats
            | Self::Reload => FEATURE_PANEL,
            Self::AuthProviders
            | Self::AuthGithubLogin
            | Self::AuthGithubExchange { .. }
            | Self::AuthSession
            | Self::AuthListTokens
            | Self::AuthCreateToken { .. }
            | Self::AuthRevokeToken { .. }
            | Self::Authenticate { .. } => FEATURE_AUTH,
            Self::ManagedStatus
            | Self::ManagedNodes
            | Self::ManagedNode { .. }
            | Self::ManagedNodeConfig { .. }
            | Self::PutManagedNodeConfig { .. }
            | Self::ManagedUsers
            | Self::PutManagedUser { .. } => FEATURE_MANAGED,
            Self::ListMiddlewares
            | Self::MiddlewareSchema { .. }
            | Self::MiddlewareConfig { .. }
            | Self::PutMiddlewareConfig { .. }
            | Self::ResetMiddlewareConfig { .. } => FEATURE_MIDDLEWARE,
        }
    }

    pub fn requires_session(&self) -> bool {
        !matches!(
            self,
            Self::Health
                | Self::AuthProviders
                | Self::AuthGithubLogin
                | Self::AuthGithubExchange { .. }
                | Self::AuthSession
                | Self::Authenticate { .. }
        )
    }

    pub fn from_rpc(name: &str, payload: &serde_json::Value) -> Result<Self, AdminError> {
        let obj = || payload.as_object();
        let str_field = |key: &str| -> Result<String, AdminError> {
            obj()
                .and_then(|o| o.get(key))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
                .ok_or_else(|| AdminError::bad_request(format!("missing field '{key}'")))
        };
        match name {
            "health" => Ok(Self::Health),
            "config_path" => Ok(Self::ConfigPath),
            "connections" => Ok(Self::Connections),
            "tunnel_services" => Ok(Self::TunnelServices),
            "optimizer_stats" => Ok(Self::OptimizerStats),
            "reload" => Ok(Self::Reload),
            "auth.providers" => Ok(Self::AuthProviders),
            "auth.github.login" => Ok(Self::AuthGithubLogin),
            "auth.github.exchange" => Ok(Self::AuthGithubExchange {
                code: str_field("code")?,
                device_id: obj()
                    .and_then(|o| o.get("device_id"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string()),
            }),
            "auth.session" => Ok(Self::AuthSession),
            "auth.tokens.list" => Ok(Self::AuthListTokens),
            "auth.tokens.create" => Ok(Self::AuthCreateToken {
                user_id: str_field("user_id")?,
                name: str_field("name")?,
                expires_in_days: obj()
                    .and_then(|o| o.get("expires_in_days"))
                    .and_then(|v| v.as_u64()),
            }),
            "auth.tokens.revoke" => Ok(Self::AuthRevokeToken {
                token_id: str_field("token_id")?,
            }),
            "managed.status" => Ok(Self::ManagedStatus),
            "managed.nodes" => Ok(Self::ManagedNodes),
            "managed.node" => Ok(Self::ManagedNode {
                node_id: str_field("node_id")?,
            }),
            "managed.node.config" => Ok(Self::ManagedNodeConfig {
                node_id: str_field("node_id")?,
            }),
            "managed.node.config.put" => {
                let node_id = str_field("node_id")?;
                let desired_config = obj()
                    .and_then(|o| o.get("desired_config"))
                    .cloned()
                    .ok_or_else(|| AdminError::bad_request("missing field 'desired_config'"))?;
                let desired_config: ManagedConfigDocument = serde_json::from_value(desired_config)
                    .map_err(|e| AdminError::bad_request(e.to_string()))?;
                Ok(Self::PutManagedNodeConfig {
                    node_id,
                    desired_config,
                })
            }
            "managed.users" => Ok(Self::ManagedUsers),
            "managed.user.put" => {
                let user_id = str_field("user_id")?;
                let role = obj()
                    .and_then(|o| o.get("role"))
                    .cloned()
                    .ok_or_else(|| AdminError::bad_request("missing field 'role'"))?;
                let role: UserRole = serde_json::from_value(role)
                    .map_err(|e| AdminError::bad_request(e.to_string()))?;
                let service_rules = obj()
                    .and_then(|o| o.get("service_rules"))
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                            .collect()
                    })
                    .unwrap_or_default();
                Ok(Self::PutManagedUser {
                    user_id,
                    role,
                    service_rules,
                })
            }
            "middlewares.list" => Ok(Self::ListMiddlewares),
            "middlewares.schema" => Ok(Self::MiddlewareSchema {
                name: str_field("name")?,
            }),
            "middlewares.config" => Ok(Self::MiddlewareConfig {
                name: str_field("name")?,
            }),
            "middlewares.config.put" => {
                let name = str_field("name")?;
                let config = obj()
                    .and_then(|o| o.get("config"))
                    .cloned()
                    .unwrap_or_else(|| payload.clone());
                let config: HashMap<String, serde_json::Value> = if config.is_object() {
                    serde_json::from_value(config)
                        .map_err(|e| AdminError::bad_request(e.to_string()))?
                } else {
                    HashMap::new()
                };
                Ok(Self::PutMiddlewareConfig { name, config })
            }
            "middlewares.config.reset" => Ok(Self::ResetMiddlewareConfig {
                name: str_field("name")?,
            }),
            "subscribe" => {
                let topics = payload
                    .get("topics")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                Ok(Self::Subscribe { topics })
            }
            "authenticate" => Ok(Self::Authenticate {
                token: str_field("token")?,
            }),
            other => Err(AdminError::bad_request(format!(
                "unknown admin method '{other}'"
            ))),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdminPayload {
    Health {
        ok: bool,
    },
    ConfigPath {
        path: String,
    },
    Connections(Vec<SessionInfo>),
    TunnelServices(Vec<ServiceSnapshot>),
    Optimizer {
        global: OptimizerStatsSnapshot,
        services: HashMap<String, OptimizerStatsSnapshot>,
    },
    Reload {
        seq: u64,
    },
    AuthProviders {
        github_enabled: bool,
        github_client_id: Option<String>,
        mode: String,
        providers: Vec<String>,
    },
    AuthGithubLogin {
        url: String,
    },
    AuthGithubExchange {
        token: String,
        user: UserRecord,
        token_id: String,
        expires_at_unix_ms: Option<u64>,
    },
    AuthSession(AuthSessionSnapshot),
    AuthTokens(Vec<TokenRecord>),
    AuthCreateToken {
        raw_token: String,
        token: TokenRecord,
    },
    AuthRevokeToken {
        revoked: bool,
    },
    ManagedStatus(ManagementStatusResponse),
    ManagedNodes(Vec<ManagedNodeSnapshot>),
    ManagedNode(ManagedNodeSnapshot),
    ManagedNodeConfig(ManagedNodeConfigResponse),
    ManagedUsers(Vec<UserRecord>),
    ManagedUser(UserRecord),
    Middlewares(Vec<MiddlewareItem>),
    MiddlewareSchema(MiddlewareConfigSchema),
    MiddlewareConfig(HashMap<String, serde_json::Value>),
    MiddlewareConfigUpdated {
        status: String,
        name: String,
        config: HashMap<String, serde_json::Value>,
    },
    MiddlewareConfigReset {
        status: String,
        name: String,
        reset: bool,
    },
    Subscribed {
        topics: u64,
    },
}

impl AdminPayload {
    pub fn to_json_value(&self) -> serde_json::Value {
        match self {
            Self::Health { ok } => serde_json::json!({ "ok": ok }),
            Self::ConfigPath { path } => serde_json::json!({ "path": path }),
            Self::Connections(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::TunnelServices(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::Optimizer { global, services } => serde_json::json!({
                "global": global,
                "services": services,
            }),
            Self::Reload { seq } => serde_json::json!({ "seq": seq }),
            Self::AuthProviders {
                github_enabled,
                github_client_id,
                mode,
                providers,
            } => serde_json::json!({
                "github_enabled": github_enabled,
                "github_client_id": github_client_id,
                "mode": mode,
                "providers": providers,
            }),
            Self::AuthGithubLogin { url } => serde_json::json!({ "url": url }),
            Self::AuthGithubExchange {
                token,
                user,
                token_id,
                expires_at_unix_ms,
            } => serde_json::json!({
                "token": token,
                "user": user,
                "token_id": token_id,
                "expires_at_unix_ms": expires_at_unix_ms,
            }),
            Self::AuthSession(s) => serde_json::to_value(s).unwrap_or(serde_json::Value::Null),
            Self::AuthTokens(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::AuthCreateToken { raw_token, token } => serde_json::json!({
                "raw_token": raw_token,
                "token": token,
            }),
            Self::AuthRevokeToken { revoked } => serde_json::json!({ "revoked": revoked }),
            Self::ManagedStatus(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::ManagedNodes(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::ManagedNode(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::ManagedNodeConfig(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::ManagedUsers(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::ManagedUser(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::Middlewares(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::MiddlewareSchema(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::MiddlewareConfig(v) => serde_json::to_value(v).unwrap_or(serde_json::Value::Null),
            Self::MiddlewareConfigUpdated {
                status,
                name,
                config,
            } => serde_json::json!({
                "status": status,
                "name": name,
                "config": config,
            }),
            Self::MiddlewareConfigReset {
                status,
                name,
                reset,
            } => serde_json::json!({
                "status": status,
                "name": name,
                "reset": reset,
            }),
            Self::Subscribed { topics } => serde_json::json!({ "topics": topics }),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuthSessionSnapshot {
    pub authenticated: bool,
    pub user_id: Option<String>,
    pub username: Option<String>,
    pub display_name: Option<String>,
    pub avatar_url: Option<String>,
    pub role: Option<String>,
    pub service_rules: Vec<String>,
    pub is_admin: bool,
}

impl AuthSessionSnapshot {
    pub fn unauthenticated() -> Self {
        Self {
            authenticated: false,
            user_id: None,
            username: None,
            display_name: None,
            avatar_url: None,
            role: None,
            service_rules: Vec::new(),
            is_admin: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MiddlewareItem {
    pub name: String,
    pub schema: Option<MiddlewareConfigSchema>,
    pub effective_config: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AdminEvent {
    Connections(Vec<SessionInfo>),
    TunnelServices(Vec<ServiceSnapshot>),
    Optimizer {
        global: OptimizerStatsSnapshot,
        services: HashMap<String, OptimizerStatsSnapshot>,
    },
    Reload {
        seq: u64,
    },
}

impl AdminEvent {
    pub fn topic(&self) -> u64 {
        match self {
            Self::Connections(_) => TOPIC_CONNECTIONS,
            Self::TunnelServices(_) => TOPIC_SERVICES,
            Self::Optimizer { .. } => TOPIC_OPTIMIZER,
            Self::Reload { .. } => TOPIC_RELOAD,
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum AdminErrorCode {
    Unauthorized,
    Forbidden,
    NotFound,
    BadRequest,
    Unavailable,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AdminError {
    pub code: AdminErrorCode,
    pub message: String,
}

impl AdminError {
    pub fn unauthorized(message: impl Into<String>) -> Self {
        Self {
            code: AdminErrorCode::Unauthorized,
            message: message.into(),
        }
    }

    pub fn forbidden(message: impl Into<String>) -> Self {
        Self {
            code: AdminErrorCode::Forbidden,
            message: message.into(),
        }
    }

    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            code: AdminErrorCode::NotFound,
            message: message.into(),
        }
    }

    pub fn bad_request(message: impl Into<String>) -> Self {
        Self {
            code: AdminErrorCode::BadRequest,
            message: message.into(),
        }
    }

    pub fn unavailable(message: impl Into<String>) -> Self {
        Self {
            code: AdminErrorCode::Unavailable,
            message: message.into(),
        }
    }

    pub fn internal(message: impl Into<String>) -> Self {
        Self {
            code: AdminErrorCode::Internal,
            message: message.into(),
        }
    }

    pub fn http_status(&self) -> u16 {
        match self.code {
            AdminErrorCode::Unauthorized => 401,
            AdminErrorCode::Forbidden => 403,
            AdminErrorCode::NotFound => 404,
            AdminErrorCode::BadRequest => 400,
            AdminErrorCode::Unavailable => 503,
            AdminErrorCode::Internal => 500,
        }
    }
}

impl std::fmt::Display for AdminError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}: {}", self.http_status(), self.message)
    }
}

impl std::error::Error for AdminError {}

#[derive(Debug, thiserror::Error)]
pub enum ControlError {
    #[error("unsupported admin protocol version {0}")]
    UnsupportedVersion(u16),
    #[error("admin handshake rejected: {0:?}")]
    Rejected(HelloRejectReason),
    #[error("admin handshake failed: {0}")]
    Handshake(String),
    #[error("admin frame too large")]
    FrameTooLarge,
    #[error("admin codec: {0}")]
    Codec(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl From<postcard::Error> for ControlError {
    fn from(err: postcard::Error) -> Self {
        ControlError::Codec(err.to_string())
    }
}
