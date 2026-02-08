use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use directories::ProjectDirs;
use serde::Deserialize;

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

pub fn resolve_config_path(explicit_flag_path: Option<PathBuf>) -> anyhow::Result<ResolvedConfigPath> {
    if let Some(p) = explicit_flag_path {
        let p = normalize_explicit_path(&p)?;
        return Ok(ResolvedConfigPath {
            path: p,
            source: ConfigPathSource::Flag,
        });
    }

    // clap already maps PRISM_CONFIG into the flag value when unset, but keep the design's precedence
    // clear by treating it as "env" when present.
    if let Some(p) = std::env::var_os("PRISM_CONFIG") {
        if !p.is_empty() {
            let p = normalize_explicit_path(Path::new(&p))?;
            return Ok(ResolvedConfigPath {
                path: p,
                source: ConfigPathSource::Env,
            });
        }
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
        if let Ok(m) = fs::metadata(&p) {
            if m.is_file() {
                return Ok(p);
            }
        }
    }
    anyhow::bail!("config: no prism.* found")
}

fn default_config_path() -> anyhow::Result<PathBuf> {
    let proj = ProjectDirs::from("com", "summpot", "prism")
        .context("config: resolve user config dir")?;
    Ok(proj.config_dir().join("prism.toml"))
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
            anyhow::bail!("config: {} exists but is not a regular file", path.display());
        }
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {}
        Err(err) => return Err(err).with_context(|| format!("config: stat {}", path.display())),
    }

    let tmpl = default_config_template_for_path(path)?;

    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .with_context(|| format!("config: mkdir {}", parent.display()))?;
        }
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

    Ok(Config::from_file_config(&mut fc)?)
}

#[derive(Debug, Clone)]
pub struct Config {
    pub listeners: Vec<ProxyListenerConfig>,
    pub admin_addr: String,
    pub logging: LoggingConfig,
    pub opentelemetry: OpenTelemetryConfig,
    pub routes: Vec<RouteConfig>,
    pub routing_parsers: Vec<RoutingParserConfig>,
    pub max_header_bytes: usize,
    pub reload: ReloadConfig,
    pub proxy_protocol_v2: bool,
    pub buffer_size: usize,
    pub upstream_dial_timeout: Duration,
    pub timeouts: Timeouts,
    pub tunnel: TunnelConfig,
}

#[derive(Debug, Clone)]
pub struct Timeouts {
    pub handshake_timeout: Duration,
    pub idle_timeout: Duration,
}

#[derive(Debug, Clone)]
pub struct ProxyListenerConfig {
    pub listen_addr: String,
    pub protocol: String, // tcp | udp
    pub upstream: String,
}

#[derive(Debug, Clone)]
pub struct ReloadConfig {
    pub enabled: bool,
    pub poll_interval: Duration,
}

#[derive(Debug, Clone)]
pub struct LoggingConfig {
    pub level: String,
    pub format: String,
    pub output: String,
    pub add_source: bool,
}

#[derive(Debug, Clone)]
pub struct OpenTelemetryUiConfig {
    pub logs_url: String,
    pub traces_url: String,
    pub metrics_url: String,
}

#[derive(Debug, Clone)]
pub struct OpenTelemetryConfig {
    pub enabled: bool,
    pub service_name: String,
    pub otlp_endpoint: String,
    pub protocol: String, // grpc | http
    pub timeout: Duration,
    pub headers: BTreeMap<String, String>,
    pub ui: OpenTelemetryUiConfig,
}

#[derive(Debug, Clone)]
pub struct RouteConfig {
    pub host: Vec<String>,
    pub upstreams: Vec<String>,
    pub strategy: String,
    pub cache_ping_ttl: Option<Duration>,
}

#[derive(Debug, Clone)]
pub struct RoutingParserConfig {
    pub name: String,
    pub path: String,
    pub function: Option<String>,
    pub max_output_len: Option<u32>,
}

#[derive(Debug, Clone, Default)]
pub struct TunnelConfig {
    pub auth_token: String,
    pub auto_listen_services: bool,
    pub endpoints: Vec<TunnelEndpointConfig>,
    pub client: Option<TunnelClientConfig>,
    pub services: Vec<TunnelServiceConfig>,
}

#[derive(Debug, Clone)]
pub struct TunnelEndpointConfig {
    pub listen_addr: String,
    pub transport: String,
    pub quic: QuicServerConfig,
}

