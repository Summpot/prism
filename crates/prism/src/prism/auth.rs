//! Authentication, user management, and service ACL control for Prism.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use rand::{RngExt, rng};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;

use crate::prism::tunnel::protocol::RegisteredService;

/// User role for RBAC.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    Admin,
    Member,
    Disabled,
}

/// Token type distinguishing clients, connectors, and admins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenType {
    Client,
    Admin,
    Connector,
}

/// User profile record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: String,
    pub username: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default)]
    pub avatar_url: Option<String>,
    pub role: UserRole,
    #[serde(default = "default_rules")]
    pub service_rules: Vec<String>,
    pub created_at_unix_ms: u64,
    pub last_login_unix_ms: u64,
}

fn default_rules() -> Vec<String> {
    vec!["*".to_string()]
}

/// Token metadata stored in state (raw token is never persisted).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenRecord {
    pub id: String,
    pub token_hash: String,
    pub user_id: String,
    pub token_type: TokenType,
    pub name: String,
    #[serde(default)]
    pub service_rules: Option<Vec<String>>,
    pub created_at_unix_ms: u64,
    #[serde(default)]
    pub expires_at_unix_ms: Option<u64>,
    #[serde(default)]
    pub last_used_unix_ms: u64,
}

/// Authenticated identity result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthIdentity {
    pub user_id: String,
    pub username: String,
    pub role: UserRole,
    pub service_rules: Vec<String>,
    pub is_admin: bool,
}

impl AuthIdentity {
    pub fn can_access_service(&self, service_name: &str) -> bool {
        if self.role == UserRole::Disabled {
            return false;
        }
        if self.is_admin {
            return true;
        }
        self.service_rules
            .iter()
            .any(|pattern| match_service_rule(pattern, service_name))
    }
}

/// Evaluates if a pattern matches a service name.
/// Supports exact match, `*` for everything, or wildcard prefix `prefix-*`.
pub fn match_service_rule(pattern: &str, target: &str) -> bool {
    let pattern = pattern.trim();
    let target = target.trim();
    if pattern == "*" {
        return true;
    }
    if let Some(prefix) = pattern.strip_suffix('*') {
        return target.starts_with(prefix);
    }
    pattern.eq_ignore_ascii_case(target)
}

/// Computes a hex SHA-256 hash of a string.
pub fn hash_token(raw: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(raw.trim().as_bytes());
    format!("{:x}", hasher.finalize())
}

/// Generates a secure random token with prefix (e.g. `prism_cl_`).
pub fn generate_token(prefix: &str) -> (String, String) {
    let mut bytes = [0u8; 24];
    rng().fill(&mut bytes);
    let hex_part = hex::encode(bytes);
    let raw = format!("{prefix}{hex_part}");
    let hash = hash_token(&raw);
    (raw, hash)
}

fn now_unix_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis() as u64
}

// Simple hex encoder without adding extra dependency
mod hex {
    pub fn encode(data: impl AsRef<[u8]>) -> String {
        let mut s = String::with_capacity(data.as_ref().len() * 2);
        for &b in data.as_ref() {
            use std::fmt::Write;
            let _ = write!(s, "{:02x}", b);
        }
        s
    }
}

/// GitHub OAuth configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitHubOAuthConfig {
    #[serde(default)]
    pub enabled: bool,
    #[serde(default)]
    pub client_id: String,
    #[serde(default)]
    pub client_secret: String,
    #[serde(default)]
    pub redirect_uri: Option<String>,
    #[serde(default)]
    pub admin_users: Vec<String>,
    #[serde(default)]
    pub admin_orgs: Vec<String>,
    #[serde(default)]
    pub allowed_users: Vec<String>,
    #[serde(default)]
    pub allowed_orgs: Vec<String>,
    #[serde(default = "default_member_role")]
    pub default_role: String,
}

fn default_member_role() -> String {
    "member".to_string()
}

