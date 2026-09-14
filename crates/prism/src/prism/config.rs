use std::{
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use directories::ProjectDirs;
use serde::{Deserialize, Deserializer, Serialize};

fn deserialize_ignored_any<'de, D: Deserializer<'de>>(deserializer: D) -> Result<(), D::Error> {
    let _ = serde::de::IgnoredAny::deserialize(deserializer)?;
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ResolvedConfigPath {
    pub path: PathBuf,
    pub source: ConfigPathSource,
}

#[derive(Debug, Clone, Copy)]
pub enum ConfigPathSource {
    Flag,
    Env,
    Cwd,
    Default,
}

impl std::fmt::Display for ConfigPathSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigPathSource::Flag => write!(f, "flag"),
            ConfigPathSource::Env => write!(f, "env"),
            ConfigPathSource::Cwd => write!(f, "cwd"),
            ConfigPathSource::Default => write!(f, "default"),
        }
    }
}

pub fn resolve_config_path(
    explicit_flag_path: Option<PathBuf>,
) -> anyhow::Result<ResolvedConfigPath> {
    if let Some(p) = explicit_flag_path {
        let p = normalize_explicit_path(&p)?;
        return Ok(ResolvedConfigPath {
            path: p,
            source: ConfigPathSource::Flag,
        });
    }

    // clap already maps PRISM_CONFIG into the flag value when unset, but keep the design's precedence
    // clear by treating it as "env" when present.
    if let Some(p) = std::env::var_os("PRISM_CONFIG")
        && !p.is_empty()
    {
        let p = normalize_explicit_path(Path::new(&p))?;
        return Ok(ResolvedConfigPath {
            path: p,
            source: ConfigPathSource::Env,
        });
    }

    if let Ok(p) = discover_config_path(Path::new(".")) {
        return Ok(ResolvedConfigPath {
            path: p,
            source: ConfigPathSource::Cwd,
        });
    }

    Ok(ResolvedConfigPath {
        path: default_config_path()?,
        source: ConfigPathSource::Default,
    })
}

fn normalize_explicit_path(p: &Path) -> anyhow::Result<PathBuf> {
    let p = p.to_path_buf();

    if p.as_os_str().is_empty() {
        anyhow::bail!("config: empty config path");
    }

    let meta = fs::metadata(&p);
    if let Ok(m) = meta {
        if m.is_dir() {
            if let Ok(discovered) = discover_config_path(&p) {
                return Ok(discovered);
            }
            return Ok(p.join("prism.toml"));
        }
        return Ok(p);
    }

    // Non-existent path: default to .toml if no extension.
    let mut out = p;
    if out.extension().is_none() {
        out.set_extension("toml");
    }
    Ok(out)
}

fn discover_config_path(dir: &Path) -> anyhow::Result<PathBuf> {
    let candidates = ["prism.toml", "prism.yaml", "prism.yml"];
    for c in candidates {
        let p = dir.join(c);
        if let Ok(m) = fs::metadata(&p)
            && m.is_file()
        {
            return Ok(p);
        }
    }
    anyhow::bail!("config: no prism.* found")
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    // Linux: system-wide default.
    #[cfg(target_os = "linux")]
    {
        return Ok(PathBuf::from("/etc/prism/prism.toml"));
    }

    // Other OSes: per-user config dir.
    #[cfg(not(target_os = "linux"))]
    {
        let proj = ProjectDirs::from("com", "summpot", "prism")
            .context("config: resolve user config dir")?;
        Ok(proj.config_dir().join("prism.toml"))
    }
}

pub fn ensure_config_file(path: &Path) -> anyhow::Result<bool> {
    if path.as_os_str().is_empty() {
        anyhow::bail!("config: empty config path");
    }

    match fs::metadata(path) {
        Ok(m) => {
            if m.is_file() {
                return Ok(false);
            }
            anyhow::bail!(
                "config: {} exists but is not a regular file",
                path.display()
            );
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err).with_context(|| format!("config: stat {}", path.display())),
    }

    let tmpl = default_config_template_for_path(path)?;

    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("config: mkdir {}", parent.display()))?;
    }

    // Create once (O_EXCL equivalent).
    let mut opts = fs::OpenOptions::new();
    opts.write(true).create_new(true);
    let mut f = opts
        .open(path)
        .with_context(|| format!("config: create {}", path.display()))?;
    use std::io::Write;
    f.write_all(tmpl.as_bytes())
        .with_context(|| format!("config: write {}", path.display()))?;
    Ok(true)
}

fn default_config_template_for_path(path: &Path) -> anyhow::Result<&'static str> {
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    match ext.as_str() {
        "toml" => Ok(DEFAULT_CONFIG_TEMPLATE_TOML),
        "yaml" | "yml" => Ok(DEFAULT_CONFIG_TEMPLATE_YAML),
        _ => anyhow::bail!(
            "config: unsupported config extension {:?} (expected .toml or .yaml/.yml)",
            path.extension()
        ),
    }
}