#[derive(Debug, Clone)]
pub struct TunnelClientConfig {
    pub server_addr: String,
    pub transport: String,
    pub dial_timeout: Duration,
    pub quic: QuicClientConfig,
}

#[derive(Debug, Clone, Default)]
pub struct QuicServerConfig {
    pub cert_file: String,
    pub key_file: String,
}

#[derive(Debug, Clone, Default)]
pub struct QuicClientConfig {
    pub server_name: String,
    pub insecure_skip_verify: bool,
}

#[derive(Debug, Clone)]
pub struct TunnelServiceConfig {
    pub name: String,
    pub proto: String,
    pub local_addr: String,
    pub route_only: bool,
    pub remote_addr: String,
}

#[derive(Debug, Deserialize)]
struct FileConfig {
    #[serde(default)]
    listeners: Vec<FileProxyListener>,

    #[serde(default)]
    admin_addr: String,

    logging: Option<FileLogging>,

    opentelemetry: Option<FileOpenTelemetry>,

    #[serde(default)]
    routes: Vec<FileRoute>,

    #[serde(default, rename = "routing_parsers")]
    routing_parsers: Vec<FileRoutingParser>,

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
}

#[derive(Debug, Deserialize)]
struct FileProxyListener {
    listen_addr: String,
    #[serde(default)]
    protocol: String,
    #[serde(default)]
    upstream: String,
}

#[derive(Debug, Deserialize)]
struct FileLogging {
    level: Option<String>,
    format: Option<String>,
    output: Option<String>,
    #[serde(default)]
    add_source: bool,
}

#[derive(Debug, Deserialize)]
struct FileOpenTelemetry {
    #[serde(default)]
    enabled: bool,
    service_name: Option<String>,
    otlp_endpoint: Option<String>,
    protocol: Option<String>,
    timeout_ms: Option<i64>,
    headers: Option<BTreeMap<String, String>>,
    ui: Option<FileOpenTelemetryUi>,
}