/// Complete auth configuration.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthConfig {
    #[serde(default = "default_auth_mode")]
    pub mode: String, // "token" | "oauth" | "hybrid"
    #[serde(default)]
    pub legacy_token: Option<String>,
    #[serde(default)]
    pub github: Option<GitHubOAuthConfig>,
}

fn default_auth_mode() -> String {
    "hybrid".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
struct PersistedAuthState {
    #[serde(default = "default_schema_version")]
    schema_version: u32,
    #[serde(default)]
    users: HashMap<String, UserRecord>,
    #[serde(default)]
    tokens: HashMap<String, TokenRecord>,
}

fn default_schema_version() -> u32 {
    1
}

/// GitHub user profile returned by API.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubUser {
    pub id: i64,
    pub login: String,
    pub name: Option<String>,
    pub avatar_url: Option<String>,
}

/// GitHub organization item returned by `/user/orgs`.
#[derive(Debug, Clone, Deserialize)]
pub struct GitHubOrg {
    pub login: String,
}

/// Device code response from GitHub.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCodeResponse {
    pub device_code: String,
    pub user_code: String,
    pub verification_uri: String,
    pub expires_in: u64,
    pub interval: u64,
}

/// Device poll result from GitHub.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "status")]
pub enum DevicePollResult {
    #[serde(rename = "pending")]
    Pending,
    #[serde(rename = "slow_down")]
    SlowDown { interval: u64 },
    #[serde(rename = "expired")]
    Expired,
    #[serde(rename = "success")]
    Success {
        user: UserRecord,
        token: String,
        token_id: String,
    },
    #[serde(rename = "denied")]
    Denied { message: String },
}

/// Central authentication and user management plane.
pub struct AuthManager {
    config: AuthConfig,
    state_path: Option<PathBuf>,
    state: RwLock<PersistedAuthState>,
    http_client: reqwest::Client,
}

impl std::fmt::Debug for AuthManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthManager")
            .field("state_path", &self.state_path)
            .finish_non_exhaustive()
    }
}

impl AuthManager {
    /// Creates a new AuthManager with persistence and optional GitHub integration.
    pub fn new(config: AuthConfig, workdir: Option<&Path>) -> Self {
        let state_path = workdir.map(|p| p.join("auth-state.json"));
        let state = if let Some(ref path) = state_path {
            if path.is_file() {
                match std::fs::read_to_string(path) {
                    Ok(content) => {
                        serde_json::from_str::<PersistedAuthState>(&content).unwrap_or_default()
                    }
                    Err(_) => PersistedAuthState::default(),
                }
            } else {
                PersistedAuthState::default()
            }
        } else {
            PersistedAuthState::default()
        };

        let http_client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();

        Self {
            config,
            state_path,
            state: RwLock::new(state),
            http_client,
        }
    }

    /// Saves state to disk if path is configured.
    async fn save_state(&self) -> anyhow::Result<()> {
        let Some(ref path) = self.state_path else {
            return Ok(());
        };
        let guard = self.state.read().await;
        let data = serde_json::to_string_pretty(&*guard)?;
        drop(guard);

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        std::fs::write(path, data)?;
        Ok(())
    }

    /// Returns the active GitHub OAuth configuration if enabled.
    pub fn github_config(&self) -> Option<&GitHubOAuthConfig> {
        self.config.github.as_ref().filter(|g| g.enabled)
    }