pub fn load_config(path: &Path) -> anyhow::Result<Config> {
    let data = fs::read(path).with_context(|| format!("read {}", path.display()))?;
    let s = String::from_utf8_lossy(&data);

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();

    let mut fc: FileConfig = match ext.as_str() {
        "toml" => toml::from_str(&s).with_context(|| format!("parse toml {}", path.display()))?,
        "yaml" | "yml" => {
            serde_yaml::from_str(&s).with_context(|| format!("parse yaml {}", path.display()))?
        }
        _ => anyhow::bail!("config: unsupported config extension {}", ext),
    };

    Config::from_file_config(&mut fc, path)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub listeners: Vec<ProxyListenerConfig>,
    pub admin_addr: String,
    pub logging: LoggingConfig,
    pub routes: Vec<RouteConfig>,
    pub max_header_bytes: usize,
    pub reload: ReloadConfig,
    pub proxy_protocol_v2: bool,
    pub buffer_size: usize,
    pub upstream_dial_timeout: Duration,
    pub timeouts: Timeouts,
    pub tunnel: TunnelConfig,
    pub auth: crate::prism::auth::AuthConfig,
    pub acme: Option<AcmeConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Timeouts {
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProxyListenerConfig {
    pub listen_addr: String,
    pub protocol: String, // tcp | udp
    pub upstream: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReloadConfig {
    pub enabled: bool,
    pub poll_interval: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub output: String,
    pub add_source: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RouteConfig {
    pub host: Vec<String>,
    pub upstreams: Vec<String>,
    pub middlewares: Vec<String>,
    pub strategy: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct MdnsConfig {
    pub enabled: bool,
    /// Domain suffix, default "local".
    pub domain: String,
    /// Optional subdomain label, e.g. "prism" -> <name>.prism.local
    pub subdomain: String,
    /// Listen address for the local proxy (e.g. ":25565"), bound on 0.0.0.0 for LAN access.
    pub listen_addr: String,
    /// Middleware names used for hostname extraction on the local proxy.
    pub middlewares: Vec<String>,
    /// Whether to broadcast Minecraft LAN discovery packets (224.0.2.60:4445).
    pub minecraft_lan: bool,
    /// MOTD prefix for Minecraft LAN broadcast reflection.
    pub motd_prefix: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TunnelConfig {
    pub auth_token: String,
    pub auto_listen_services: bool,
    pub endpoints: Vec<TunnelEndpointConfig>,
    pub connector: Option<TunnelConnectorConfig>,
    pub client: Option<TunnelClientConfig>,
    pub services: Vec<TunnelServiceConfig>,
    pub mdns: MdnsConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelEndpointConfig {
    pub listen_addr: String,
    pub transport: String,
    pub quic: QuicServerConfig,
    pub websocket: WebSocketServerConfig,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelConnectorConfig {
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub dial_timeout_ms: Option<i64>,
    pub dial_timeout: Duration,
    pub quic: Option<QuicClientConfig>,
    pub websocket: Option<WebSocketClientConfig>,
    pub doh_servers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelClientConfig {
    pub server_addr: String,
    pub transport: String,
    pub auth_token: String,
    pub listen_addr: String,
    pub middleware: Option<String>,
    pub fake_lan_broadcast: bool,
    pub motd_prefix: String,
    pub optimizer: Option<OptimizerClientConfig>,
    pub websocket: Option<WebSocketClientConfig>,
    pub doh_servers: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizerClientConfig {
    pub enabled: bool,
    pub zstd_window_log: Option<u32>,
    pub zstd_window_log_uplink: Option<u32>,
    pub zstd_window_log_downlink: Option<u32>,
    pub zstd_dictionary: Option<String>,
}

impl Default for OptimizerClientConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            zstd_window_log: Some(23),
            zstd_window_log_uplink: Some(18),
            zstd_window_log_downlink: Some(23),
            zstd_dictionary: None,
        }
    }
}

#[allow(dead_code)]
impl OptimizerClientConfig {
    pub fn zstd_window_log(&self) -> u32 {
        self.zstd_window_log.unwrap_or(23)
    }

    pub fn zstd_window_log_uplink(&self) -> u32 {
        self.zstd_window_log_uplink
            .or(self.zstd_window_log)
            .unwrap_or(18)
    }

    pub fn zstd_window_log_downlink(&self) -> u32 {
        self.zstd_window_log_downlink
            .or(self.zstd_window_log)
            .unwrap_or(23)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuicServerConfig {
    pub cert_file: String,
    pub key_file: String,
    pub use_acme: Option<bool>,
}

impl QuicServerConfig {
    pub fn should_use_acme(&self, acme_enabled: bool) -> bool {
        match self.use_acme {
            Some(v) => v,
            None => acme_enabled && self.cert_file.trim().is_empty(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QuicClientConfig {
    pub server_name: String,
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebSocketServerConfig {
    pub cert_file: String,
    pub key_file: String,
    pub use_acme: Option<bool>,
}

impl WebSocketServerConfig {
    pub fn should_use_acme(&self, acme_enabled: bool) -> bool {
        match self.use_acme {
            Some(v) => v,
            None => acme_enabled && self.cert_file.trim().is_empty(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WebSocketClientConfig {
    pub server_name: String,
    pub insecure_skip_verify: bool,
}

#[derive(Clone, PartialEq, Eq)]
pub struct CloudflareAcmeConfig {
    pub api_token: String,
    pub zone_id: String,
    pub propagation_timeout_secs: u64,
}

impl std::fmt::Debug for CloudflareAcmeConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudflareAcmeConfig")
            .field("api_token", &if self.api_token.is_empty() { "" } else { "***" })
            .field("zone_id", &self.zone_id)
            .field("propagation_timeout_secs", &self.propagation_timeout_secs)
            .finish()
    }
}

impl Default for CloudflareAcmeConfig {
    fn default() -> Self {
        Self {
            api_token: String::new(),
            zone_id: String::new(),
            propagation_timeout_secs: 120,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcmeConfig {
    pub enabled: bool,
    pub domains: Vec<String>,
    pub email: Option<String>,
    pub directory_url: String,
    pub storage_dir: String,
    pub cert_file: String,
    pub key_file: String,
    pub renew_before_days: u32,
    pub auto_renew: bool,
    pub cloudflare: CloudflareAcmeConfig,
}

impl Default for AcmeConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            domains: Vec::new(),
            email: None,
            directory_url: "production".to_string(),
            storage_dir: String::new(),
            cert_file: String::new(),
            key_file: String::new(),
            renew_before_days: 30,
            auto_renew: true,
            cloudflare: CloudflareAcmeConfig::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OptimizerConfig {
    pub enabled: bool,
    pub flush_interval_ms: Option<u64>,
    pub flush_interval_uplink_ms: Option<u64>,
    pub flush_interval_min_ms: Option<u64>,
    pub flush_interval_max_ms: Option<u64>,
    pub adaptive_flush: Option<bool>,
    pub buffer_threshold: Option<usize>,
    pub buffer_threshold_uplink: Option<usize>,
    pub zstd_window_log: Option<u32>,
    pub zstd_window_log_uplink: Option<u32>,
    pub zstd_window_log_downlink: Option<u32>,
    pub zstd_level: Option<i32>,
    pub zstd_dictionary: Option<String>,
}

impl Default for OptimizerConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            flush_interval_ms: Some(20),
            flush_interval_uplink_ms: Some(8),
            flush_interval_min_ms: Some(8),
            flush_interval_max_ms: Some(50),
            adaptive_flush: Some(true),
            buffer_threshold: Some(64 * 1024),
            buffer_threshold_uplink: Some(16 * 1024),
            zstd_window_log: Some(23),
            zstd_window_log_uplink: Some(18),
            zstd_window_log_downlink: Some(23),
            zstd_level: Some(3),
            zstd_dictionary: None,
        }
    }
}

#[allow(dead_code)]
impl OptimizerConfig {
    pub fn flush_interval_ms(&self) -> u64 {
        self.flush_interval_ms.unwrap_or(20)
    }

    pub fn flush_interval_uplink_ms(&self) -> u64 {
        self.flush_interval_uplink_ms.unwrap_or(8)
    }

    pub fn flush_interval_min_ms(&self) -> u64 {
        self.flush_interval_min_ms.unwrap_or(8)
    }

    pub fn flush_interval_max_ms(&self) -> u64 {
        self.flush_interval_max_ms.unwrap_or(50)
    }

    pub fn adaptive_flush(&self) -> bool {
        self.adaptive_flush.unwrap_or(true)
    }

    pub fn buffer_threshold(&self) -> usize {
        self.buffer_threshold.unwrap_or(64 * 1024)
    }

    pub fn buffer_threshold_uplink(&self) -> usize {
        self.buffer_threshold_uplink.unwrap_or(16 * 1024)
    }

    pub fn zstd_window_log(&self) -> u32 {
        self.zstd_window_log.unwrap_or(23)
    }

    pub fn zstd_window_log_uplink(&self) -> u32 {
        self.zstd_window_log_uplink
            .or(self.zstd_window_log)
            .unwrap_or(18)
    }

    pub fn zstd_window_log_downlink(&self) -> u32 {
        self.zstd_window_log_downlink
            .or(self.zstd_window_log)
            .unwrap_or(23)
    }

    pub fn zstd_level(&self) -> i32 {
        self.zstd_level.unwrap_or(3)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TunnelServiceConfig {
    pub name: String,
    pub proto: String,
    pub local_addr: String,
    pub route_only: bool,
    pub remote_addr: String,
    /// Advanced host label used for rewrite middlewares when this service is dialed as
    /// an upstream (tunnel:<service>). Leave empty to preserve the client's protocol host.
    /// This supports $1, $2... substitutions from route wildcard captures.
    pub masquerade_host: String,
    pub middleware: Option<String>,
    pub optimizer: Option<OptimizerConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileConfig {
    /// Accepted and ignored for backward compatibility (role concept removed).
    #[serde(default, deserialize_with = "deserialize_ignored_any")]
    #[allow(dead_code)]
    role: (),

    #[serde(default)]
    listeners: Vec<FileProxyListener>,

    #[serde(default)]
    admin_addr: String,

    /// Accepted and ignored for backward compatibility (metrics support removed).
    #[serde(default, deserialize_with = "deserialize_ignored_any")]
    #[allow(dead_code)]
    metrics: (),

    logging: Option<FileLogging>,

    #[serde(default)]
    routes: Vec<FileRoute>,

    #[serde(default)]
    max_header_bytes: i64,

    reload: Option<FileReload>,

    #[serde(default)]
    proxy_protocol_v2: bool,

    #[serde(default)]
    buffer_size: i64,

    #[serde(default)]
    upstream_dial_timeout_ms: i64,

    timeouts: Option<FileTimeouts>,

    tunnel: Option<FileTunnel>,

    auth: Option<FileAuthConfig>,

    acme: Option<FileAcmeConfig>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileCloudflareConfig {
    api_token: Option<String>,
    zone_id: Option<String>,
    propagation_timeout_secs: Option<u64>,
}

impl std::fmt::Debug for FileCloudflareConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileCloudflareConfig")
            .field("api_token", &self.api_token.as_ref().map(|_| "***"))
            .field("zone_id", &self.zone_id)
            .field("propagation_timeout_secs", &self.propagation_timeout_secs)
            .finish()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAcmeConfig {
    #[serde(default)]
    enabled: bool,
    #[serde(default)]
    domains: Option<StringOrVec>,
    email: Option<String>,
    directory_url: Option<String>,
    storage_dir: Option<String>,
    cert_file: Option<String>,
    key_file: Option<String>,
    renew_before_days: Option<u32>,
    #[serde(default)]
    auto_renew: Option<bool>,
    cloudflare: Option<FileCloudflareConfig>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileProxyListener {
    listen_addr: String,
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    upstream: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileLogging {
    level: Option<String>,
    format: Option<String>,
    output: Option<String>,
    #[serde(default)]
    add_source: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileReload {
    #[serde(default)]
    enabled: bool,
    poll_interval_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTimeouts {
    handshake_timeout_ms: Option<i64>,
    idle_timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileRoute {
    host: Option<StringOrVec>,
    hosts: Option<StringOrVec>,
    upstream: Option<StringOrVec>,
    upstreams: Option<StringOrVec>,
    backend: Option<StringOrVec>,
    backends: Option<StringOrVec>,

    middlewares: Option<StringOrVec>,
    // Back-compat alias (deprecated): `parsers`.
    parsers: Option<StringOrVec>,

    strategy: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTunnel {
    auth_token: Option<String>,
    auto_listen_services: Option<bool>,
    endpoints: Option<Vec<FileTunnelEndpoint>>,
    connector: Option<FileTunnelConnector>,
    client: Option<FileTunnelClient>,
    services: Option<Vec<FileTunnelService>>,
    mdns: Option<FileMdns>,
    acme: Option<FileAcmeConfig>,
}

impl std::fmt::Debug for FileTunnel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTunnel")
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***"))
            .field("auto_listen_services", &self.auto_listen_services)
            .field("endpoints", &self.endpoints)
            .field("connector", &self.connector)
            .field("client", &self.client)
            .field("services", &self.services)
            .field("mdns", &self.mdns)
            .field("acme", &self.acme)
            .finish()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileMdns {
    #[serde(default)]
    enabled: bool,
    domain: Option<String>,
    subdomain: Option<String>,
    listen_addr: Option<String>,
    middlewares: Option<StringOrVec>,
    #[serde(default)]
    minecraft_lan: bool,
    #[serde(default)]
    fake_lan_broadcast: bool,
    motd_prefix: Option<String>,
    discovery: Option<FileClientDiscovery>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTunnelEndpoint {
    listen_addr: String,
    transport: Option<String>,
    quic: Option<FileQuicServer>,
    websocket: Option<FileWebSocketServer>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTunnelConnector {
    server_addr: String,
    transport: Option<String>,
    auth_token: Option<String>,
    dial_timeout_ms: Option<i64>,
    quic: Option<FileQuicClient>,
    websocket: Option<FileWebSocketClient>,
    #[serde(default)]
    doh_servers: Option<StringOrVec>,
}

impl std::fmt::Debug for FileTunnelConnector {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTunnelConnector")
            .field("server_addr", &self.server_addr)
            .field("transport", &self.transport)
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***"))
            .field("dial_timeout_ms", &self.dial_timeout_ms)
            .field("quic", &self.quic)
            .field("websocket", &self.websocket)
            .field("doh_servers", &self.doh_servers)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTunnelClient {
    server_addr: String,
    transport: Option<String>,
    auth_token: Option<String>,
    listen_addr: Option<String>,
    middleware: Option<String>,
    #[serde(default)]
    fake_lan_broadcast: bool,
    motd_prefix: Option<String>,
    optimizer: Option<FileOptimizerClient>,
    discovery: Option<FileClientDiscovery>,
    websocket: Option<FileWebSocketClient>,
    #[serde(default)]
    doh_servers: Option<StringOrVec>,
}

impl std::fmt::Debug for FileTunnelClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileTunnelClient")
            .field("server_addr", &self.server_addr)
            .field("transport", &self.transport)
            .field("auth_token", &self.auth_token.as_ref().map(|_| "***"))
            .field("listen_addr", &self.listen_addr)
            .field("middleware", &self.middleware)
            .field("fake_lan_broadcast", &self.fake_lan_broadcast)
            .field("motd_prefix", &self.motd_prefix)
            .field("optimizer", &self.optimizer)
            .field("discovery", &self.discovery)
            .field("websocket", &self.websocket)
            .field("doh_servers", &self.doh_servers)
            .finish()
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileClientDiscovery {
    minecraft_lan: Option<FileMinecraftLanDiscovery>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileMinecraftLanDiscovery {
    #[serde(default)]
    enabled: bool,
    motd_prefix: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOptimizerClient {
    #[serde(default)]
    enabled: bool,
    zstd_window_log: Option<u32>,
    zstd_window_log_uplink: Option<u32>,
    zstd_window_log_downlink: Option<u32>,
    zstd_dictionary: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileQuicServer {
    cert_file: Option<String>,
    key_file: Option<String>,
    use_acme: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileQuicClient {
    server_name: Option<String>,
    #[serde(default)]
    insecure_skip_verify: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileWebSocketServer {
    cert_file: Option<String>,
    key_file: Option<String>,
    use_acme: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileWebSocketClient {
    server_name: Option<String>,
    #[serde(default)]
    insecure_skip_verify: bool,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileTunnelService {
    name: String,
    proto: Option<String>,
    local_addr: String,
    #[serde(default)]
    route_only: bool,
    remote_addr: Option<String>,
    masquerade_host: Option<String>,
    middleware: Option<String>,
    optimizer: Option<FileOptimizer>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct FileOptimizer {
    #[serde(default)]
    enabled: bool,
    flush_interval_ms: Option<u64>,
    flush_interval_uplink_ms: Option<u64>,
    flush_interval_min_ms: Option<u64>,
    flush_interval_max_ms: Option<u64>,
    adaptive_flush: Option<bool>,
    buffer_threshold: Option<usize>,
    buffer_threshold_uplink: Option<usize>,
    zstd_window_log: Option<u32>,
    zstd_window_log_uplink: Option<u32>,
    zstd_window_log_downlink: Option<u32>,
    zstd_level: Option<i32>,
    zstd_dictionary: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileAuthConfig {
    #[serde(default)]
    mode: Option<String>,
    #[serde(default)]
    legacy_token: Option<String>,
    github: Option<FileGitHubOAuthConfig>,
}

impl std::fmt::Debug for FileAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileAuthConfig")
            .field("mode", &self.mode)
            .field("legacy_token", &self.legacy_token.as_ref().map(|_| "***"))
            .field("github", &self.github)
            .finish()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FileGitHubOAuthConfig {
    #[serde(default)]
    enabled: bool,
    client_id: Option<String>,
    client_secret: Option<String>,
    redirect_uri: Option<String>,
    #[serde(default)]
    admin_users: Option<StringOrVec>,
    #[serde(default)]
    admin_orgs: Option<StringOrVec>,
    #[serde(default)]
    allowed_users: Option<StringOrVec>,
    #[serde(default)]
    allowed_orgs: Option<StringOrVec>,
    default_role: Option<String>,
}

impl std::fmt::Debug for FileGitHubOAuthConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FileGitHubOAuthConfig")
            .field("enabled", &self.enabled)
            .field("client_id", &self.client_id)
            .field("client_secret", &self.client_secret.as_ref().map(|_| "***"))
            .field("redirect_uri", &self.redirect_uri)
            .field("admin_users", &self.admin_users)
            .field("admin_orgs", &self.admin_orgs)
            .field("allowed_users", &self.allowed_users)
            .field("allowed_orgs", &self.allowed_orgs)
            .field("default_role", &self.default_role)
            .finish()
    }
}

#[derive(Debug, Deserialize, Clone)]
#[serde(untagged)]
enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

impl StringOrVec {
    fn into_vec(self) -> Vec<String> {
        match self {
            StringOrVec::One(s) => vec![s],
            StringOrVec::Many(v) => v,
        }
    }
}

impl Config {
    fn from_file_config(fc: &mut FileConfig, _config_path: &Path) -> anyhow::Result<Config> {
        let mut cfg = Config {
            listeners: vec![],
            admin_addr: fc.admin_addr.trim().to_string(),
            logging: LoggingConfig {
                level: "info".into(),
                format: "json".into(),
                output: "stderr".into(),
                add_source: false,
            },
            routes: vec![],
            max_header_bytes: fc.max_header_bytes as usize,
            reload: ReloadConfig {
                enabled: fc.reload.as_ref().map(|r| r.enabled).unwrap_or(true),
                poll_interval: Duration::from_millis(
                    fc.reload
                        .as_ref()
                        .and_then(|r| r.poll_interval_ms)
                        .unwrap_or(1000)
                        .max(0) as u64,
                ),
            },
            proxy_protocol_v2: fc.proxy_protocol_v2,
            buffer_size: (fc.buffer_size).max(0) as usize,
            upstream_dial_timeout: Duration::from_millis(
                (fc.upstream_dial_timeout_ms).max(0) as u64
            ),
            timeouts: Timeouts {
                handshake_timeout: Duration::from_millis(
                    fc.timeouts
                        .as_ref()
                        .and_then(|t| t.handshake_timeout_ms)
                        .unwrap_or(3000)
                        .max(0) as u64,
                ),
                idle_timeout: Duration::from_millis(
                    fc.timeouts
                        .as_ref()
                        .and_then(|t| t.idle_timeout_ms)
                        .unwrap_or(0)
                        .max(0) as u64,
                ),
            },
            tunnel: TunnelConfig::default(),
            auth: crate::prism::auth::AuthConfig::default(),
            acme: None,
        };

        if cfg.max_header_bytes == 0 {
            cfg.max_header_bytes = 64 * 1024;
        }
        if cfg.buffer_size == 0 {
            cfg.buffer_size = 32 * 1024;
        }
        if cfg.upstream_dial_timeout == Duration::from_millis(0) {
            cfg.upstream_dial_timeout = Duration::from_millis(5000);
        }

        // --- Listeners ---
        for l in &fc.listeners {
            let proto = if l.protocol.trim().is_empty() {
                "tcp".to_string()
            } else {
                l.protocol.trim().to_ascii_lowercase()
            };
            cfg.listeners.push(ProxyListenerConfig {
                listen_addr: l.listen_addr.trim().to_string(),
                protocol: proto,
                upstream: l.upstream.trim().to_string(),
            });
        }

        // --- Routes ---
        if !fc.routes.is_empty() {
            for (i, r) in fc.routes.iter().enumerate() {
                let mut hosts: Vec<String> = vec![];
                if let Some(h) = r.host.clone() {
                    hosts.extend(h.into_vec());
                }
                if hosts.is_empty()
                    && let Some(h) = r.hosts.clone()
                {
                    hosts.extend(h.into_vec());
                }
                let mut upstreams: Vec<String> = vec![];
                if let Some(u) = r.upstreams.clone() {
                    upstreams.extend(u.into_vec());
                }
                if upstreams.is_empty()
                    && let Some(u) = r.upstream.clone()
                {
                    upstreams.extend(u.into_vec());
                }
                if upstreams.is_empty()
                    && let Some(u) = r.backends.clone()
                {
                    upstreams.extend(u.into_vec());
                }
                if upstreams.is_empty()
                    && let Some(u) = r.backend.clone()
                {
                    upstreams.extend(u.into_vec());
                }

                let hosts: Vec<String> = hosts
                    .into_iter()
                    .map(|s| s.trim().to_ascii_lowercase())
                    .filter(|s| !s.is_empty())
                    .collect();
                if hosts.is_empty() {
                    anyhow::bail!("config: routes[{}] missing host", i);
                }

                let upstreams: Vec<String> = upstreams
                    .into_iter()
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect();
                if upstreams.is_empty() {
                    anyhow::bail!("config: routes[{}] missing upstreams", i);
                }

                let strategy = r
                    .strategy
                    .clone()
                    .unwrap_or_else(|| "sequential".into())
                    .trim()
                    .to_ascii_lowercase();

                // Middleware chain (required for hostname-routing routes).
                // Prefer `middlewares`, but accept legacy `parsers` as an alias.
                let mut middlewares: Vec<String> = r
                    .middlewares
                    .clone()
                    .or_else(|| r.parsers.clone())
                    .map(|m| m.into_vec())
                    .unwrap_or_default();

                middlewares = middlewares
                    .into_iter()
                    .map(|s| normalize_middleware_ref(&s))
                    .collect::<anyhow::Result<Vec<_>>>()
                    .with_context(|| format!("config: routes[{}] invalid middlewares", i))?;

                if middlewares.is_empty() {
                    anyhow::bail!(
                        "config: routes[{}] missing middlewares (set routes[].middlewares)",
                        i
                    );
                }

                cfg.routes.push(RouteConfig {
                    host: hosts,
                    upstreams,
                    middlewares,
                    strategy,
                });
            }
        }

        // --- Logging ---
        if let Some(l) = &fc.logging {
            if let Some(level) = &l.level
                && !level.trim().is_empty()
            {
                cfg.logging.level = level.trim().to_string();
            }
            if let Some(fmt) = &l.format
                && !fmt.trim().is_empty()
            {
                cfg.logging.format = fmt.trim().to_string();
            }
            if let Some(out) = &l.output
                && !out.trim().is_empty()
            {
                cfg.logging.output = out.trim().to_string();
            }
            cfg.logging.add_source = l.add_source;
        }

        // --- Tunnel ---
        if let Some(t) = &fc.tunnel {
            cfg.tunnel.auth_token = t.auth_token.clone().unwrap_or_default().trim().to_string();
            cfg.tunnel.auto_listen_services = t.auto_listen_services.unwrap_or(true);

            if let Some(eps) = &t.endpoints {
                for ep in eps {
                    cfg.tunnel.endpoints.push(TunnelEndpointConfig {
                        listen_addr: ep.listen_addr.trim().to_string(),
                        transport: ep
                            .transport
                            .clone()
                            .unwrap_or_else(|| "tcp".into())
                            .trim()
                            .to_ascii_lowercase(),
                        quic: QuicServerConfig {
                            cert_file: ep
                                .quic
                                .as_ref()
                                .and_then(|q| q.cert_file.clone())
                                .unwrap_or_default()
                                .trim()
                                .to_string(),
                            key_file: ep
                                .quic
                                .as_ref()
                                .and_then(|q| q.key_file.clone())
                                .unwrap_or_default()
                                .trim()
                                .to_string(),
                            use_acme: ep.quic.as_ref().and_then(|q| q.use_acme),
                        },
                        websocket: WebSocketServerConfig {
                            cert_file: ep
                                .websocket
                                .as_ref()
                                .and_then(|w| w.cert_file.clone())
                                .unwrap_or_default()
                                .trim()
                                .to_string(),
                            key_file: ep
                                .websocket
                                .as_ref()
                                .and_then(|w| w.key_file.clone())
                                .unwrap_or_default()
                                .trim()
                                .to_string(),
                            use_acme: ep.websocket.as_ref().and_then(|w| w.use_acme),
                        },
                    });
                }
            }

            if let Some(conn) = &t.connector {
                let mut auth_token = conn
                    .auth_token
                    .clone()
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if auth_token.is_empty() {
                    auth_token = cfg.tunnel.auth_token.clone();
                }
                cfg.tunnel.connector = Some(TunnelConnectorConfig {
                    server_addr: conn.server_addr.trim().to_string(),
                    transport: conn
                        .transport
                        .clone()
                        .unwrap_or_else(|| "auto".into())
                        .trim()
                        .to_ascii_lowercase(),
                    auth_token,
                    dial_timeout_ms: conn.dial_timeout_ms,
                    dial_timeout: Duration::from_millis(
                        conn.dial_timeout_ms.unwrap_or(5000).max(0) as u64,
                    ),
                    quic: conn.quic.as_ref().map(|q| QuicClientConfig {
                        server_name: q.server_name.clone().unwrap_or_default().trim().to_string(),
                        insecure_skip_verify: q.insecure_skip_verify,
                    }),
                    websocket: conn.websocket.as_ref().map(|w| WebSocketClientConfig {
                        server_name: w.server_name.clone().unwrap_or_default().trim().to_string(),
                        insecure_skip_verify: w.insecure_skip_verify,
                    }),
                    doh_servers: conn
                        .doh_servers
                        .clone()
                        .map(|s| s.into_vec())
                        .unwrap_or_default(),
                });
            }

            if let Some(c) = &t.client {
                let mut auth_token = c.auth_token.clone().unwrap_or_default().trim().to_string();
                if auth_token.is_empty() {
                    auth_token = cfg.tunnel.auth_token.clone();
                }
                let middleware = match c.middleware.as_deref() {
                    Some(m) if !m.trim().is_empty() => Some(
                        normalize_middleware_ref(m)
                            .with_context(|| "config: tunnel.client invalid middleware")?,
                    ),
                    _ => None,
                };
                let optimizer = c.optimizer.as_ref().map(|to| OptimizerClientConfig {
                    enabled: to.enabled,
                    zstd_window_log: Some(to.zstd_window_log.unwrap_or(23)),
                    zstd_window_log_uplink: to.zstd_window_log_uplink,
                    zstd_window_log_downlink: to.zstd_window_log_downlink,
                    zstd_dictionary: to.zstd_dictionary.clone(),
                });

                let (fake_lan_broadcast, motd_prefix) = if let Some(ref d) = c.discovery {
                    if let Some(ref mc) = d.minecraft_lan {
                        (
                            mc.enabled,
                            mc.motd_prefix.clone().unwrap_or_else(|| "[Prism] ".into()),
                        )
                    } else {
                        (
                            c.fake_lan_broadcast,
                            c.motd_prefix.clone().unwrap_or_else(|| "[Prism] ".into()),
                        )
                    }
                } else {
                    (
                        c.fake_lan_broadcast,
                        c.motd_prefix.clone().unwrap_or_else(|| "[Prism] ".into()),
                    )
                };

                cfg.tunnel.client = Some(TunnelClientConfig {
                    server_addr: c.server_addr.trim().to_string(),
                    transport: c
                        .transport
                        .clone()
                        .unwrap_or_else(|| "auto".into())
                        .trim()
                        .to_ascii_lowercase(),
                    auth_token,
                    listen_addr: c.listen_addr.clone().unwrap_or_default().trim().to_string(),
                    middleware,
                    fake_lan_broadcast,
                    motd_prefix,
                    optimizer,
                    websocket: c.websocket.as_ref().map(|w| WebSocketClientConfig {
                        server_name: w.server_name.clone().unwrap_or_default().trim().to_string(),
                        insecure_skip_verify: w.insecure_skip_verify,
                    }),
                    doh_servers: c
                        .doh_servers
                        .clone()
                        .map(|s| s.into_vec())
                        .unwrap_or_default(),
                });
            }

            if let Some(svcs) = &t.services {
                for s in svcs {
                    let middleware = match s.middleware.as_deref() {
                        Some(m) if !m.trim().is_empty() => {
                            Some(normalize_middleware_ref(m).with_context(|| {
                                format!("config: tunnel.services[{}] invalid middleware", s.name)
                            })?)
                        }
                        _ => None,
                    };
                    let optimizer = s.optimizer.as_ref().map(|to| OptimizerConfig {
                        enabled: to.enabled,
                        flush_interval_ms: Some(to.flush_interval_ms.unwrap_or(20)),
                        flush_interval_uplink_ms: to.flush_interval_uplink_ms,
                        flush_interval_min_ms: to.flush_interval_min_ms,
                        flush_interval_max_ms: to.flush_interval_max_ms,
                        adaptive_flush: to.adaptive_flush,
                        buffer_threshold: to.buffer_threshold,
                        buffer_threshold_uplink: to.buffer_threshold_uplink,
                        zstd_window_log: Some(to.zstd_window_log.unwrap_or(23)),
                        zstd_window_log_uplink: to.zstd_window_log_uplink,
                        zstd_window_log_downlink: to.zstd_window_log_downlink,
                        zstd_level: Some(to.zstd_level.unwrap_or(3)),
                        zstd_dictionary: to.zstd_dictionary.clone(),
                    });

                    cfg.tunnel.services.push(TunnelServiceConfig {
                        name: s.name.trim().to_string(),
                        proto: s
                            .proto
                            .clone()
                            .unwrap_or_else(|| "tcp".into())
                            .trim()
                            .to_ascii_lowercase(),
                        local_addr: s.local_addr.trim().to_string(),
                        route_only: s.route_only,
                        remote_addr: s.remote_addr.clone().unwrap_or_default().trim().to_string(),
                        masquerade_host: s
                            .masquerade_host
                            .clone()
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                        middleware,
                        optimizer,
                    });
                }
            }

            if let Some(m) = &t.mdns {
                cfg.tunnel.mdns.enabled = m.enabled;
                cfg.tunnel.mdns.domain = m
                    .domain
                    .clone()
                    .unwrap_or_else(|| "local".into())
                    .trim()
                    .to_ascii_lowercase();
                if cfg.tunnel.mdns.domain.is_empty() {
                    cfg.tunnel.mdns.domain = "local".into();
                }
                cfg.tunnel.mdns.subdomain = m
                    .subdomain
                    .clone()
                    .unwrap_or_default()
                    .trim()
                    .to_ascii_lowercase();
                cfg.tunnel.mdns.listen_addr =
                    m.listen_addr.clone().unwrap_or_default().trim().to_string();
                cfg.tunnel.mdns.middlewares = m
                    .middlewares
                    .clone()
                    .map(|v| v.into_vec())
                    .unwrap_or_default()
                    .into_iter()
                    .filter_map(|s| normalize_middleware_ref(&s).ok())
                    .collect();

                let (minecraft_lan, motd_prefix) = if let Some(ref d) = m.discovery {
                    if let Some(ref mc) = d.minecraft_lan {
                        (
                            mc.enabled,
                            mc.motd_prefix.clone().unwrap_or_else(default_motd_prefix),
                        )
                    } else {
                        (
                            m.minecraft_lan || m.fake_lan_broadcast,
                            m.motd_prefix.clone().unwrap_or_else(default_motd_prefix),
                        )
                    }
                } else {
                    (
                        m.minecraft_lan || m.fake_lan_broadcast,
                        m.motd_prefix.clone().unwrap_or_else(default_motd_prefix),
                    )
                };
                cfg.tunnel.mdns.minecraft_lan = minecraft_lan;
                cfg.tunnel.mdns.motd_prefix = motd_prefix;
            }
        } else {
            // Default: match Go defaults.
            cfg.tunnel.auto_listen_services = true;
        }

        let mut auth_cfg = crate::prism::auth::AuthConfig::default();
        if let Some(fa) = fc.auth.take() {
            if let Some(mode) = fa.mode {
                auth_cfg.mode = mode.trim().to_string();
            }
            auth_cfg.legacy_token = fa
                .legacy_token
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            if let Some(fg) = fa.github {
                auth_cfg.github = Some(crate::prism::auth::GitHubOAuthConfig {
                    enabled: fg.enabled,
                    client_id: fg.client_id.unwrap_or_default().trim().to_string(),
                    client_secret: fg.client_secret.unwrap_or_default().trim().to_string(),
                    redirect_uri: fg
                        .redirect_uri
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    admin_users: fg.admin_users.map(|s| s.into_vec()).unwrap_or_default(),
                    admin_orgs: fg.admin_orgs.map(|s| s.into_vec()).unwrap_or_default(),
                    allowed_users: fg.allowed_users.map(|s| s.into_vec()).unwrap_or_default(),
                    allowed_orgs: fg.allowed_orgs.map(|s| s.into_vec()).unwrap_or_default(),
                    default_role: fg
                        .default_role
                        .unwrap_or_else(|| "member".to_string())
                        .trim()
                        .to_string(),
                });
            }
        }
        cfg.auth = auth_cfg;

        // --- ACME ---
        let raw_acme = fc.acme.take().or_else(|| fc.tunnel.as_mut().and_then(|t| t.acme.take()));
        if let Some(fa) = raw_acme {
            let mut domains = fa.domains.map(|s| s.into_vec()).unwrap_or_default();
            domains.retain(|d| !d.trim().is_empty());

            let email = fa.email
                .or_else(|| std::env::var("ACME_EMAIL").ok())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());

            let mut cf_cfg = CloudflareAcmeConfig::default();
            if let Some(cf) = fa.cloudflare {
                cf_cfg.api_token = cf.api_token
                    .or_else(|| std::env::var("CLOUDFLARE_API_TOKEN").ok())
                    .or_else(|| std::env::var("CF_API_TOKEN").ok())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                cf_cfg.zone_id = cf.zone_id
                    .or_else(|| std::env::var("CLOUDFLARE_ZONE_ID").ok())
                    .or_else(|| std::env::var("CF_ZONE_ID").ok())
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                if let Some(to) = cf.propagation_timeout_secs {
                    cf_cfg.propagation_timeout_secs = to.max(5);
                }
            } else {
                cf_cfg.api_token = std::env::var("CLOUDFLARE_API_TOKEN")
                    .or_else(|_| std::env::var("CF_API_TOKEN"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                cf_cfg.zone_id = std::env::var("CLOUDFLARE_ZONE_ID")
                    .or_else(|_| std::env::var("CF_ZONE_ID"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
            }

            cfg.acme = Some(AcmeConfig {
                enabled: fa.enabled,
                domains,
                email,
                directory_url: fa.directory_url.unwrap_or_else(|| "production".into()).trim().to_string(),
                storage_dir: fa.storage_dir.unwrap_or_default().trim().to_string(),
                cert_file: fa.cert_file.unwrap_or_default().trim().to_string(),
                key_file: fa.key_file.unwrap_or_default().trim().to_string(),
                renew_before_days: fa.renew_before_days.unwrap_or(30),
                auto_renew: fa.auto_renew.unwrap_or(true),
                cloudflare: cf_cfg,
            });
        }

        Ok(cfg)
    }
}

fn default_motd_prefix() -> String {
    "[Prism] ".to_string()
}

pub fn normalize_middleware_ref(s: &str) -> anyhow::Result<String> {
    // Configs refer to middleware modules by name only (no paths/extensions).
    // Normalization:
    // - trim
    // - lowercase
    // - treat '-' as '_'
    // Validation:
    // - no path separators
    // - no '.' and no extension
    let mut out = s.trim().to_ascii_lowercase();
    out = out.replace('-', "_");

    if out.is_empty() {
        anyhow::bail!("empty middleware name");
    }
    if out.contains('/') || out.contains('\\') {
        anyhow::bail!("middleware name must not contain path separators");
    }
    if out.contains('.') {
        anyhow::bail!("middleware name must not contain '.' or file extensions");
    }
    Ok(out)
}

pub fn restart_required_reasons(current: &Config, next: &Config) -> Vec<String> {
    let mut reasons = Vec::new();

    if current.listeners != next.listeners {
        reasons.push("listener topology changed".to_string());
    }
    if current.admin_addr.trim() != next.admin_addr.trim() {
        reasons.push("admin_addr changed".to_string());
    }
    if current.tunnel.auth_token != next.tunnel.auth_token {
        reasons.push("tunnel auth_token changed".to_string());
    }
    if current.tunnel.auto_listen_services != next.tunnel.auto_listen_services {
        reasons.push("tunnel auto_listen_services changed".to_string());
    }
    if current.tunnel.endpoints != next.tunnel.endpoints {
        reasons.push("tunnel endpoints changed".to_string());
    }
    if current.tunnel.connector != next.tunnel.connector {
        reasons.push("tunnel connector changed".to_string());
    }
    if current.tunnel.client != next.tunnel.client {
        reasons.push("tunnel client changed".to_string());
    }
    if current.tunnel.services != next.tunnel.services {
        reasons.push("tunnel services changed".to_string());
    }
    if current.tunnel.mdns != next.tunnel.mdns {
        reasons.push("tunnel mdns changed".to_string());
    }

    reasons
}

const DEFAULT_CONFIG_TEMPLATE_TOML: &str = r#"# $schema=https://raw.githubusercontent.com/Summpot/prism/master/prism.schema.json
# Prism configuration (auto-generated)
#
# This file was created because Prism could not find a configuration file at the
# resolved config path.
#
# This default config is meant to be runnable without edits and is focused on
# tunnel mode (frp-like): Prism starts a tunnel server and waits for clients to
# connect and register services.
#
# To expose a service to the public internet, configure the tunnel connector with a
# service remote_addr (for example ":25565"); Prism will auto-listen on that port
# on the server side.

admin_addr = "127.0.0.1:8080"

[tunnel]
auth_token = ""
auto_listen_services = true

[[tunnel.endpoints]]
listen_addr = ":7000"
transport = "tcp" # tcp | udp | quic

[logging]
level = "info"
format = "json"
output = "stderr"
add_source = false

[reload]
enabled = true
poll_interval_ms = 1000

[timeouts]
handshake_timeout_ms = 3000
idle_timeout_ms = 0

"#;

const DEFAULT_CONFIG_TEMPLATE_YAML: &str = r#"# yaml-language-server: $schema=https://raw.githubusercontent.com/Summpot/prism/master/prism.schema.json
# Prism configuration (auto-generated)
#
# This file was created because Prism could not find a configuration file at the
# resolved config path.
#
# This default config is meant to be runnable without edits and is focused on
# tunnel mode (frp-like): Prism starts a tunnel server and waits for clients to
# connect and register services.
#
# To expose a service to the public internet, configure the tunnel connector with a
# service remote_addr (for example ":25565"); Prism will auto-listen on that port
# on the server side.

admin_addr: "127.0.0.1:8080"

tunnel:
  auth_token: ""
  auto_listen_services: true
  endpoints:
    - listen_addr: ":7000"
      transport: "tcp" # tcp | udp | quic

logging:
  level: "info"
  format: "json"
  output: "stderr"
  add_source: false

reload:
  enabled: true
  poll_interval_ms: 1000

timeouts:
  handshake_timeout_ms: 3000
  idle_timeout_ms: 0

"#;

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        p.push(format!(
            "prism_cfg_test_{name}_{}_{}",
            std::process::id(),
            now
        ));
        std::fs::create_dir_all(&p).expect("mkdir");
        p
    }

    #[test]
    fn route_middlewares_required() {
        let dir = temp_dir("mw_required");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
[[routes]]
host = "example.com"
upstreams = ["127.0.0.1:1234"]
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let err = load_config(&cfg_path).unwrap_err();
        let s = err.to_string().to_ascii_lowercase();
        assert!(s.contains("missing middlewares"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn route_middlewares_normalize_and_reject_extensions() {
        let dir = temp_dir("reject");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
[[routes]]
host = "example.com"
upstreams = ["127.0.0.1:1234"]
middlewares = ["Foo-Bar", "baz_qux"]
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert_eq!(
            cfg.routes[0].middlewares,
            vec!["foo_bar".to_string(), "baz_qux".to_string()]
        );

        let toml_bad = r#"
[[routes]]
host = "example.com"
upstreams = ["127.0.0.1:1234"]
middlewares = ["bad.wat"]
"#;

        std::fs::write(&cfg_path, toml_bad).expect("write");
        let err = load_config(&cfg_path).unwrap_err();
        let s = err.to_string();
        assert!(s.contains("invalid middlewares") || s.to_ascii_lowercase().contains("middleware"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_parsers_alias_still_works() {
        let dir = temp_dir("legacy_parsers_alias");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
[[routes]]
host = "example.com"
upstreams = ["127.0.0.1:1234"]
parsers = ["Foo-Bar"]
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert_eq!(cfg.routes[0].middlewares, vec!["foo_bar".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_configs_default_to_standalone_role() {
        let dir = temp_dir("standalone_default_role");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[[listeners]]
listen_addr = ":25565"
protocol = "tcp"

[[routes]]
host = "example.com"
upstreams = ["127.0.0.1:25565"]
middlewares = ["minecraft"]
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert_eq!(cfg.listeners.len(), 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    fn parse_test_config(s: &str) -> Config {
        let mut fc: FileConfig = toml::from_str(s).expect("parse toml");
        Config::from_file_config(&mut fc, Path::new("test.toml")).expect("convert to runtime config")
    }

    #[test]
    fn legacy_metrics_section_is_ignored() {
        let dir = temp_dir("metrics_ignored");
        let cfg_path = dir.join("prism.toml");

        std::fs::write(
            &cfg_path,
            r#"
admin_addr = ":8080"

[metrics]
enabled = true
store_path = "local.sqlite"
"#,
        )
        .expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert_eq!(cfg.admin_addr, ":8080");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_required_reasons_detect_listener_changes() {
        let current = parse_test_config(
            r#"
[[listeners]]
listen_addr = ":25565"

[[routes]]
hosts = ["play.example.com"]
upstreams = ["127.0.0.1:25566"]
middlewares = ["minecraft"]
"#,
        );

        let next = parse_test_config(
            r#"
[[listeners]]
listen_addr = ":25566"

[[routes]]
hosts = ["play.example.com"]
upstreams = ["127.0.0.1:25566"]
middlewares = ["minecraft"]
"#,
        );

        let reasons = restart_required_reasons(&current, &next);
        assert!(reasons.iter().any(|reason| reason.contains("listener")));
    }

    #[test]
    fn reject_legacy_routing_parser_dir_field() {
        let dir = temp_dir("legacy");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
routing_parser_dir = "./parsers"

[[routes]]
host = "example.com"
upstreams = ["127.0.0.1:1234"]
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let err = load_config(&cfg_path).unwrap_err();
        let msg = format!("{err:#}").to_ascii_lowercase();
        assert!(
            msg.contains("routing_parser_dir"),
            "expected error mentioning routing_parser_dir, got: {msg}"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tunnel_mdns_config_toml_parsing() {
        let dir = temp_dir("mdns_config");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel]
auth_token = "secret"

[tunnel.connector]
server_addr = "127.0.0.1:7000"

[[tunnel.services]]
name = "home-mc"
local_addr = "127.0.0.1:25565"

[tunnel.mdns]
enabled = true
domain = "local"
subdomain = "prism"
listen_addr = ":25565"
middlewares = ["minecraft"]
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert!(cfg.tunnel.mdns.enabled);
        assert_eq!(cfg.tunnel.mdns.domain, "local");
        assert_eq!(cfg.tunnel.mdns.subdomain, "prism");
        assert_eq!(cfg.tunnel.mdns.listen_addr, ":25565");
        assert_eq!(cfg.tunnel.mdns.middlewares, vec!["minecraft".to_string()]);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tunnel_connector_and_service_optimizer_config() {
        let dir = temp_dir("connector_config");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.connector]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "cluster-secret-token"
dial_timeout_ms = 3500

[tunnel.connector.quic]
server_name = "relay.example.com"
insecure_skip_verify = true

[[tunnel.services]]
name = "minecraft-survival"
proto = "tcp"
local_addr = "127.0.0.1:25565"
middleware = "minecraft"

[tunnel.services.optimizer]
enabled = true
flush_interval_ms = 20
zstd_window_log = 23
zstd_level = 3
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");

        let conn = cfg.tunnel.connector.as_ref().expect("connector configured");
        assert_eq!(conn.server_addr, "relay.example.com:7000");
        assert_eq!(conn.transport, "quic");
        assert_eq!(conn.auth_token, "cluster-secret-token");
        assert_eq!(conn.dial_timeout_ms, Some(3500));
        assert_eq!(conn.dial_timeout, Duration::from_millis(3500));
        let quic = conn.quic.as_ref().expect("quic client configured");
        assert_eq!(quic.server_name, "relay.example.com");
        assert!(quic.insecure_skip_verify);

        assert_eq!(cfg.tunnel.services.len(), 1);
        let svc = &cfg.tunnel.services[0];
        assert_eq!(svc.name, "minecraft-survival");
        assert_eq!(svc.proto, "tcp");
        assert_eq!(svc.local_addr, "127.0.0.1:25565");
        assert_eq!(svc.middleware, Some("minecraft".to_string()));

        let opt = svc.optimizer.as_ref().expect("optimizer configured");
        assert!(opt.enabled);
        assert_eq!(opt.flush_interval_ms, Some(20));
        assert_eq!(opt.zstd_window_log, Some(23));
        assert_eq!(opt.zstd_level, Some(3));
        assert_eq!(opt.flush_interval_ms(), 20);
        assert_eq!(opt.zstd_window_log(), 23);
        assert_eq!(opt.zstd_level(), 3);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tunnel_client_sidecar_config() {
        let dir = temp_dir("client_sidecar_config");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.client]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "cluster-secret-token"
listen_addr = "127.0.0.1:25565"
middleware = "minecraft"
fake_lan_broadcast = true
motd_prefix = "[Prism] "

[tunnel.client.optimizer]
enabled = true
zstd_window_log = 23
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");

        let client = cfg.tunnel.client.as_ref().expect("client configured");
        assert_eq!(client.server_addr, "relay.example.com:7000");
        assert_eq!(client.transport, "quic");
        assert_eq!(client.auth_token, "cluster-secret-token");
        assert_eq!(client.listen_addr, "127.0.0.1:25565");
        assert_eq!(client.middleware, Some("minecraft".to_string()));
        assert!(client.fake_lan_broadcast);
        assert_eq!(client.motd_prefix, "[Prism] ");

        let opt = client
            .optimizer
            .as_ref()
            .expect("client optimizer configured");
        assert!(opt.enabled);
        assert_eq!(opt.zstd_window_log, Some(23));
        assert_eq!(opt.zstd_window_log(), 23);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tunnel_optimizer_defaults() {
        let dir = temp_dir("optimizer_defaults");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.client]
server_addr = "relay.example.com:7000"

[tunnel.client.optimizer]
enabled = true

[[tunnel.services]]
name = "minecraft-survival"
local_addr = "127.0.0.1:25565"

[tunnel.services.optimizer]
enabled = true
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");

        let client = cfg.tunnel.client.as_ref().expect("client configured");
        assert_eq!(client.transport, "auto");
        assert!(!client.fake_lan_broadcast);
        assert_eq!(client.motd_prefix, "[Prism] ");
        let c_opt = client.optimizer.as_ref().expect("client optimizer");
        assert!(c_opt.enabled);
        assert_eq!(c_opt.zstd_window_log, Some(23));

        let svc = &cfg.tunnel.services[0];
        let s_opt = svc.optimizer.as_ref().expect("service optimizer");
        assert!(s_opt.enabled);
        assert_eq!(s_opt.flush_interval_ms, Some(20));
        assert_eq!(s_opt.zstd_window_log, Some(23));
        assert_eq!(s_opt.zstd_level, Some(3));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tunnel_optimizer_custom_config() {
        let dir = temp_dir("optimizer_custom_config");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.client]
server_addr = "relay.example.com:7000"

[tunnel.client.optimizer]
enabled = true
zstd_window_log = 22

[[tunnel.services]]
name = "minecraft-survival"
local_addr = "127.0.0.1:25565"

[tunnel.services.optimizer]
enabled = true
flush_interval_ms = 15
zstd_window_log = 22
zstd_level = 4
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");

        let client = cfg.tunnel.client.as_ref().expect("client");
        let c_opt = client.optimizer.as_ref().expect("client optimizer");
        assert!(c_opt.enabled);
        assert_eq!(c_opt.zstd_window_log, Some(22));

        let svc = &cfg.tunnel.services[0];
        let s_opt = svc.optimizer.as_ref().expect("service optimizer");
        assert!(s_opt.enabled);
        assert_eq!(s_opt.flush_interval_ms, Some(15));
        assert_eq!(s_opt.zstd_window_log, Some(22));
        assert_eq!(s_opt.zstd_level, Some(4));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_required_reasons_detect_connector_and_client_changes() {
        let current = parse_test_config(
            r#"
[tunnel.connector]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "token1"

[tunnel.client]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "token1"
listen_addr = "127.0.0.1:25565"
middleware = "minecraft"
"#,
        );

        let next_connector_changed = parse_test_config(
            r#"
[tunnel.connector]
server_addr = "relay2.example.com:7000"
transport = "quic"
auth_token = "token1"

[tunnel.client]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "token1"
listen_addr = "127.0.0.1:25565"
middleware = "minecraft"
"#,
        );

        let next_client_changed = parse_test_config(
            r#"
[tunnel.connector]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "token1"

[tunnel.client]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "token1"
listen_addr = "127.0.0.1:25566"
middleware = "minecraft"
fake_lan_broadcast = true
"#,
        );

        let reasons_conn = restart_required_reasons(&current, &next_connector_changed);
        assert!(reasons_conn.iter().any(|r| r.contains("connector")));

        let reasons_client = restart_required_reasons(&current, &next_client_changed);
        assert!(reasons_client.iter().any(|r| r.contains("client")));
    }

    #[test]
    fn tunnel_mdns_defaults() {
        let dir = temp_dir("mdns_defaults");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.mdns]
enabled = true
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert!(cfg.tunnel.mdns.enabled);
        assert_eq!(cfg.tunnel.mdns.domain, "local");
        assert_eq!(cfg.tunnel.mdns.subdomain, "");
        assert_eq!(cfg.tunnel.mdns.listen_addr, "");
        assert!(cfg.tunnel.mdns.middlewares.is_empty());
        assert!(!cfg.tunnel.mdns.minecraft_lan);
        assert_eq!(cfg.tunnel.mdns.motd_prefix, "[Prism] ");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn tunnel_mdns_minecraft_lan() {
        let dir = temp_dir("mdns_minecraft_lan");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.mdns]
enabled = true
minecraft_lan = true
motd_prefix = "[Custom] "
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert!(cfg.tunnel.mdns.enabled);
        assert!(cfg.tunnel.mdns.minecraft_lan);
        assert_eq!(cfg.tunnel.mdns.motd_prefix, "[Custom] ");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn restart_required_reasons_detect_mdns_changes() {
        let current = parse_test_config(
            r#"
[tunnel.mdns]
enabled = false
domain = "local"
subdomain = "prism"
listen_addr = ":25565"
middlewares = ["minecraft"]
"#,
        );

        let next = parse_test_config(
            r#"
[tunnel.mdns]
enabled = true
domain = "local"
subdomain = "prism"
listen_addr = ":25565"
middlewares = ["minecraft"]
"#,
        );

        let reasons = restart_required_reasons(&current, &next);
        assert!(reasons.iter().any(|reason| reason.contains("mdns")));
    }

    #[test]
    fn test_tunnel_client_discovery_minecraft_lan() {
        let dir = temp_dir("client_discovery_lan");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
admin_addr = ":8080"

[tunnel.client]
server_addr = "relay.example.com:7000"
transport = "quic"
auth_token = "secret"
listen_addr = "127.0.0.1:25565"
middleware = "minecraft"

[tunnel.client.optimizer]
enabled = true
zstd_window_log = 23

[tunnel.client.discovery.minecraft_lan]
enabled = true
motd_prefix = "[Custom] "
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        let client = cfg.tunnel.client.expect("client config");
        assert_eq!(client.server_addr, "relay.example.com:7000");
        assert_eq!(client.transport, "quic");
        assert_eq!(client.listen_addr, "127.0.0.1:25565");
        assert_eq!(client.middleware, Some("minecraft".into()));
        assert!(client.fake_lan_broadcast);
        assert_eq!(client.motd_prefix, "[Custom] ");

        let opt = client.optimizer.expect("traffic optimizer");
        assert!(opt.enabled);
        assert_eq!(opt.zstd_window_log(), 23);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_auth_config_parsing() {
        let dir =
            std::env::temp_dir().join(format!("prism-test-auth-cfg-{}", rand::random::<u32>()));
        std::fs::create_dir_all(&dir).expect("create_dir_all");
        let cfg_path = dir.join("prism.toml");

        let toml = r#"
[auth]
mode = "hybrid"
legacy_token = "old-secret"

[auth.github]
enabled = true
client_id = "gh_id_123"
client_secret = "gh_sec_456"
redirect_uri = "https://example.com/callback"
admin_users = ["Summpot", "alice"]
admin_orgs = "MyOrg"
allowed_users = ["bob"]
default_role = "member"
"#;

        std::fs::write(&cfg_path, toml).expect("write");
        let cfg = load_config(&cfg_path).expect("load_config");
        assert_eq!(cfg.auth.mode, "hybrid");
        assert_eq!(cfg.auth.legacy_token.as_deref(), Some("old-secret"));

        let gh = cfg.auth.github.expect("github config");
        assert!(gh.enabled);
        assert_eq!(gh.client_id, "gh_id_123");
        assert_eq!(gh.client_secret, "gh_sec_456");
        assert_eq!(
            gh.redirect_uri.as_deref(),
            Some("https://example.com/callback")
        );
        assert_eq!(gh.admin_users, vec!["Summpot", "alice"]);
        assert_eq!(gh.admin_orgs, vec!["MyOrg"]);
        assert_eq!(gh.allowed_users, vec!["bob"]);
        assert_eq!(gh.default_role, "member");

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_acme_config_parsing() {
        let toml_content = r#"
[acme]
enabled = true
domains = ["tunnel.example.com", "*.tunnel.example.com"]
email = "admin@example.com"
directory_url = "staging"
renew_before_days = 25
auto_renew = true

[acme.cloudflare]
api_token = "cf-token-12345"
zone_id = "zone-id-67890"
propagation_timeout_secs = 60

[tunnel]
auth_token = "tunnel-secret"

[[tunnel.endpoints]]
listen_addr = ":7001"
transport = "quic"

[tunnel.endpoints.quic]
use_acme = true

[[tunnel.endpoints]]
listen_addr = ":7002"
transport = "wss"
"#;
        let mut fc: FileConfig = toml::from_str(toml_content).expect("parse toml with acme");
        let cfg = Config::from_file_config(&mut fc, Path::new("test.toml")).expect("convert to runtime config");

        let acme = cfg.acme.expect("acme should be configured");
        assert!(acme.enabled);
        assert_eq!(acme.domains, vec!["tunnel.example.com", "*.tunnel.example.com"]);
        assert_eq!(acme.email.as_deref(), Some("admin@example.com"));
        assert_eq!(acme.directory_url, "staging");
        assert_eq!(acme.renew_before_days, 25);
        assert!(acme.auto_renew);
        assert_eq!(acme.cloudflare.api_token, "cf-token-12345");
        assert_eq!(acme.cloudflare.zone_id, "zone-id-67890");
        assert_eq!(acme.cloudflare.propagation_timeout_secs, 60);

        assert_eq!(cfg.tunnel.endpoints.len(), 2);
        let ep0 = &cfg.tunnel.endpoints[0];
        assert_eq!(ep0.transport, "quic");
        assert_eq!(ep0.quic.use_acme, Some(true));
        assert!(ep0.quic.should_use_acme(true));

        let ep1 = &cfg.tunnel.endpoints[1];
        assert_eq!(ep1.transport, "wss");
        assert_eq!(ep1.websocket.use_acme, None);
        assert!(ep1.websocket.should_use_acme(true));
        assert!(!ep1.websocket.should_use_acme(false));
    }
}