#[derive(Debug, Deserialize)]
struct FileOpenTelemetryUi {
    logs_url: Option<String>,
    traces_url: Option<String>,
    metrics_url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileReload {
    #[serde(default)]
    enabled: bool,
    poll_interval_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FileTimeouts {
    handshake_timeout_ms: Option<i64>,
    idle_timeout_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FileRoute {
    host: Option<StringOrVec>,
    hosts: Option<StringOrVec>,
    upstream: Option<StringOrVec>,
    upstreams: Option<StringOrVec>,
    backend: Option<StringOrVec>,
    backends: Option<StringOrVec>,

    strategy: Option<String>,

    cache_ping_ttl: Option<String>,
    cache_ping_ttl_ms: Option<i64>,
}

#[derive(Debug, Deserialize)]
struct FileRoutingParser {
    #[serde(rename = "type")]
    ty: Option<String>,
    name: Option<String>,
    path: Option<String>,
    function: Option<String>,
    max_output_len: Option<u32>,
}

#[derive(Debug, Deserialize)]
struct FileTunnel {
    auth_token: Option<String>,
    auto_listen_services: Option<bool>,
    endpoints: Option<Vec<FileTunnelEndpoint>>,
    client: Option<FileTunnelClient>,
    services: Option<Vec<FileTunnelService>>,
}

#[derive(Debug, Deserialize)]
struct FileTunnelEndpoint {
    listen_addr: String,
    transport: Option<String>,
    quic: Option<FileQuicServer>,
}

#[derive(Debug, Deserialize)]
struct FileTunnelClient {
    server_addr: String,
    transport: Option<String>,
    dial_timeout_ms: Option<i64>,
    quic: Option<FileQuicClient>,
}

#[derive(Debug, Deserialize)]
struct FileQuicServer {
    cert_file: Option<String>,
    key_file: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FileQuicClient {
    server_name: Option<String>,
    #[serde(default)]
    insecure_skip_verify: bool,
}

#[derive(Debug, Deserialize)]
struct FileTunnelService {
    name: String,
    proto: Option<String>,
    local_addr: String,
    #[serde(default)]
    route_only: bool,
    remote_addr: Option<String>,
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
    fn from_file_config(fc: &mut FileConfig) -> anyhow::Result<Config> {
        let mut cfg = Config {
            listeners: vec![],
            admin_addr: fc.admin_addr.trim().to_string(),
            logging: LoggingConfig {
                level: "info".into(),
                format: "json".into(),
                output: "stderr".into(),
                add_source: false,
            },
            opentelemetry: OpenTelemetryConfig {
                enabled: false,
                service_name: "prism".into(),
                otlp_endpoint: "".into(),
                protocol: "grpc".into(),
                timeout: Duration::from_millis(5000),
                headers: BTreeMap::new(),
                ui: OpenTelemetryUiConfig {
                    logs_url: "".into(),
                    traces_url: "".into(),
                    metrics_url: "".into(),
                },
            },
            routes: vec![],
            routing_parsers: vec![],
            max_header_bytes: fc.max_header_bytes as i64 as usize,
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
            upstream_dial_timeout: Duration::from_millis((fc.upstream_dial_timeout_ms).max(0) as u64),
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
                if hosts.is_empty() {
                    if let Some(h) = r.hosts.clone() {
                        hosts.extend(h.into_vec());
                    }
                }
                let mut upstreams: Vec<String> = vec![];
                if let Some(u) = r.upstreams.clone() {
                    upstreams.extend(u.into_vec());
                }
                if upstreams.is_empty() {
                    if let Some(u) = r.upstream.clone() {
                        upstreams.extend(u.into_vec());
                    }
                }
                if upstreams.is_empty() {
                    if let Some(u) = r.backends.clone() {
                        upstreams.extend(u.into_vec());
                    }
                }
                if upstreams.is_empty() {
                    if let Some(u) = r.backend.clone() {
                        upstreams.extend(u.into_vec());
                    }
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

                let cache_ttl = parse_cache_ttl(r.cache_ping_ttl.as_deref(), r.cache_ping_ttl_ms)
                    .with_context(|| format!("config: routes[{}] invalid cache_ping_ttl", i))?;

                cfg.routes.push(RouteConfig {
                    host: hosts,
                    upstreams,
                    strategy,
                    cache_ping_ttl: cache_ttl,
                });
            }
        }

        // --- Logging ---
        if let Some(l) = &fc.logging {
            if let Some(level) = &l.level {
                if !level.trim().is_empty() {
                    cfg.logging.level = level.trim().to_string();
                }
            }
            if let Some(fmt) = &l.format {
                if !fmt.trim().is_empty() {
                    cfg.logging.format = fmt.trim().to_string();
                }
            }
            if let Some(out) = &l.output {
                if !out.trim().is_empty() {
                    cfg.logging.output = out.trim().to_string();
                }
            }
            cfg.logging.add_source = l.add_source;
        }

        // --- OpenTelemetry ---
        if let Some(ot) = &fc.opentelemetry {
            cfg.opentelemetry.enabled = ot.enabled;
            if let Some(sn) = &ot.service_name {
                if !sn.trim().is_empty() {
                    cfg.opentelemetry.service_name = sn.trim().to_string();
                }
            }
            if let Some(ep) = &ot.otlp_endpoint {
                if !ep.trim().is_empty() {
                    cfg.opentelemetry.otlp_endpoint = ep.trim().to_string();
                }
            }
            if let Some(p) = &ot.protocol {
                if !p.trim().is_empty() {
                    cfg.opentelemetry.protocol = p.trim().to_ascii_lowercase();
                }
            }
            if let Some(ms) = ot.timeout_ms {
                if ms > 0 {
                    cfg.opentelemetry.timeout = Duration::from_millis(ms as u64);
                }
            }
            if let Some(h) = &ot.headers {
                cfg.opentelemetry.headers = h.clone();
            }
            if let Some(ui) = &ot.ui {
                if let Some(v) = &ui.logs_url {
                    cfg.opentelemetry.ui.logs_url = v.trim().to_string();
                }
                if let Some(v) = &ui.traces_url {
                    cfg.opentelemetry.ui.traces_url = v.trim().to_string();
                }
                if let Some(v) = &ui.metrics_url {
                    cfg.opentelemetry.ui.metrics_url = v.trim().to_string();
                }
            }
        }

        // --- Routing parsers ---
        if !fc.routing_parsers.is_empty() {
            for rp in &fc.routing_parsers {
                if let Some(t) = &rp.ty {
                    let t = t.trim().to_ascii_lowercase();
                    if !t.is_empty() && t != "wasm" {
                        anyhow::bail!("config: routing_parsers only supports type=wasm in Rust (got {t})");
                    }
                }

                let path = rp.path.clone().unwrap_or_default().trim().to_string();
                if path.is_empty() {
                    anyhow::bail!("config: routing_parsers entry missing path");
                }

                cfg.routing_parsers.push(RoutingParserConfig {
                    name: rp
                        .name
                        .clone()
                        .unwrap_or_default()
                        .trim()
                        .to_string(),
                    path,
                    function: rp
                        .function
                        .clone()
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty()),
                    max_output_len: rp.max_output_len,
                });
            }
        }
        if cfg.routing_parsers.is_empty() {
            cfg.routing_parsers = vec![
                RoutingParserConfig {
                    name: "minecraft_handshake".into(),
                    path: "builtin:minecraft_handshake".into(),
                    function: None,
                    max_output_len: None,
                },
                RoutingParserConfig {
                    name: "tls_sni".into(),
                    path: "builtin:tls_sni".into(),
                    function: None,
                    max_output_len: None,
                },
            ];
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
                        },
                    });
                }
            }