    /// Verifies any presented token (Client PAT, Admin Token, or legacy PSK).
    pub async fn verify_token(&self, raw_token: &str) -> Option<AuthIdentity> {
        let raw_token = raw_token.trim();
        if raw_token.is_empty() {
            return None;
        }

        // 1. Check legacy token fallback if configured
        if let Some(ref legacy) = self.config.legacy_token {
            if !legacy.trim().is_empty() && raw_token == legacy.trim() {
                return Some(AuthIdentity {
                    user_id: "legacy_admin".to_string(),
                    username: "Legacy Admin".to_string(),
                    role: UserRole::Admin,
                    service_rules: vec!["*".to_string()],
                    is_admin: true,
                });
            }
        }

        // 2. Hash raw token and lookup in tokens map
        let hash = hash_token(raw_token);
        let now = now_unix_ms();

        let mut guard = self.state.write().await;
        let token_record = guard.tokens.get_mut(&hash)?;

        // Check token expiration
        if let Some(exp) = token_record.expires_at_unix_ms {
            if now > exp {
                return None;
            }
        }

        token_record.last_used_unix_ms = now;
        let user_id = token_record.user_id.clone();
        let token_type = token_record.token_type;
        let specific_rules = token_record.service_rules.clone();

        let user = guard.users.get(&user_id)?;
        if user.role == UserRole::Disabled {
            return None;
        }

        let is_admin = user.role == UserRole::Admin || token_type == TokenType::Admin;
        let service_rules = specific_rules.unwrap_or_else(|| user.service_rules.clone());

        Some(AuthIdentity {
            user_id: user.id.clone(),
            username: user.username.clone(),
            role: user.role,
            service_rules,
            is_admin,
        })
    }

    /// Filters a list of registered services according to the user's ACL.
    pub fn filter_services(
        &self,
        identity: &AuthIdentity,
        services: &[RegisteredService],
    ) -> Vec<RegisteredService> {
        if identity.is_admin {
            return services.to_vec();
        }
        services
            .iter()
            .filter(|s| identity.can_access_service(&s.name))
            .cloned()
            .collect()
    }

    /// Checks if identity can access target service.
    pub fn can_access_service(&self, identity: &AuthIdentity, service: &str) -> bool {
        identity.can_access_service(service)
    }

    /// Creates a new client token for a user.
    pub async fn create_client_token(
        &self,
        user_id: &str,
        name: &str,
        expires_in_days: Option<u64>,
    ) -> anyhow::Result<(String, TokenRecord)> {
        let (raw, hash) = generate_token("prism_cl_");
        let now = now_unix_ms();
        let expires_at_unix_ms = expires_in_days.map(|days| now + days * 86_400_000);

        let record = TokenRecord {
            id: format!("tok_{}", &hash[..12]),
            token_hash: hash.clone(),
            user_id: user_id.to_string(),
            token_type: TokenType::Client,
            name: name.to_string(),
            service_rules: None,
            created_at_unix_ms: now,
            expires_at_unix_ms,
            last_used_unix_ms: 0,
        };

        {
            let mut guard = self.state.write().await;
            guard.tokens.insert(hash, record.clone());
        }
        let _ = self.save_state().await;
        Ok((raw, record))
    }

    pub async fn is_auth_enabled(&self) -> bool {
        if self
            .config
            .github
            .as_ref()
            .map(|g| g.enabled)
            .unwrap_or(false)
        {
            return true;
        }
        let st = self.state.read().await;
        !st.tokens.is_empty() || !st.users.is_empty()
    }

    /// Creates a new admin token for a user.
    #[allow(dead_code)]
    pub async fn create_admin_token(
        &self,
        user_id: &str,
        name: &str,
    ) -> anyhow::Result<(String, TokenRecord)> {
        let (raw, hash) = generate_token("prism_adm_");
        let now = now_unix_ms();

        let record = TokenRecord {
            id: format!("tok_{}", &hash[..12]),
            token_hash: hash.clone(),
            user_id: user_id.to_string(),
            token_type: TokenType::Admin,
            name: name.to_string(),
            service_rules: Some(vec!["*".to_string()]),
            created_at_unix_ms: now,
            expires_at_unix_ms: None,
            last_used_unix_ms: 0,
        };

        {
            let mut guard = self.state.write().await;
            guard.tokens.insert(hash, record.clone());
        }
        let _ = self.save_state().await;
        Ok((raw, record))
    }

    /// Revokes a token by its token ID (`tok_...`).
    pub async fn revoke_token(&self, token_id: &str) -> bool {
        let mut guard = self.state.write().await;
        let mut target_hash = None;
        for (hash, tok) in guard.tokens.iter() {
            if tok.id == token_id {
                target_hash = Some(hash.clone());
                break;
            }
        }
        if let Some(hash) = target_hash {
            guard.tokens.remove(&hash);
            drop(guard);
            let _ = self.save_state().await;
            true
        } else {
            false
        }
    }

    /// Lists tokens, optionally filtered by user ID.
    pub async fn list_tokens(&self, user_id: Option<&str>) -> Vec<TokenRecord> {
        let guard = self.state.read().await;
        guard
            .tokens
            .values()
            .filter(|t| match user_id {
                Some(uid) => t.user_id == uid,
                None => true,
            })
            .cloned()
            .collect()
    }

    /// Lists all users.
    pub async fn list_users(&self) -> Vec<UserRecord> {
        let guard = self.state.read().await;
        guard.users.values().cloned().collect()
    }

    /// Gets a user by user_id.
    pub async fn get_user(&self, user_id: &str) -> Option<UserRecord> {
        let guard = self.state.read().await;
        guard.users.get(user_id).cloned()
    }

    /// Upserts user record.
    pub async fn upsert_user(&self, user: UserRecord) -> anyhow::Result<()> {
        {
            let mut guard = self.state.write().await;
            guard.users.insert(user.id.clone(), user);
        }
        self.save_state().await
    }

    /// Initiates GitHub Device Authorization Flow.
    pub async fn request_device_code(&self) -> anyhow::Result<DeviceCodeResponse> {
        let Some(gh) = self.github_config() else {
            anyhow::bail!("GitHub OAuth is not configured or enabled");
        };

        let res = self
            .http_client
            .post("https://github.com/login/device/code")
            .header("Accept", "application/json")
            .header("User-Agent", "prism-proxy")
            .json(&serde_json::json!({
                "client_id": &gh.client_id,
                "scope": "read:user"
            }))
            .send()
            .await?;

        if !res.status().is_success() {
            let body = res.text().await.unwrap_or_default();
            anyhow::bail!("GitHub device code request failed: {body}");
        }

        let resp: DeviceCodeResponse = res.json().await?;
        Ok(resp)
    }

    /// Polls GitHub Device Flow for authorization status.
    pub async fn poll_device_code(&self, device_code: &str) -> anyhow::Result<DevicePollResult> {
        let Some(gh) = self.github_config() else {
            anyhow::bail!("GitHub OAuth is not configured or enabled");
        };

        #[derive(Deserialize)]
        struct PollResponse {
            access_token: Option<String>,
            error: Option<String>,
            interval: Option<u64>,
        }

        let res = self
            .http_client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .header("User-Agent", "prism-proxy")
            .json(&serde_json::json!({
                "client_id": &gh.client_id,
                "device_code": device_code,
                "grant_type": "urn:ietf:params:oauth:grant-type:device_code"
            }))
            .send()
            .await?;

        let parsed: PollResponse = res.json().await?;

        if let Some(err) = parsed.error {
            match err.as_str() {
                "authorization_pending" => return Ok(DevicePollResult::Pending),
                "slow_down" => {
                    return Ok(DevicePollResult::SlowDown {
                        interval: parsed.interval.unwrap_or(5),
                    });
                }
                "expired_token" => return Ok(DevicePollResult::Expired),
                "access_denied" => {
                    return Ok(DevicePollResult::Denied {
                        message: "Access was denied by user".to_string(),
                    });
                }
                other => {
                    anyhow::bail!("GitHub error: {other}");
                }
            }
        }

        let Some(token) = parsed.access_token else {
            return Ok(DevicePollResult::Pending);
        };

        // Fetch user profile from GitHub
        let (gh_user, orgs) = self.fetch_github_profile(&token).await?;
        let (user, raw_token, token_record) = self
            .on_oauth_success(gh_user, &orgs, "GitHub Device Login")
            .await?;

        Ok(DevicePollResult::Success {
            user,
            token: raw_token,
            token_id: token_record.id,
        })
    }