            if let Some(c) = &t.client {
                cfg.tunnel.client = Some(TunnelClientConfig {
                    server_addr: c.server_addr.trim().to_string(),
                    transport: c
                        .transport
                        .clone()
                        .unwrap_or_else(|| "tcp".into())
                        .trim()
                        .to_ascii_lowercase(),
                    dial_timeout: Duration::from_millis(c.dial_timeout_ms.unwrap_or(5000).max(0) as u64),
                    quic: QuicClientConfig {
                        server_name: c
                            .quic
                            .as_ref()
                            .and_then(|q| q.server_name.clone())
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                        insecure_skip_verify: c
                            .quic
                            .as_ref()
                            .map(|q| q.insecure_skip_verify)
                            .unwrap_or(false),
                    },
                });
            }

            if let Some(svcs) = &t.services {
                for s in svcs {
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
                    });
                }
            }
        } else {
            // Default: match Go defaults.
            cfg.tunnel.auto_listen_services = true;
        }

        Ok(cfg)
    }
}

fn parse_cache_ttl(cache_ping_ttl: Option<&str>, cache_ping_ttl_ms: Option<i64>) -> anyhow::Result<Option<Duration>> {
    // Default matches gate lite: enabled by default for a short TTL.
    let mut ttl = Some(Duration::from_secs(10));

    if let Some(s) = cache_ping_ttl {
        let st = s.trim();
        if !st.is_empty() {
            if st == "-1" {
                ttl = None;
            } else {
                let d = humantime::parse_duration(st)?;
                if d.as_nanos() == 0 {
                    ttl = Some(Duration::from_secs(0));
                } else {
                    ttl = Some(d);
                }
            }
        }
    } else if let Some(ms) = cache_ping_ttl_ms {
        if ms < 0 {
            ttl = None;
        } else {
            ttl = Some(Duration::from_millis(ms as u64));
        }
    }

    Ok(ttl)
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
# To expose a service to the public internet, configure the tunnel client with a
# service remote_addr (for example ":25565"); Prism will auto-listen on that port
# on the server side.

admin_addr = ":8080"

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

[opentelemetry]
enabled = false
service_name = "prism"
otlp_endpoint = "" # e.g. http://127.0.0.1:4317 (OTLP/gRPC) or http://127.0.0.1:4318 (OTLP/HTTP)
protocol = "grpc" # grpc | http
timeout_ms = 5000

[opentelemetry.ui]
logs_url = "" # optional: external logs UI link
traces_url = "" # optional: external traces UI link
metrics_url = "" # optional: external metrics UI link

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
# To expose a service to the public internet, configure the tunnel client with a
# service remote_addr (for example ":25565"); Prism will auto-listen on that port
# on the server side.

admin_addr: ":8080"

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

opentelemetry:
    enabled: false
    service_name: "prism"
    otlp_endpoint: "" # e.g. http://127.0.0.1:4317 (OTLP/gRPC) or http://127.0.0.1:4318 (OTLP/HTTP)
    protocol: "grpc" # grpc | http
    timeout_ms: 5000
    ui:
        logs_url: "" # optional: external logs UI link
        traces_url: "" # optional: external traces UI link
        metrics_url: "" # optional: external metrics UI link

reload:
  enabled: true
  poll_interval_ms: 1000

timeouts:
  handshake_timeout_ms: 3000
  idle_timeout_ms: 0

"#;