    /// Exchanges Web OAuth Code for token and user profile.
    pub async fn exchange_web_code(
        &self,
        code: &str,
    ) -> anyhow::Result<(UserRecord, String, TokenRecord)> {
        let Some(gh) = self.github_config() else {
            anyhow::bail!("GitHub OAuth is not configured or enabled");
        };

        #[derive(Deserialize)]
        struct CodeResponse {
            access_token: Option<String>,
            error: Option<String>,
        }

        let mut payload = serde_json::json!({
            "client_id": &gh.client_id,
            "client_secret": &gh.client_secret,
            "code": code
        });
        if let Some(ref r) = gh.redirect_uri {
            payload["redirect_uri"] = serde_json::Value::String(r.clone());
        }

        let res = self
            .http_client
            .post("https://github.com/login/oauth/access_token")
            .header("Accept", "application/json")
            .header("User-Agent", "prism-proxy")
            .json(&payload)
            .send()
            .await?;

        let parsed: CodeResponse = res.json().await?;
        if let Some(err) = parsed.error {
            anyhow::bail!("GitHub code exchange error: {err}");
        }
        let Some(token) = parsed.access_token else {
            anyhow::bail!("Missing access token from GitHub code exchange");
        };

        let (gh_user, orgs) = self.fetch_github_profile(&token).await?;
        self.on_oauth_success(gh_user, &orgs, "GitHub Web Login")
            .await
    }

    async fn fetch_github_profile(
        &self,
        access_token: &str,
    ) -> anyhow::Result<(GitHubUser, Vec<String>)> {
        let user_res = self
            .http_client
            .get("https://api.github.com/user")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "prism-proxy")
            .send()
            .await?;

        if !user_res.status().is_success() {
            anyhow::bail!("Failed to fetch GitHub user info");
        }
        let gh_user: GitHubUser = user_res.json().await?;

        // Fetch orgs
        let orgs_res = self
            .http_client
            .get("https://api.github.com/user/orgs")
            .header("Authorization", format!("Bearer {access_token}"))
            .header("User-Agent", "prism-proxy")
            .send()
            .await;

        let orgs: Vec<String> = match orgs_res {
            Ok(r) if r.status().is_success() => r
                .json::<Vec<GitHubOrg>>()
                .await
                .map(|list| list.into_iter().map(|o| o.login).collect())
                .unwrap_or_default(),
            _ => Vec::new(),
        };

        Ok((gh_user, orgs))
    }

    /// Handles successful GitHub authentication, role check, and token generation.
    async fn on_oauth_success(
        &self,
        gh_user: GitHubUser,
        orgs: &[String],
        token_name: &str,
    ) -> anyhow::Result<(UserRecord, String, TokenRecord)> {
        let gh_cfg = self
            .config
            .github
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("GitHub OAuth configuration missing"))?;

        let user_id = format!("gh_{}", gh_user.id);
        let username = gh_user.login.clone();

        // 1. Check if user is allowed
        let is_allowed = {
            if gh_cfg.allowed_users.is_empty() && gh_cfg.allowed_orgs.is_empty() {
                true
            } else {
                let user_matched = gh_cfg
                    .allowed_users
                    .iter()
                    .any(|u| u.eq_ignore_ascii_case(&username));
                let org_matched = orgs.iter().any(|org| {
                    gh_cfg
                        .allowed_orgs
                        .iter()
                        .any(|ao| ao.eq_ignore_ascii_case(org))
                });
                user_matched || org_matched
            }
        };

        if !is_allowed {
            anyhow::bail!("User '{username}' is not in allowed users or orgs");
        }

        // 2. Check if user is Admin
        let is_admin = {
            let admin_user = gh_cfg
                .admin_users
                .iter()
                .any(|u| u.eq_ignore_ascii_case(&username));
            let admin_org = orgs.iter().any(|org| {
                gh_cfg
                    .admin_orgs
                    .iter()
                    .any(|ao| ao.eq_ignore_ascii_case(org))
            });
            admin_user || admin_org
        };

        let now = now_unix_ms();
        let mut guard = self.state.write().await;
        let existing = guard.users.get(&user_id).cloned();

        let role = if is_admin {
            UserRole::Admin
        } else if let Some(ref e) = existing {
            e.role
        } else if gh_cfg.default_role.eq_ignore_ascii_case("admin") {
            UserRole::Admin
        } else {
            UserRole::Member
        };

        let user_record = UserRecord {
            id: user_id.clone(),
            username,
            display_name: gh_user.name,
            avatar_url: gh_user.avatar_url,
            role,
            service_rules: existing
                .as_ref()
                .map(|e| e.service_rules.clone())
                .unwrap_or_else(default_rules),
            created_at_unix_ms: existing
                .as_ref()
                .map(|e| e.created_at_unix_ms)
                .unwrap_or(now),
            last_login_unix_ms: now,
        };

        guard.users.insert(user_id.clone(), user_record.clone());
        drop(guard);

        // 3. Generate token
        let token_prefix = if role == UserRole::Admin {
            "prism_adm_"
        } else {
            "prism_cl_"
        };
        let (raw, hash) = generate_token(token_prefix);
        let token_record = TokenRecord {
            id: format!("tok_{}", &hash[..12]),
            token_hash: hash.clone(),
            user_id,
            token_type: if role == UserRole::Admin {
                TokenType::Admin
            } else {
                TokenType::Client
            },
            name: token_name.to_string(),
            service_rules: None,
            created_at_unix_ms: now,
            expires_at_unix_ms: None,
            last_used_unix_ms: now,
        };

        {
            let mut guard = self.state.write().await;
            guard.tokens.insert(hash, token_record.clone());
        }

        let _ = self.save_state().await;
        Ok((user_record, raw, token_record))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_token_generation_and_verification() {
        let auth = AuthManager::new(AuthConfig::default(), None);

        // Add a member user
        let user = UserRecord {
            id: "user_1".into(),
            username: "alice".into(),
            display_name: Some("Alice".into()),
            avatar_url: None,
            role: UserRole::Member,
            service_rules: vec!["minecraft-*".into(), "web".into()],
            created_at_unix_ms: now_unix_ms(),
            last_login_unix_ms: now_unix_ms(),
        };
        auth.upsert_user(user).await.unwrap();

        // Create client token
        let (raw_token, record) = auth
            .create_client_token("user_1", "Alice Laptop", None)
            .await
            .unwrap();
        assert!(raw_token.starts_with("prism_cl_"));
        assert_eq!(record.user_id, "user_1");

        // Verify token
        let ident = auth.verify_token(&raw_token).await.expect("valid token");
        assert_eq!(ident.username, "alice");
        assert_eq!(ident.role, UserRole::Member);
        assert!(!ident.is_admin);

        // Check ACL matching
        assert!(ident.can_access_service("minecraft-survival"));
        assert!(ident.can_access_service("minecraft-bedrock"));
        assert!(ident.can_access_service("web"));
        assert!(!ident.can_access_service("internal-db"));
        assert!(!ident.can_access_service("ssh"));

        // Revoke token
        assert!(auth.revoke_token(&record.id).await);
        assert!(auth.verify_token(&raw_token).await.is_none());
    }

    #[tokio::test]
    async fn test_legacy_token_fallback() {
        let mut cfg = AuthConfig::default();
        cfg.legacy_token = Some("my-legacy-secret".into());
        let auth = AuthManager::new(cfg, None);

        let ident = auth
            .verify_token("my-legacy-secret")
            .await
            .expect("matches legacy token");
        assert!(ident.is_admin);
        assert!(ident.can_access_service("anything"));

        assert!(auth.verify_token("wrong-secret").await.is_none());
    }

    #[test]
    fn test_service_rule_patterns() {
        assert!(match_service_rule("*", "any-service"));
        assert!(match_service_rule("minecraft-*", "minecraft-1"));
        assert!(match_service_rule("minecraft-*", "minecraft-survival"));
        assert!(!match_service_rule("minecraft-*", "web-server"));
        assert!(match_service_rule("web", "WEB"));
        assert!(!match_service_rule("web", "website"));
    }
}
