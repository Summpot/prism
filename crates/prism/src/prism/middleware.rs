use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, LazyLock, Mutex, RwLock as StdRwLock, Weak},
};

use aes::Aes128;
use aes::cipher::{Array, BlockCipherEncrypt, KeyInit};
use anyhow::Context;
use rsa::pkcs1::DecodeRsaPrivateKey;
use rsa::pkcs8::DecodePrivateKey;
use rsa::{Pkcs1v15Encrypt, RsaPrivateKey};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use wasmtime::{
    Caller, Engine, Instance, Linker, Memory, Module, Store, TypedFunc, WasmParams, WasmResults,
};

#[derive(Debug, Error)]
pub enum MiddlewareError {
    #[error("need more data")]
    NeedMoreData,
    #[error("no match")]
    NoMatch,
    #[error("fatal middleware error: {0}")]
    Fatal(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MiddlewarePhase {
    /// Extract routing host (and optionally normalize/strip custom headers).
    Parse = 0,
    /// Rewrite the captured prelude before proxying upstream.
    Rewrite = 1,
}

#[derive(Debug, Clone)]
pub struct MiddlewareCtx {
    pub phase: MiddlewarePhase,
    /// The selected upstream label (after any default port fill) for rewrite.
    ///
    /// For direct upstreams this is typically a dial address like `host:port`.
    /// For tunnel upstreams (`tunnel:<service>`), Prism skips rewrite unless a configured
    /// masquerade host (see `tunnel.services[].masquerade_host`) provides a real protocol host.
    #[allow(dead_code)]
    pub selected_upstream: Option<String>,
}

impl MiddlewareCtx {
    pub fn parse() -> Self {
        Self {
            phase: MiddlewarePhase::Parse,
            selected_upstream: None,
        }
    }

    pub fn rewrite(selected_upstream: &str) -> Self {
        Self {
            phase: MiddlewarePhase::Rewrite,
            selected_upstream: Some(selected_upstream.trim().to_string()),
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct MiddlewareOutput {
    /// Routing host extracted by this middleware (lowercased by the host runtime).
    pub host: Option<String>,
    /// Replacement bytes for the captured prelude.
    pub rewrite: Option<Vec<u8>>,
}

pub trait Middleware: Send + Sync {
    fn name(&self) -> &str;
    fn apply(
        &self,
        prelude: &[u8],
        ctx: &MiddlewareCtx,
    ) -> Result<MiddlewareOutput, MiddlewareError>;
}

pub type SharedMiddleware = Arc<dyn Middleware>;

pub trait MiddlewareProvider: Send + Sync {
    fn get(&self, name: &str) -> anyhow::Result<SharedMiddleware>;

    fn chain(&self, names: &[String]) -> anyhow::Result<SharedMiddlewareChain> {
        let mut out: Vec<SharedMiddleware> = Vec::with_capacity(names.len());
        for n in names {
            out.push(self.get(n)?);
        }
        Ok(Arc::new(ChainMiddleware::new(out)))
    }
}

pub type SharedMiddlewareChain = Arc<dyn MiddlewareChain>;

pub trait MiddlewareChain: Send + Sync {
    #[allow(dead_code)]
    fn name(&self) -> &str;

    /// Apply middleware chain in parse mode.
    ///
    /// On success returns (host, maybe_rewritten_prelude).
    fn parse(&self, prelude: &[u8]) -> Result<(String, Option<Vec<u8>>), MiddlewareError>;

    /// Apply middleware chain in rewrite mode.
    ///
    /// Returns Some(new_prelude) if any middleware rewrote the buffer.
    fn rewrite(&self, prelude: &[u8], selected_upstream: &str) -> Option<Vec<u8>>;
}

pub struct ChainMiddleware {
    middlewares: Vec<SharedMiddleware>,
}

impl ChainMiddleware {
    pub fn new(middlewares: Vec<SharedMiddleware>) -> Self {
        let middlewares = middlewares
            .into_iter()
            .filter(|m| !m.name().trim().is_empty())
            .collect();
        Self { middlewares }
    }
}

impl MiddlewareChain for ChainMiddleware {
    fn name(&self) -> &str {
        "chain"
    }

    fn parse(&self, prelude: &[u8]) -> Result<(String, Option<Vec<u8>>), MiddlewareError> {
        let ctx = MiddlewareCtx::parse();

        let mut need_more = false;
        let mut current: Vec<u8> = prelude.to_vec();
        let mut rewritten: Option<Vec<u8>> = None;

        for m in &self.middlewares {
            match m.apply(&current, &ctx) {
                Ok(out) => {
                    if let Some(rw) = out.rewrite {
                        current = rw;
                        rewritten = Some(current.clone());
                    }

                    if let Some(host) = out.host {
                        let h = host.trim().to_ascii_lowercase();
                        if h.is_empty() {
                            continue;
                        }
                        return Ok((h, rewritten));
                    }

                    // Output with neither host nor rewrite is treated as "no-op".
                }
                Err(MiddlewareError::NeedMoreData) => need_more = true,
                Err(MiddlewareError::NoMatch) => {}
                Err(MiddlewareError::Fatal(_)) => {
                    // Treat per-middleware failures as non-matches so other middleware can win.
                    // The router will treat total failure as no-match.
                }
            }
        }

        if need_more {
            Err(MiddlewareError::NeedMoreData)
        } else {
            Err(MiddlewareError::NoMatch)
        }
    }

    fn rewrite(&self, prelude: &[u8], selected_upstream: &str) -> Option<Vec<u8>> {
        let ctx = MiddlewareCtx::rewrite(selected_upstream);

        let mut current: Vec<u8> = prelude.to_vec();
        let mut changed = false;

        for m in &self.middlewares {
            match m.apply(&current, &ctx) {
                Ok(out) => {
                    if let Some(rw) = out.rewrite {
                        current = rw;
                        changed = true;
                    }
                }
                Err(_) => {
                    // Fail-safe: ignore rewrite errors and keep going.
                }
            }
        }

        if changed { Some(current) } else { None }
    }
}

pub struct FsWasmMiddlewareProvider {
    dir: PathBuf,
    cache: Mutex<HashMap<String, SharedMiddleware>>,
}

impl FsWasmMiddlewareProvider {
    pub fn new(dir: PathBuf) -> Self {
        Self {
            dir,
            cache: Mutex::new(HashMap::new()),
        }
    }

    fn wat_path_for(&self, name: &str) -> PathBuf {
        self.dir.join(format!("{name}.wat"))
    }
}

impl MiddlewareProvider for FsWasmMiddlewareProvider {
    fn get(&self, name: &str) -> anyhow::Result<SharedMiddleware> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("middleware: empty name");
        }

        if let Ok(guard) = self.cache.lock()
            && let Some(m) = guard.get(name)
        {
            return Ok(m.clone());
        }

        let wat_path = self.wat_path_for(name);
        let mw = Arc::new(WasmMiddleware::from_wat_path(name, &wat_path)?) as SharedMiddleware;

        if let Ok(mut guard) = self.cache.lock() {
            guard.insert(name.to_string(), mw.clone());
        }

        Ok(mw)
    }
}

pub const DEFAULT_MIDDLEWARES: &[(&str, &str)] = &[
    (
        "tls_sni",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../middlewares/tls_sni.wat"
        )),
    ),
    (
        "minecraft",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../middlewares/minecraft.wat"
        )),
    ),
];

pub fn get_default_middleware_wat(name: &str) -> Option<&'static str> {
    let base = name.strip_suffix(".wat").unwrap_or(name).trim();
    for (n, wat) in DEFAULT_MIDDLEWARES {
        if *n == base {
            return Some(wat);
        }
    }
    None
}

/// Ensure the middleware directory exists and contains Prism's default WAT middlewares.
///
/// This is intended to match the historical behavior of materializing built-in parsers:
/// on startup, Prism writes a few reference middlewares into the configured directory
/// **if they do not already exist**.
pub fn materialize_default_middlewares(dir: &Path) -> anyhow::Result<Vec<PathBuf>> {
    if dir.as_os_str().is_empty() {
        anyhow::bail!("middleware: empty middleware_dir");
    }

    std::fs::create_dir_all(dir)
        .with_context(|| format!("middleware: create dir {}", dir.display()))?;

    let mut created = Vec::new();

    for (name, wat) in DEFAULT_MIDDLEWARES {
        let path = dir.join(format!("{name}.wat"));
        if path.exists() {
            continue;
        }

        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(mut f) => {
                f.write_all(wat.as_bytes()).with_context(|| {
                    format!("middleware: write default {} to {}", name, path.display())
                })?;
                created.push(path);
            }
            Err(err) if err.kind() == std::io::ErrorKind::AlreadyExists => {
                // Racy create: another thread/process created it.
                continue;
            }
            Err(err) => {
                return Err(err).with_context(|| format!("middleware: create {}", path.display()));
            }
        }
    }

    Ok(created)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum SessionState {
    Handshake = 0,
    Streaming = 1,
    StreamingEgress = 2,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HandshakeResult {
    NeedMoreData,
    RouteMatch {
        host: Option<String>,
        rewrite: Option<Vec<u8>>,
    },
    NoMatch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FramePriority {
    /// Default time-slice (typically 20ms). Independent compression lane.
    Defer = 1,
    /// Flush immediately. Independent compression lane; does not skip earlier frames.
    Urgent = 2,
    /// Short queue (typically 8ms). Independent compression lane.
    High = 3,
    /// Long queue (typically 40ms). Independent compression lane.
    Bulk = 4,
}

impl FramePriority {
    fn from_stream_action(action: u32) -> Option<(Self, bool)> {
        match action {
            1 => Some((Self::Defer, false)),
            2 => Some((Self::Urgent, false)),
            5 => Some((Self::High, false)),
            7 => Some((Self::Bulk, false)),
            3 => Some((Self::Defer, true)),
            4 => Some((Self::Urgent, true)),
            8 => Some((Self::High, true)),
            9 => Some((Self::Bulk, true)),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamResult {
    NeedMoreData,
    /// Middleware is waiting for peer session data (e.g. a shared secret).
    /// The host must not consume the buffer and should retry after `set_session_data`.
    Blocked,
    Frame {
        len: usize,
        priority: FramePriority,
        payload: Option<Vec<u8>>,
        /// Host should emit this frame uncompressed (no zstd window update).
        incompressible: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PollResult {
    Handshake(HandshakeResult),
    Stream(StreamResult),
}

static MIDDLEWARE_DATA_STORE: LazyLock<Arc<StdRwLock<HashMap<(String, Option<u16>), Vec<u8>>>>> =
    LazyLock::new(|| Arc::new(StdRwLock::new(HashMap::new())));

/// Injects external data (e.g. RSA private key DER) for a named middleware and optional port.
pub fn set_injected_middleware_data(name: &str, port: Option<u16>, data: Vec<u8>) {
    let mut store = MIDDLEWARE_DATA_STORE.write().unwrap();
    store.insert((name.trim().to_ascii_lowercase(), port), data);
}

/// Retrieves previously injected external data for a named middleware and optional port.
pub fn get_injected_middleware_data(name: &str, port: Option<u16>) -> Option<Vec<u8>> {
    let store = MIDDLEWARE_DATA_STORE.read().unwrap();
    let name_lower = name.trim().to_ascii_lowercase();
    store
        .get(&(name_lower.clone(), port))
        .or_else(|| store.get(&(name_lower, None)))
        .cloned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfigFieldType {
    U8,
    U16,
    U32,
    I32,
    I64,
    Bool,
    String,
    ListString,
    Unsupported(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigFieldSchema {
    pub key: String,
    pub field_type: ConfigFieldType,
    pub label: String,
    pub description: String,
    pub default_value: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct MiddlewareConfigSchema {
    pub name: String,
    pub fields: Vec<ConfigFieldSchema>,
}

pub fn extract_component_schema(
    engine: &Engine,
    name: &str,
    component: &wasmtime::component::Component,
) -> MiddlewareConfigSchema {
    let mut fields = Vec::new();
    let comp_type = component.component_type();
    for (export_name, item) in comp_type.exports(engine) {
        if export_name == "config" {
            if let wasmtime::component::types::ComponentItem::Type(
                wasmtime::component::Type::Record(r),
            ) = item.ty
            {
                for f in r.fields() {
                    let key = f.name.to_string();
                    let (field_type, default_val) = match f.ty {
                        wasmtime::component::Type::U8 => {
                            (ConfigFieldType::U8, serde_json::json!(1))
                        }
                        wasmtime::component::Type::U16 => {
                            (ConfigFieldType::U16, serde_json::json!(0))
                        }
                        wasmtime::component::Type::U32 => {
                            if key.contains("threshold") {
                                (ConfigFieldType::U32, serde_json::json!(256))
                            } else {
                                (ConfigFieldType::U32, serde_json::json!(0))
                            }
                        }
                        wasmtime::component::Type::S32 => {
                            (ConfigFieldType::I32, serde_json::json!(0))
                        }
                        wasmtime::component::Type::S64 => {
                            (ConfigFieldType::I64, serde_json::json!(0))
                        }
                        wasmtime::component::Type::Bool => {
                            (ConfigFieldType::Bool, serde_json::json!(false))
                        }
                        wasmtime::component::Type::String => {
                            if key.contains("targets") {
                                (
                                    ConfigFieldType::String,
                                    serde_json::json!(
                                        "224.0.2.60:4445,255.255.255.255:4445,127.0.0.1:4445"
                                    ),
                                )
                            } else if key.contains("template") {
                                (
                                    ConfigFieldType::String,
                                    serde_json::json!("[MOTD]{prefix}{name}[/MOTD][AD]{port}[/AD]"),
                                )
                            } else {
                                (ConfigFieldType::String, serde_json::json!(""))
                            }
                        }
                        wasmtime::component::Type::List(_) => {
                            (ConfigFieldType::ListString, serde_json::json!([]))
                        }
                        other => (
                            ConfigFieldType::Unsupported(format!("{other:?}")),
                            serde_json::Value::Null,
                        ),
                    };

                    let label = key
                        .replace(['-', '_'], " ")
                        .split_whitespace()
                        .map(|word| {
                            let mut c = word.chars();
                            match c.next() {
                                None => String::new(),
                                Some(ch) => ch.to_uppercase().collect::<String>() + c.as_str(),
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(" ");

                    let description = match key.as_str() {
                        "recompress-threshold" | "recompress_threshold" => "Uncompressed payload size threshold (in bytes) to trigger Deflate recompression",
                        "deflate-level" | "deflate_level" => "Deflate compression level (1 = fastest/low-latency, 9 = max compression)",
                        "discovery-targets" | "discovery_targets" => "Comma-separated target broadcast/multicast IP:port list",
                        "motd-template" | "motd_template" => "LAN broadcast discovery payload template string",
                        _ => "",
                    }.to_string();

                    fields.push(ConfigFieldSchema {
                        key,
                        field_type,
                        label,
                        description,
                        default_value: default_val,
                    });
                }
            }
        }
    }

    MiddlewareConfigSchema {
        name: name.to_string(),
        fields,
    }
}

pub fn compile_module_from_wat(
    engine: &Engine,
    name: &str,
    wat_bytes: &[u8],
) -> anyhow::Result<(Module, Option<MiddlewareConfigSchema>)> {
    let is_component = {
        let s = std::str::from_utf8(wat_bytes).unwrap_or("");
        s.lines()
            .map(|l| l.trim())
            .find(|l| !l.is_empty() && !l.starts_with(";;"))
            .map(|l| l.starts_with("(component"))
            .unwrap_or(false)
    };
    if is_component {
        let component = wasmtime::component::Component::new(engine, wat_bytes)
            .map_err(|e| anyhow::anyhow!("compile component '{name}': {e}"))?;
        let schema = extract_component_schema(engine, name, &component);
        let mut store = Store::new(engine, ());
        let linker = wasmtime::component::Linker::new(engine);
        let instance = linker
            .instantiate(&mut store, &component)
            .map_err(|e| anyhow::anyhow!("instantiate component '{name}': {e}"))?;
        let module = instance.get_module(&mut store, "main").ok_or_else(|| {
            anyhow::anyhow!("component '{name}' does not export core module 'main'")
        })?;
        Ok((module, Some(schema)))
    } else {
        let module = Module::new(engine, wat_bytes)
            .map_err(|e| anyhow::anyhow!("compile wat module '{name}': {e}"))?;
        Ok((module, None))
    }
}

static MIDDLEWARE_DYNAMIC_CONFIGS: LazyLock<
    Arc<StdRwLock<HashMap<String, HashMap<String, serde_json::Value>>>>,
> = LazyLock::new(|| Arc::new(StdRwLock::new(HashMap::new())));

pub fn set_dynamic_middleware_config(name: &str, config: HashMap<String, serde_json::Value>) {
    let mut store = MIDDLEWARE_DYNAMIC_CONFIGS.write().unwrap();
    let name_key = name.trim().to_ascii_lowercase();
    store.insert(name_key.clone(), config.clone());

    broadcast_session_config_update(&name_key, &config);
}

pub fn get_dynamic_middleware_config(name: &str) -> Option<HashMap<String, serde_json::Value>> {
    let store = MIDDLEWARE_DYNAMIC_CONFIGS.read().unwrap();
    store.get(&name.trim().to_ascii_lowercase()).cloned()
}

pub fn reset_dynamic_middleware_config(name: &str) {
    let mut store = MIDDLEWARE_DYNAMIC_CONFIGS.write().unwrap();
    let name_key = name.trim().to_ascii_lowercase();
    store.remove(&name_key);
}

pub fn get_all_dynamic_middleware_configs() -> HashMap<String, HashMap<String, serde_json::Value>> {
    let store = MIDDLEWARE_DYNAMIC_CONFIGS.read().unwrap();
    store.clone()
}

pub type SessionHandle = Arc<Mutex<WasmProtocolSession>>;
static ACTIVE_SESSIONS: LazyLock<
    Arc<StdRwLock<HashMap<String, Vec<Weak<Mutex<WasmProtocolSession>>>>>>,
> = LazyLock::new(|| Arc::new(StdRwLock::new(HashMap::new())));

pub fn register_active_session(name: &str, session: &SessionHandle) {
    let mut registry = ACTIVE_SESSIONS.write().unwrap();
    let list = registry
        .entry(name.trim().to_ascii_lowercase())
        .or_default();
    list.retain(|w| w.upgrade().is_some());
    list.push(Arc::downgrade(session));
}

pub fn broadcast_session_config_update(name: &str, config: &HashMap<String, serde_json::Value>) {
    let mut registry = ACTIVE_SESSIONS.write().unwrap();
    if let Some(list) = registry.get_mut(&name.trim().to_ascii_lowercase()) {
        list.retain(|w| {
            if let Some(s) = w.upgrade() {
                if let Ok(mut session) = s.lock() {
                    let _ = session.apply_config_map(config);
                }
                true
            } else {
                false
            }
        });
    }
}

pub const DEFAULT_SYMBOL_TABLE_CAPACITY: usize = 1024;

/// HPACK-style (RFC 7541) dynamic symbol table for zero-copy string deduplication.
#[derive(Debug, Clone)]
pub struct DynamicSymbolTable {
    capacity: usize,
    current_max_seq: u64,
    seq_to_str: HashMap<u64, Vec<u8>>,
    str_to_seq: HashMap<Vec<u8>, u64>,
}

impl DynamicSymbolTable {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            current_max_seq: 0,
            seq_to_str: HashMap::new(),
            str_to_seq: HashMap::new(),
        }
    }

    /// Intern a symbol into the dynamic table.
    ///
    /// Returns:
    /// - `(0 << 32) | index`: if symbol already exists in table (1-based index)
    /// - `(1 << 32) | 1`: if newly added (inserted at index 1)
    pub fn intern(&mut self, symbol: &[u8]) -> i64 {
        if let Some(&seq) = self.str_to_seq.get(symbol) {
            let index = self.current_max_seq.saturating_sub(seq) + 1;
            return index as i64; // High 32 bits = 0
        }

        self.current_max_seq += 1;
        let new_seq = self.current_max_seq;
        self.str_to_seq.insert(symbol.to_vec(), new_seq);
        self.seq_to_str.insert(new_seq, symbol.to_vec());

        if self.seq_to_str.len() > self.capacity {
            let oldest_seq = self.current_max_seq.saturating_sub(self.capacity as u64);
            if let Some(old_sym) = self.seq_to_str.remove(&oldest_seq) {
                self.str_to_seq.remove(&old_sym);
            }
        }

        (1i64 << 32) | 1i64
    }

    /// Resolve a 1-based index back to original symbol bytes.
    pub fn resolve(&self, index: i32) -> Option<&[u8]> {
        if index <= 0 {
            return None;
        }
        let target_seq = self.current_max_seq.checked_sub(index as u64 - 1)?;
        self.seq_to_str.get(&target_seq).map(|v| v.as_slice())
    }

    pub fn len(&self) -> usize {
        self.seq_to_str.len()
    }

    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.seq_to_str.is_empty()
    }
}

impl Default for DynamicSymbolTable {
    fn default() -> Self {
        Self::new(DEFAULT_SYMBOL_TABLE_CAPACITY)
    }
}

/// Offset at which [`WasmProtocolSession::poll`] writes the input buffer so it
/// cannot clobber driver structs, scratch, or injected key material in pages 0-15.
pub const WASM_INPUT_OFFSET: usize = 0x100000;

#[derive(Clone, Debug)]
pub struct HostEnv {
    pub sym_table_from_client: Arc<Mutex<DynamicSymbolTable>>,
    pub sym_table_from_server: Arc<Mutex<DynamicSymbolTable>>,
    pub flow_from_server: bool,
    pub announced: Vec<Vec<u8>>,
}

impl Default for HostEnv {
    fn default() -> Self {
        Self {
            sym_table_from_client: Arc::new(Mutex::new(DynamicSymbolTable::default())),
            sym_table_from_server: Arc::new(Mutex::new(DynamicSymbolTable::default())),
            flow_from_server: false,
            announced: Vec::new(),
        }
    }
}

impl HostEnv {
    fn active_sym_table(&self) -> &Arc<Mutex<DynamicSymbolTable>> {
        if self.flow_from_server {
            &self.sym_table_from_server
        } else {
            &self.sym_table_from_client
        }
    }
}

/// Standalone RSA PKCS#1 v1.5 decryption helper.
pub fn crypto_rsa_decrypt(key_der: &[u8], ciphertext: &[u8]) -> Result<Vec<u8>, i32> {
    let key = RsaPrivateKey::from_pkcs1_der(key_der)
        .or_else(|_| RsaPrivateKey::from_pkcs8_der(key_der))
        .map_err(|_| -1)?;

    key.decrypt(Pkcs1v15Encrypt, ciphertext).map_err(|_| -2)
}

/// Standalone AES-128-CFB8 in-place encryption/decryption helper.
pub fn crypto_aes_cfb8(
    key: &[u8; 16],
    iv: &mut [u8; 16],
    data: &mut [u8],
    is_encrypt: bool,
) -> Result<(), i32> {
    let key_array = Array::from(*key);
    let cipher = Aes128::new(&key_array);
    for b in data.iter_mut() {
        let mut block = Array::from(*iv);
        cipher.encrypt_block(&mut block);
        let keystream = block[0];
        let in_b = *b;
        let out_b = in_b ^ keystream;
        *b = out_b;
        let feedback = if is_encrypt { out_b } else { in_b };
        iv.copy_within(1..16, 0);
        iv[15] = feedback;
    }
    Ok(())
}

/// Standalone Deflate/Zlib decompress helper (Zlib RFC 1950 with raw Deflate RFC 1951 fallback).
pub fn deflate_decompress(input: &[u8]) -> Result<Vec<u8>, i32> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    match miniz_oxide::inflate::decompress_to_vec_zlib(input) {
        Ok(out) => Ok(out),
        Err(_) => match miniz_oxide::inflate::decompress_to_vec(input) {
            Ok(out) => Ok(out),
            Err(_) => Err(-2),
        },
    }
}

/// Standalone Deflate/Zlib compress helper (Zlib RFC 1950).
pub fn deflate_compress(input: &[u8], level: i32) -> Result<Vec<u8>, i32> {
    let lvl = level.clamp(0, 10) as u8;
    Ok(miniz_oxide::deflate::compress_to_vec_zlib(input, lvl))
}

#[cfg(test)]
pub mod legacy_varint_framing {
    use super::*;

    /// Encodes an unsigned integer as a 7-bit continuation VarInt into `buf`.
    pub fn write_varint(buf: &mut Vec<u8>, mut val: u32) {
        loop {
            let mut b = (val & 0x7F) as u8;
            val >>= 7;
            if val != 0 {
                b |= 0x80;
            }
            buf.push(b);
            if val == 0 {
                break;
            }
        }
    }

    /// Reads a 7-bit continuation VarInt from the start of `buf`.
    /// Returns `Some((value, bytes_read))` or `None` if incomplete or invalid.
    pub fn read_varint(buf: &[u8]) -> Option<(u32, usize)> {
        let mut val = 0u32;
        let mut shift = 0;
        for (i, &b) in buf.iter().enumerate().take(5) {
            val |= ((b & 0x7F) as u32) << shift;
            if (b & 0x80) == 0 {
                return Some((val, i + 1));
            }
            shift += 7;
        }
        None
    }

    /// Frames an uncompressed packet payload as `[VarInt length][0x00][raw_payload]`.
    pub fn frame_uncompressed_packet(raw_payload: &[u8]) -> Vec<u8> {
        let packet_len = 1 + raw_payload.len();
        let mut out = Vec::with_capacity(5 + packet_len);
        write_varint(&mut out, packet_len as u32);
        out.push(0x00);
        out.extend_from_slice(raw_payload);
        out
    }

    /// Re-compresses an uncompressed packet payload as `[VarInt length][VarInt data_len][deflate]`.
    pub fn deflate_recompress_packet(raw_payload: &[u8], threshold: usize) -> Vec<u8> {
        if raw_payload.len() >= threshold {
            if let Ok(compressed) = deflate_compress(raw_payload, 1) {
                let mut dl_buf = Vec::new();
                write_varint(&mut dl_buf, raw_payload.len() as u32);
                let packet_len = dl_buf.len() + compressed.len();
                let mut out = Vec::with_capacity(5 + packet_len);
                write_varint(&mut out, packet_len as u32);
                out.extend_from_slice(&dl_buf);
                out.extend_from_slice(&compressed);
                return out;
            }
        }
        frame_uncompressed_packet(raw_payload)
    }

    /// Recompresses packets in a stream buffer that have `data_length == 0` and payload size >= threshold.
    /// Writes complete processed packets to `writer`, keeping any incomplete trailing packet in `pending`.
    pub async fn recompress_packet_stream<W: tokio::io::AsyncWrite + Unpin>(
        pending: &mut Vec<u8>,
        writer: &mut W,
        threshold: usize,
    ) -> std::io::Result<usize> {
        let mut written = 0;
        let mut offset = 0;
        while offset < pending.len() {
            let slice = &pending[offset..];
            let Some((pkt_len, len_n)) = read_varint(slice) else {
                break;
            };
            let total_pkt_len = len_n + pkt_len as usize;
            if slice.len() < total_pkt_len {
                break;
            }
            let pkt_data = &slice[len_n..total_pkt_len];
            if let Some((data_len, dl_n)) = read_varint(pkt_data) {
                if data_len == 0 {
                    let uncompressed_payload = &pkt_data[dl_n..];
                    if uncompressed_payload.len() >= threshold {
                        let recompressed =
                            deflate_recompress_packet(uncompressed_payload, threshold);
                        tokio::io::AsyncWriteExt::write_all(writer, &recompressed).await?;
                        written += recompressed.len();
                        offset += total_pkt_len;
                        continue;
                    }
                }
            }
            tokio::io::AsyncWriteExt::write_all(writer, &slice[..total_pkt_len]).await?;
            written += total_pkt_len;
            offset += total_pkt_len;
        }
        if offset > 0 {
            pending.drain(..offset);
        }
        Ok(written)
    }
}

pub fn host_crypto_rsa_decrypt(
    mut caller: Caller<'_, HostEnv>,
    key_ptr: i32,
    key_len: i32,
    in_ptr: i32,
    in_len: i32,
    out_ptr: i32,
) -> i32 {
    if key_ptr < 0 || key_len <= 0 || in_ptr < 0 || in_len <= 0 || out_ptr < 0 {
        return -3;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -3,
    };
    let mem_size = memory.data_size(&caller);

    let key_end = (key_ptr as usize).saturating_add(key_len as usize);
    let in_end = (in_ptr as usize).saturating_add(in_len as usize);
    if key_end > mem_size || in_end > mem_size {
        return -3;
    }

    let mut key_bytes = vec![0u8; key_len as usize];
    if memory
        .read(&caller, key_ptr as usize, &mut key_bytes)
        .is_err()
    {
        return -3;
    }

    let mut in_bytes = vec![0u8; in_len as usize];
    if memory
        .read(&caller, in_ptr as usize, &mut in_bytes)
        .is_err()
    {
        return -3;
    }

    let decrypted = match crypto_rsa_decrypt(&key_bytes, &in_bytes) {
        Ok(d) => d,
        Err(code) => return code,
    };

    let out_end = (out_ptr as usize).saturating_add(decrypted.len());
    if out_end > mem_size {
        return -3;
    }

    if memory
        .write(&mut caller, out_ptr as usize, &decrypted)
        .is_err()
    {
        return -3;
    }

    decrypted.len() as i32
}

pub fn host_crypto_aes_cfb8(
    mut caller: Caller<'_, HostEnv>,
    key_ptr: i32,
    iv_ptr: i32,
    data_ptr: i32,
    data_len: i32,
    is_encrypt: i32,
) -> i32 {
    if key_ptr < 0 || iv_ptr < 0 || data_ptr < 0 || data_len < 0 {
        return -1;
    }
    if data_len == 0 {
        return 0;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);

    let key_end = (key_ptr as usize).saturating_add(16);
    let iv_end = (iv_ptr as usize).saturating_add(16);
    let data_end = (data_ptr as usize).saturating_add(data_len as usize);

    if key_end > mem_size || iv_end > mem_size || data_end > mem_size {
        return -1;
    }

    let mut key = [0u8; 16];
    if memory.read(&caller, key_ptr as usize, &mut key).is_err() {
        return -1;
    }

    let mut iv = [0u8; 16];
    if memory.read(&caller, iv_ptr as usize, &mut iv).is_err() {
        return -1;
    }

    let mut buf = vec![0u8; data_len as usize];
    if memory.read(&caller, data_ptr as usize, &mut buf).is_err() {
        return -1;
    }

    if let Err(code) = crypto_aes_cfb8(&key, &mut iv, &mut buf, is_encrypt != 0) {
        return code;
    }

    if memory.write(&mut caller, data_ptr as usize, &buf).is_err() {
        return -1;
    }

    if memory.write(&mut caller, iv_ptr as usize, &iv).is_err() {
        return -1;
    }

    0
}

pub fn host_deflate_decompress(
    mut caller: Caller<'_, HostEnv>,
    in_ptr: i32,
    in_len: i32,
    out_ptr: i32,
    out_max_len: i32,
) -> i32 {
    if in_ptr < 0 || in_len < 0 || out_ptr < 0 || out_max_len < 0 {
        return -1;
    }
    if in_len == 0 {
        return 0;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);

    let in_end = (in_ptr as usize).saturating_add(in_len as usize);
    if in_end > mem_size {
        return -1;
    }

    let mut in_bytes = vec![0u8; in_len as usize];
    if memory
        .read(&caller, in_ptr as usize, &mut in_bytes)
        .is_err()
    {
        return -1;
    }

    let decompressed = match deflate_decompress(&in_bytes) {
        Ok(d) => d,
        Err(code) => return code,
    };

    if decompressed.len() > out_max_len as usize && out_max_len > 0 {
        return -3;
    }

    let out_end = (out_ptr as usize).saturating_add(decompressed.len());
    let mem_size = memory.data_size(&caller);
    if out_end > mem_size {
        let delta = out_end - mem_size;
        let pages = delta.div_ceil(65536);
        if memory.grow(&mut caller, pages as u64).is_err() {
            return -3;
        }
    }

    if memory
        .write(&mut caller, out_ptr as usize, &decompressed)
        .is_err()
    {
        return -3;
    }

    decompressed.len() as i32
}

pub fn host_deflate_compress(
    mut caller: Caller<'_, HostEnv>,
    in_ptr: i32,
    in_len: i32,
    out_ptr: i32,
    out_max_len: i32,
    level: i32,
) -> i32 {
    if in_ptr < 0 || in_len < 0 || out_ptr < 0 || out_max_len < 0 {
        return -1;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);

    let in_end = (in_ptr as usize).saturating_add(in_len as usize);
    if in_end > mem_size {
        return -1;
    }

    let mut in_bytes = vec![0u8; in_len as usize];
    if memory
        .read(&caller, in_ptr as usize, &mut in_bytes)
        .is_err()
    {
        return -1;
    }

    let compressed = match deflate_compress(&in_bytes, level) {
        Ok(c) => c,
        Err(code) => return code,
    };

    if compressed.len() > out_max_len as usize {
        return -3;
    }

    let out_end = (out_ptr as usize).saturating_add(compressed.len());
    if out_end > mem_size {
        return -3;
    }

    if memory
        .write(&mut caller, out_ptr as usize, &compressed)
        .is_err()
    {
        return -3;
    }

    compressed.len() as i32
}

pub fn host_sym_intern(mut caller: Caller<'_, HostEnv>, str_ptr: i32, str_len: i32) -> i64 {
    if str_ptr < 0 || str_len < 0 {
        return -1;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);

    let end = (str_ptr as usize).saturating_add(str_len as usize);
    if end > mem_size {
        return -1;
    }

    let mut sym_bytes = vec![0u8; str_len as usize];
    if memory
        .read(&caller, str_ptr as usize, &mut sym_bytes)
        .is_err()
    {
        return -1;
    }

    let table_arc = caller.data().active_sym_table().clone();
    let mut table = table_arc.lock().unwrap();
    table.intern(&sym_bytes)
}

pub fn host_sym_resolve(
    mut caller: Caller<'_, HostEnv>,
    index: i32,
    out_ptr: i32,
    max_len: i32,
) -> i32 {
    if index <= 0 || out_ptr < 0 || max_len < 0 {
        return -1;
    }
    let table_arc = caller.data().active_sym_table().clone();
    let sym_bytes = {
        let table = table_arc.lock().unwrap();
        match table.resolve(index) {
            Some(s) => s.to_vec(),
            None => return -1,
        }
    };

    if sym_bytes.len() > max_len as usize {
        return -3;
    }

    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);

    let out_end = (out_ptr as usize).saturating_add(sym_bytes.len());
    if out_end > mem_size {
        return -2;
    }

    if memory
        .write(&mut caller, out_ptr as usize, &sym_bytes)
        .is_err()
    {
        return -2;
    }

    sym_bytes.len() as i32
}

pub fn host_announce_session_data(
    mut caller: Caller<'_, HostEnv>,
    ptr: i32,
    len: i32,
) -> i32 {
    if ptr < 0 || len <= 0 {
        return -1;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);
    let end = (ptr as usize).saturating_add(len as usize);
    if end > mem_size {
        return -1;
    }
    let mut buf = vec![0u8; len as usize];
    if memory.read(&caller, ptr as usize, &mut buf).is_err() {
        return -1;
    }
    caller.data_mut().announced.push(buf);
    0
}

pub fn host_mem_copy(
    mut caller: Caller<'_, HostEnv>,
    dst: i32,
    src: i32,
    n: i32,
) -> i32 {
    if n <= 0 {
        return 0;
    }
    if dst < 0 || src < 0 {
        return -1;
    }
    let memory = match caller.get_export("memory").and_then(|e| e.into_memory()) {
        Some(m) => m,
        None => return -1,
    };
    let mem_size = memory.data_size(&caller);
    let n = n as usize;
    let dst = dst as usize;
    let src = src as usize;
    if dst.saturating_add(n) > mem_size || src.saturating_add(n) > mem_size {
        return -2;
    }
    let data = memory.data_mut(&mut caller);
    data.copy_within(src..src + n, dst);
    0
}

pub fn create_prism_linker(engine: &Engine) -> anyhow::Result<Linker<HostEnv>> {
    let mut linker = Linker::new(engine);
    linker.func_wrap("prism", "crypto_rsa_decrypt", host_crypto_rsa_decrypt)?;
    linker.func_wrap("prism", "crypto_aes_cfb8", host_crypto_aes_cfb8)?;
    linker.func_wrap("prism", "deflate_decompress", host_deflate_decompress)?;
    linker.func_wrap("prism", "deflate_compress", host_deflate_compress)?;
    linker.func_wrap("prism", "sym_intern", host_sym_intern)?;
    linker.func_wrap("prism", "sym_resolve", host_sym_resolve)?;
    linker.func_wrap("prism", "announce_session_data", host_announce_session_data)?;
    linker.func_wrap("prism", "mem_copy", host_mem_copy)?;
    Ok(linker)
}

pub struct WasmProtocolSession {
    store: Store<HostEnv>,
    #[allow(dead_code)]
    instance: Instance,
    memory: Memory,
    poll_fn: Option<TypedFunc<(i32, i32, i32), i64>>,
    #[allow(dead_code)]
    set_data_fn: Option<TypedFunc<(i32, i32), i32>>,
    state: SessionState,
    /// Bytes currently valid at [`WASM_INPUT_OFFSET`].
    input_len: usize,
    /// Prefix of `input_len` consumed by the last streaming frame, if any.
    last_consumed: usize,
    input_head: [u8; 8],
    input_head_len: usize,
}

#[allow(dead_code)]
impl WasmProtocolSession {
    pub fn new(engine: &Engine, module: &Module) -> anyhow::Result<Self> {
        let mut store = Store::new(engine, HostEnv::default());
        let linker = create_prism_linker(engine)?;
        let instance = linker
            .instantiate(&mut store, module)
            .map_err(|e| anyhow::anyhow!("session: instantiate wasm module: {e}"))?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .ok_or_else(|| anyhow::anyhow!("session: wasm missing exported memory 'memory'"))?;

        let poll_fn = instance
            .get_typed_func::<(i32, i32, i32), i64>(&mut store, "poll")
            .ok();
        let set_data_fn = instance
            .get_typed_func::<(i32, i32), i32>(&mut store, "set_data")
            .ok();

        Ok(Self {
            store,
            instance,
            memory,
            poll_fn,
            set_data_fn,
            state: SessionState::Handshake,
            input_len: 0,
            last_consumed: 0,
            input_head: [0u8; 8],
            input_head_len: 0,
        })
    }

    pub fn from_wat(wat: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        let engine = Engine::default();
        let (module, _) = compile_module_from_wat(&engine, "default", wat.as_ref())?;
        Self::new(&engine, &module)
    }

    pub fn into_shared(self, name: &str) -> SessionHandle {
        let handle = Arc::new(Mutex::new(self));
        register_active_session(name, &handle);
        handle
    }

    pub fn state(&self) -> SessionState {
        self.state
    }

    pub fn set_state(&mut self, state: SessionState) {
        self.state = state;
    }

    pub fn has_poll(&self) -> bool {
        self.poll_fn.is_some()
    }

    pub fn has_set_data(&self) -> bool {
        self.set_data_fn.is_some()
    }

    pub fn sym_table(&self) -> Arc<Mutex<DynamicSymbolTable>> {
        self.store.data().active_sym_table().clone()
    }

    pub fn memory(&self) -> &Memory {
        &self.memory
    }

    pub fn store(&self) -> &Store<HostEnv> {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut Store<HostEnv> {
        &mut self.store
    }

    pub fn instance(&self) -> &Instance {
        &self.instance
    }

    pub fn get_typed_func<Params, Results>(
        &mut self,
        name: &str,
    ) -> anyhow::Result<TypedFunc<Params, Results>>
    where
        Params: WasmParams,
        Results: WasmResults,
    {
        self.instance
            .get_typed_func(&mut self.store, name)
            .map_err(|e| anyhow::anyhow!("{e}"))
    }

    pub fn read_memory(&self, offset: usize, buf: &mut [u8]) -> Result<(), MiddlewareError> {
        self.memory
            .read(&self.store, offset, buf)
            .map_err(|e| MiddlewareError::Fatal(format!("wasm read memory failed: {e}")))
    }

    pub fn write_memory(&mut self, offset: usize, data: &[u8]) -> Result<(), MiddlewareError> {
        self.memory
            .write(&mut self.store, offset, data)
            .map_err(|e| MiddlewareError::Fatal(format!("wasm write memory failed: {e}")))
    }

    pub fn take_announced(&mut self) -> Vec<Vec<u8>> {
        std::mem::take(&mut self.store.data_mut().announced)
    }

    pub fn set_conn_state(&mut self, state: i32) {
        if let Ok(func) = self
            .instance
            .get_typed_func::<(i32,), ()>(&mut self.store, "set_conn_state")
        {
            let _ = func.call(&mut self.store, (state,));
        }
    }

    /// `from_server = true` means the buffer is origin→client (clientbound).
    pub fn set_flow_direction(&mut self, from_server: bool) {
        self.store.data_mut().flow_from_server = from_server;
        let dir = if from_server { 1i32 } else { 0i32 };
        if let Ok(func) = self
            .instance
            .get_typed_func::<(i32,), ()>(&mut self.store, "set_direction")
        {
            let _ = func.call(&mut self.store, (dir,));
        } else if let Ok(func) = self
            .instance
            .get_typed_func::<(i32,), i32>(&mut self.store, "set_direction")
        {
            let _ = func.call(&mut self.store, (dir,));
        }
    }

    pub fn set_session_data(&mut self, data: &[u8]) -> Result<i32, MiddlewareError> {
        let func = match self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "set_session_data")
        {
            Ok(f) => f,
            Err(_) => return Ok(-1),
        };
        let needed = WASM_INPUT_OFFSET + data.len() + 64;
        let mem_size = self.memory.data_size(&self.store);
        if needed > mem_size {
            let pages = (needed - mem_size).div_ceil(65536);
            self.memory
                .grow(&mut self.store, pages as u64)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm memory grow failed: {e}")))?;
        }
        if !data.is_empty() {
            self.memory
                .write(&mut self.store, WASM_INPUT_OFFSET, data)
                .map_err(|e| {
                    MiddlewareError::Fatal(format!("wasm write set_session_data failed: {e}"))
                })?;
        }
        func.call(
            &mut self.store,
            (WASM_INPUT_OFFSET as i32, data.len() as i32),
        )
        .map_err(|e| MiddlewareError::Fatal(format!("wasm set_session_data call failed: {e}")))
    }

    pub fn set_data(&mut self, data: &[u8]) -> Result<i32, MiddlewareError> {
        let set_data_fn = match &self.set_data_fn {
            Some(f) => f.clone(),
            None => {
                return Err(MiddlewareError::Fatal(
                    "wasm module missing 'set_data' export".into(),
                ));
            }
        };

        let needed = (data.len() as usize).max(65536 * 4);
        let mem_size = self.memory.data_size(&self.store);
        if needed > mem_size {
            let delta = needed - mem_size;
            let pages = delta.div_ceil(65536);
            self.memory
                .grow(&mut self.store, pages as u64)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm memory grow failed: {e}")))?;
        }

        if !data.is_empty() {
            self.memory
                .write(&mut self.store, 0, data)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm write set_data failed: {e}")))?;
        }

        let code = set_data_fn
            .call(&mut self.store, (0, data.len() as i32))
            .map_err(|e| MiddlewareError::Fatal(format!("wasm set_data call failed: {e}")))?;

        Ok(code)
    }

    pub fn update_config(
        &mut self,
        key: &str,
        val: &serde_json::Value,
    ) -> Result<(), MiddlewareError> {
        let mut map = HashMap::new();
        map.insert(key.to_string(), val.clone());
        self.apply_config_map(&map)
    }

    pub fn apply_config_map(
        &mut self,
        config: &HashMap<String, serde_json::Value>,
    ) -> Result<(), MiddlewareError> {
        for (key, val) in config {
            let norm_key = key.replace('-', "_");
            let func_name = format!("set_{norm_key}");

            match val {
                serde_json::Value::Number(n) => {
                    if let Some(v) = n.as_i64() {
                        if let Ok(func) = self
                            .instance
                            .get_typed_func::<(i32,), ()>(&mut self.store, &func_name)
                        {
                            let _ = func.call(&mut self.store, (v as i32,));
                        } else if let Ok(func) = self
                            .instance
                            .get_typed_func::<(i32,), i32>(&mut self.store, &func_name)
                        {
                            let _ = func.call(&mut self.store, (v as i32,));
                        } else if let Ok(func) = self
                            .instance
                            .get_typed_func::<(i64,), ()>(&mut self.store, &func_name)
                        {
                            let _ = func.call(&mut self.store, (v,));
                        } else if let Ok(func) = self
                            .instance
                            .get_typed_func::<(i64,), i32>(&mut self.store, &func_name)
                        {
                            let _ = func.call(&mut self.store, (v,));
                        }
                    }
                }
                serde_json::Value::Bool(b) => {
                    let v = if *b { 1i32 } else { 0i32 };
                    if let Ok(func) = self
                        .instance
                        .get_typed_func::<(i32,), ()>(&mut self.store, &func_name)
                    {
                        let _ = func.call(&mut self.store, (v,));
                    } else if let Ok(func) = self
                        .instance
                        .get_typed_func::<(i32,), i32>(&mut self.store, &func_name)
                    {
                        let _ = func.call(&mut self.store, (v,));
                    }
                }
                serde_json::Value::String(s) => {
                    let bytes = s.as_bytes();
                    let scratch_ptr = 131072usize;
                    let scratch_len = bytes.len();
                    let mem_size = self.memory.data_size(&self.store);
                    if scratch_ptr + scratch_len > mem_size {
                        let needed = scratch_ptr + scratch_len;
                        let delta = needed - mem_size;
                        let pages = delta.div_ceil(65536);
                        let _ = self.memory.grow(&mut self.store, pages as u64);
                    }
                    if self
                        .memory
                        .write(&mut self.store, scratch_ptr, bytes)
                        .is_ok()
                    {
                        if let Ok(func) = self
                            .instance
                            .get_typed_func::<(i32, i32), ()>(&mut self.store, &func_name)
                        {
                            let _ = func
                                .call(&mut self.store, (scratch_ptr as i32, scratch_len as i32));
                        } else if let Ok(func) = self
                            .instance
                            .get_typed_func::<(i32, i32), i32>(&mut self.store, &func_name)
                        {
                            let _ = func
                                .call(&mut self.store, (scratch_ptr as i32, scratch_len as i32));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn write_poll_input(&mut self, buf: &[u8]) -> Result<(), MiddlewareError> {
        let needed = WASM_INPUT_OFFSET + buf.len() + 65536;
        let mem_size = self.memory.data_size(&self.store);
        if needed > mem_size {
            let delta = needed - mem_size;
            let pages = delta.div_ceil(65536);
            self.memory
                .grow(&mut self.store, pages as u64)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm memory grow failed: {e}")))?;
        }

        if !buf.is_empty() {
            self.memory
                .write(&mut self.store, WASM_INPUT_OFFSET, buf)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm write buf failed: {e}")))?;
        }
        self.input_len = buf.len();
        self.last_consumed = 0;
        self.remember_input_head(buf);
        Ok(())
    }

    fn remember_input_head(&mut self, buf: &[u8]) {
        self.input_head_len = buf.len().min(8);
        self.input_head[..self.input_head_len].copy_from_slice(&buf[..self.input_head_len]);
    }

    pub fn poll(&mut self, buf: &[u8]) -> Result<PollResult, MiddlewareError> {
        let poll_fn = match &self.poll_fn {
            Some(f) => f.clone(),
            None => {
                return Err(MiddlewareError::Fatal(
                    "wasm module missing 'poll' export".into(),
                ));
            }
        };

        self.write_poll_input(buf)?;

        let res = poll_fn
            .call(
                &mut self.store,
                (
                    WASM_INPUT_OFFSET as i32,
                    buf.len() as i32,
                    self.state as i32,
                ),
            )
            .map_err(|e| MiddlewareError::Fatal(format!("wasm poll call failed: {e}")))?;

        let action = ((res as u64) >> 32) as u32;
        let value = (res as u64 & 0xffff_ffff) as u32;

        match self.state {
            SessionState::Handshake => match action {
                0 => Ok(PollResult::Handshake(HandshakeResult::NeedMoreData)),
                1 => {
                    let mem_size = self.memory.data_size(&self.store);
                    if (value as usize) + 16 > mem_size {
                        return Err(MiddlewareError::Fatal(format!(
                            "route match struct pointer out of bounds: {value}"
                        )));
                    }
                    let mut header = [0u8; 16];
                    self.memory
                        .read(&self.store, value as usize, &mut header)
                        .map_err(|e| {
                            MiddlewareError::Fatal(format!("read route struct failed: {e}"))
                        })?;

                    let host_ptr = u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
                    let host_len = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
                    let rw_ptr = u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;
                    let rw_len = u32::from_le_bytes(header[12..16].try_into().unwrap()) as usize;

                    let mut host = None;
                    if host_len > 0 {
                        if host_ptr + host_len > mem_size {
                            return Err(MiddlewareError::Fatal(
                                "host pointer out of bounds".into(),
                            ));
                        }
                        let mut hbuf = vec![0u8; host_len];
                        self.memory
                            .read(&self.store, host_ptr, &mut hbuf)
                            .map_err(|e| {
                                MiddlewareError::Fatal(format!("read host failed: {e}"))
                            })?;
                        let h = String::from_utf8_lossy(&hbuf).trim().to_ascii_lowercase();
                        if !h.is_empty() {
                            host = Some(h);
                        }
                    }

                    let mut rewrite = None;
                    if rw_len > 0 {
                        if rw_ptr + rw_len > mem_size {
                            return Err(MiddlewareError::Fatal(
                                "rewrite pointer out of bounds".into(),
                            ));
                        }
                        let mut rwbuf = vec![0u8; rw_len];
                        self.memory
                            .read(&self.store, rw_ptr, &mut rwbuf)
                            .map_err(|e| {
                                MiddlewareError::Fatal(format!("read rewrite failed: {e}"))
                            })?;
                        rewrite = Some(rwbuf);
                    }

                    self.state = SessionState::Streaming;
                    self.input_len = 0;
                    self.last_consumed = 0;
                    self.input_head_len = 0;
                    Ok(PollResult::Handshake(HandshakeResult::RouteMatch {
                        host,
                        rewrite,
                    }))
                }
                2 => {
                    self.input_len = 0;
                    self.last_consumed = 0;
                    self.input_head_len = 0;
                    Ok(PollResult::Handshake(HandshakeResult::NoMatch))
                }
                other => Err(MiddlewareError::Fatal(format!(
                    "invalid handshake action code: {other}"
                ))),
            },
            SessionState::Streaming | SessionState::StreamingEgress => match action {
                0 => Ok(PollResult::Stream(StreamResult::NeedMoreData)),
                6 => Ok(PollResult::Stream(StreamResult::Blocked)),
                action => {
                    let Some((priority, rewritten)) = FramePriority::from_stream_action(action)
                    else {
                        return Err(MiddlewareError::Fatal(format!(
                            "invalid streaming action code: {action}"
                        )));
                    };
                    if !rewritten {
                        self.last_consumed = value as usize;
                        return Ok(PollResult::Stream(StreamResult::Frame {
                            len: value as usize,
                            priority,
                            payload: None,
                            incompressible: false,
                        }));
                    }
                    let mem_size = self.memory.data_size(&self.store);
                    let ptr = value as usize;
                    if ptr + 12 > mem_size {
                        return Err(MiddlewareError::Fatal(format!(
                            "stream frame struct pointer out of bounds: {value}"
                        )));
                    }
                    let mut header = [0u8; 12];
                    self.memory
                        .read(&self.store, ptr, &mut header)
                        .map_err(|e| {
                            MiddlewareError::Fatal(format!("read stream frame struct failed: {e}"))
                        })?;

                    let consumed_len =
                        u32::from_le_bytes(header[0..4].try_into().unwrap()) as usize;
                    let payload_ptr = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
                    let payload_len =
                        u32::from_le_bytes(header[8..12].try_into().unwrap()) as usize;

                    if payload_ptr + payload_len > mem_size {
                        return Err(MiddlewareError::Fatal(format!(
                            "stream payload pointer out of bounds: {payload_ptr} + {payload_len}"
                        )));
                    }

                    let mut payload = vec![0u8; payload_len];
                    self.memory
                        .read(&self.store, payload_ptr, &mut payload)
                        .map_err(|e| {
                            MiddlewareError::Fatal(format!("read stream payload failed: {e}"))
                        })?;

                    let mut incompressible = false;
                    if ptr + 16 <= mem_size {
                        let mut extra = [0u8; 4];
                        if self.memory.read(&self.store, ptr + 12, &mut extra).is_ok() {
                            incompressible = u32::from_le_bytes(extra) & 1 != 0;
                        }
                    }

                    self.last_consumed = consumed_len;
                    Ok(PollResult::Stream(StreamResult::Frame {
                        len: consumed_len,
                        priority,
                        payload: Some(payload),
                        incompressible,
                    }))
                }
            },
        }
    }

    pub fn poll_ingress(&mut self, buf: &[u8]) -> Result<PollResult, MiddlewareError> {
        self.set_state(SessionState::Streaming);
        self.poll(buf)
    }

    pub fn poll_egress(&mut self, buf: &[u8]) -> Result<PollResult, MiddlewareError> {
        self.set_state(SessionState::StreamingEgress);
        self.poll(buf)
    }

    /// Generic egress stream processing: transforms buffer through WASM egress driver
    /// and writes resulting frames to the writer.
    pub async fn process_egress_stream<W: tokio::io::AsyncWrite + Unpin>(
        &mut self,
        pending: &mut Vec<u8>,
        writer: &mut W,
    ) -> std::io::Result<usize> {
        self.set_state(SessionState::StreamingEgress);
        let mut written = 0;
        let mut offset = 0;
        while offset < pending.len() {
            let slice = &pending[offset..];
            match self.poll(slice) {
                Ok(PollResult::Stream(StreamResult::Frame { len, payload, .. })) => {
                    if len == 0 || len > slice.len() {
                        break;
                    }
                    if let Some(ref p) = payload {
                        tokio::io::AsyncWriteExt::write_all(writer, p).await?;
                        written += p.len();
                    } else {
                        tokio::io::AsyncWriteExt::write_all(writer, &slice[..len]).await?;
                        written += len;
                    }
                    offset += len;
                }
                Ok(PollResult::Stream(StreamResult::NeedMoreData)) => break,
                Ok(PollResult::Stream(StreamResult::Blocked)) => break,
                _ => {
                    tokio::io::AsyncWriteExt::write_all(writer, slice).await?;
                    written += slice.len();
                    offset += slice.len();
                    break;
                }
            }
        }
        if offset > 0 {
            pending.drain(..offset);
        }
        Ok(written)
    }

    /// Queries the discovery target addresses defined by the WASM module.
    /// If the module exports `discovery_targets`, returns the parsed list of target addresses.
    pub fn discovery_targets(&mut self) -> Result<Option<Vec<String>>, MiddlewareError> {
        let func = match self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "discovery_targets")
        {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let out_ptr = 65536;
        let max_len = 1024;
        let res = func
            .call(&mut self.store, (out_ptr, max_len))
            .map_err(|e| {
                MiddlewareError::Fatal(format!("wasm discovery_targets call failed: {e}"))
            })?;

        if res <= 0 {
            return Ok(Some(Vec::new()));
        }

        let mut buf = vec![0u8; res as usize];
        self.memory
            .read(&self.store, out_ptr as usize, &mut buf)
            .map_err(|e| {
                MiddlewareError::Fatal(format!("wasm read discovery_targets failed: {e}"))
            })?;

        let s = String::from_utf8(buf)
            .map_err(|e| MiddlewareError::Fatal(format!("invalid utf-8 discovery_targets: {e}")))?;

        let targets: Vec<String> = s
            .split([',', '\n', ';'])
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();

        Ok(Some(targets))
    }

    /// Queries the discovery template string defined by the WASM module.
    /// If the module exports `discovery_template`, returns the template string.
    pub fn discovery_template(&mut self) -> Result<Option<String>, MiddlewareError> {
        let func = match self
            .instance
            .get_typed_func::<(i32, i32), i32>(&mut self.store, "discovery_template")
        {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let out_ptr = 65536;
        let max_len = 1024;
        let res = func
            .call(&mut self.store, (out_ptr, max_len))
            .map_err(|e| {
                MiddlewareError::Fatal(format!("wasm discovery_template call failed: {e}"))
            })?;

        if res <= 0 {
            return Ok(None);
        }

        let mut buf = vec![0u8; res as usize];
        self.memory
            .read(&self.store, out_ptr as usize, &mut buf)
            .map_err(|e| {
                MiddlewareError::Fatal(format!("wasm read discovery_template failed: {e}"))
            })?;

        let s = String::from_utf8(buf).map_err(|e| {
            MiddlewareError::Fatal(format!("invalid utf-8 discovery_template: {e}"))
        })?;

        Ok(Some(s))
    }

    /// Generates discovery payload using the WASM module's `build_discovery_payload` export.
    pub fn build_discovery_payload(
        &mut self,
        name: &str,
        port: u16,
        prefix: &str,
    ) -> Result<Option<Vec<u8>>, MiddlewareError> {
        let func = match self
            .instance
            .get_typed_func::<(i32, i32, i32, i32, i32, i32, i32), i32>(
                &mut self.store,
                "build_discovery_payload",
            ) {
            Ok(f) => f,
            Err(_) => return Ok(None),
        };

        let name_bytes = name.as_bytes();
        let prefix_bytes = prefix.as_bytes();

        let name_ptr = 0i32;
        let name_len = name_bytes.len() as i32;
        let prefix_ptr = name_len;
        let prefix_len = prefix_bytes.len() as i32;

        let out_ptr = 4096i32;
        let out_max_len = 8192i32;

        if !name_bytes.is_empty() {
            self.memory
                .write(&mut self.store, name_ptr as usize, name_bytes)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm write name failed: {e}")))?;
        }
        if !prefix_bytes.is_empty() {
            self.memory
                .write(&mut self.store, prefix_ptr as usize, prefix_bytes)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm write prefix failed: {e}")))?;
        }

        let res = func
            .call(
                &mut self.store,
                (
                    name_ptr,
                    name_len,
                    port as i32,
                    prefix_ptr,
                    prefix_len,
                    out_ptr,
                    out_max_len,
                ),
            )
            .map_err(|e| {
                MiddlewareError::Fatal(format!("wasm build_discovery_payload call failed: {e}"))
            })?;

        if res < 0 {
            return Err(MiddlewareError::Fatal(
                "wasm build_discovery_payload returned error".into(),
            ));
        }

        let mut out = vec![0u8; res as usize];
        self.memory
            .read(&self.store, out_ptr as usize, &mut out)
            .map_err(|e| {
                MiddlewareError::Fatal(format!("wasm read discovery payload failed: {e}"))
            })?;

        Ok(Some(out))
    }
}

/// Queries discovery target addresses from the default protocol driver ("minecraft").
pub fn get_default_discovery_targets() -> Vec<String> {
    if let Some(wat) = get_default_middleware_wat("minecraft") {
        if let Ok(mut session) = WasmProtocolSession::from_wat(wat) {
            if let Ok(Some(targets)) = session.discovery_targets() {
                return targets;
            }
        }
    }
    vec![]
}

/// Builds discovery payload using the default protocol driver ("minecraft").
pub fn build_default_discovery_payload(name: &str, port: u16, prefix: &str) -> Option<String> {
    if let Some(wat) = get_default_middleware_wat("minecraft") {
        if let Ok(mut session) = WasmProtocolSession::from_wat(wat) {
            if let Ok(Some(bytes)) = session.build_discovery_payload(name, port, prefix) {
                return String::from_utf8(bytes).ok();
            }
        }
    }
    None
}

pub struct WasmMiddleware {
    name: String,
    path_hint: String,
    engine: Engine,
    module: Module,
    schema: Option<MiddlewareConfigSchema>,
}

impl WasmMiddleware {
    pub fn from_wat_path(name: &str, path: &Path) -> anyhow::Result<Self> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("middleware: empty wasm middleware name");
        }
        if path.as_os_str().is_empty() {
            anyhow::bail!("middleware: empty wasm middleware path");
        }

        if path
            .extension()
            .is_some_and(|e| e.to_string_lossy().eq_ignore_ascii_case("wasm"))
        {
            anyhow::bail!(
                "middleware: loading raw .wasm is disabled; provide a .wat file instead ({})",
                path.display()
            );
        }

        let wat_bytes = std::fs::read(path)
            .with_context(|| format!("middleware: read wat {}", path.display()))?;

        if wat_bytes.starts_with(b"\0asm") {
            anyhow::bail!(
                "middleware: expected WAT text input but got a wasm binary (path={})",
                path.display()
            );
        }

        Self::from_wat_bytes(name, path.display().to_string(), wat_bytes)
    }

    #[allow(dead_code)]
    pub fn from_wat(name: &str, wat: impl AsRef<[u8]>) -> anyhow::Result<Self> {
        Self::from_wat_bytes(name, name.to_string(), wat.as_ref().to_vec())
    }

    fn from_wat_bytes(name: &str, path_hint: String, wat_bytes: Vec<u8>) -> anyhow::Result<Self> {
        let name = name.trim();
        if name.is_empty() {
            anyhow::bail!("middleware: empty wasm middleware name");
        }

        let engine = Engine::default();
        let (module, schema) = compile_module_from_wat(&engine, name, &wat_bytes)?;

        Ok(Self {
            name: name.to_string(),
            path_hint,
            engine,
            module,
            schema,
        })
    }

    #[allow(dead_code)]
    pub fn from_module(name: &str, module: Module, engine: Engine) -> Self {
        Self {
            name: name.to_string(),
            path_hint: name.to_string(),
            engine,
            module,
            schema: None,
        }
    }

    pub fn schema(&self) -> Option<&MiddlewareConfigSchema> {
        self.schema.as_ref()
    }

    #[allow(dead_code)]
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    #[allow(dead_code)]
    pub fn module(&self) -> &Module {
        &self.module
    }

    pub fn create_session(&self) -> anyhow::Result<WasmProtocolSession> {
        let mut session = WasmProtocolSession::new(&self.engine, &self.module)?;
        if let Some(config) = get_dynamic_middleware_config(&self.name) {
            let _ = session.apply_config_map(&config);
        }
        Ok(session)
    }

    pub fn create_shared_session(&self) -> anyhow::Result<SessionHandle> {
        let session = self.create_session()?;
        let handle = Arc::new(Mutex::new(session));
        register_active_session(&self.name, &handle);
        Ok(handle)
    }

    fn apply_impl(
        &self,
        prelude: &[u8],
        ctx: &MiddlewareCtx,
    ) -> Result<MiddlewareOutput, MiddlewareError> {
        let mut session = self
            .create_session()
            .map_err(|e| MiddlewareError::Fatal(e.to_string()))?;

        if ctx.phase == MiddlewarePhase::Parse {
            match session.poll(prelude)? {
                PollResult::Handshake(HandshakeResult::NeedMoreData) => {
                    return Err(MiddlewareError::NeedMoreData);
                }
                PollResult::Handshake(HandshakeResult::NoMatch) => {
                    return Err(MiddlewareError::NoMatch);
                }
                PollResult::Handshake(HandshakeResult::RouteMatch { host, rewrite }) => {
                    if host.is_none() && rewrite.is_none() {
                        return Err(MiddlewareError::NoMatch);
                    }
                    return Ok(MiddlewareOutput { host, rewrite });
                }
                PollResult::Stream(_) => {
                    return Err(MiddlewareError::Fatal(
                        "unexpected stream result in parse phase".into(),
                    ));
                }
            }
        }

        if ctx.phase == MiddlewarePhase::Rewrite {
            return Ok(MiddlewareOutput::default());
        }

        Err(MiddlewareError::Fatal(format!(
            "wasm middleware {} missing required 'poll' export",
            self.path_hint
        )))
    }
}

impl Middleware for WasmMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply(
        &self,
        prelude: &[u8],
        ctx: &MiddlewareCtx,
    ) -> Result<MiddlewareOutput, MiddlewareError> {
        self.apply_impl(prelude, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::legacy_varint_framing::*;
    use super::*;
    use std::fs;

    fn temp_test_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        p.push(format!(
            "prism_mw_test_{name}_{}_{}",
            std::process::id(),
            now
        ));
        fs::create_dir_all(&p).expect("mkdir");
        p
    }

    // Minimal middleware: if state==0, always return host="x" and rewrite="abc".
    const TEST_WAT: &str = r#"(module
    (memory (export "memory") 2)

    (func $pack_result (param $action i32) (param $val i32) (result i64)
      (i64.or
        (i64.shl (i64.extend_i32_u (local.get $action)) (i64.const 32))
        (i64.extend_i32_u (local.get $val))
      )
    )

    (func (export "poll") (param $buf_ptr i32) (param $buf_len i32) (param $state i32) (result i64)
      (if (i32.eq (local.get $state) (i32.const 0))
        (then
          ;; host at 100
          (i32.store (i32.const 100) (i32.const 0x78)) ;; 'x'
          ;; rewrite at 200: 'a''b''c'
          (i32.store8 (i32.const 200) (i32.const 0x61))
          (i32.store8 (i32.const 201) (i32.const 0x62))
          (i32.store8 (i32.const 202) (i32.const 0x63))

          (i32.store (i32.const 65536) (i32.const 100))
          (i32.store (i32.const 65540) (i32.const 1))
          (i32.store (i32.const 65544) (i32.const 200))
          (i32.store (i32.const 65548) (i32.const 3))
          (return (call $pack_result (i32.const 1) (i32.const 65536)))
        )
      )

      (call $pack_result (i32.const 1) (local.get $buf_len))
    )
  )"#;

    #[test]
    fn wasm_middleware_returns_host_and_rewrite() {
        let dir = temp_test_dir("basic");
        let wat_path = dir.join("t.wat");
        fs::write(&wat_path, TEST_WAT).expect("write");

        let m = WasmMiddleware::from_wat_path("t", &wat_path).expect("load");
        let out = m.apply(b"zzz", &MiddlewareCtx::parse()).expect("apply");
        assert_eq!(out.host.as_deref(), Some("x"));
        assert_eq!(out.rewrite.as_deref(), Some(b"abc".as_slice()));

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn repo_sample_middlewares_compile() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let dir = root.join("middlewares");

        for name in ["tls_sni", "minecraft"] {
            let wat_path = dir.join(format!("{name}.wat"));
            assert!(
                wat_path.exists(),
                "expected repo sample middleware at {}, but it does not exist",
                wat_path.display()
            );
            WasmMiddleware::from_wat_path(name, &wat_path)
                .unwrap_or_else(|e| panic!("failed to compile {name}.wat: {e:#}"));
        }
    }

    #[test]
    fn materialize_default_middlewares_is_idempotent_and_non_destructive() {
        let dir = temp_test_dir("materialize_defaults");

        // First run should create the default files.
        let created = materialize_default_middlewares(&dir).expect("materialize");
        assert!(!created.is_empty(), "expected some files to be created");

        // Second run should not create anything.
        let created2 = materialize_default_middlewares(&dir).expect("materialize 2");
        assert!(created2.is_empty(), "expected no new files on second run");

        // Ensure we do not overwrite user-edited content.
        let custom = dir.join("tls_sni.wat");
        fs::write(&custom, "(module)\n").expect("write custom");
        let _ = materialize_default_middlewares(&dir).expect("materialize 3");
        let now = fs::read_to_string(&custom).expect("read custom");
        assert_eq!(now, "(module)\n");

        let _ = fs::remove_dir_all(&dir);
    }

    fn push_varint(mut v: u32, out: &mut Vec<u8>) {
        loop {
            let mut b = (v & 0x7f) as u8;
            v >>= 7;
            if v != 0 {
                b |= 0x80;
            }
            out.push(b);
            if v == 0 {
                break;
            }
        }
    }

    fn mc_handshake_prelude(host: &str, port: u16) -> Vec<u8> {
        mc_handshake_prelude_proto(host, port, 47, 1)
    }

    fn mc_handshake_prelude_proto(host: &str, port: u16, protocol: u32, next_state: u32) -> Vec<u8> {
        let mut pkt = Vec::new();
        push_varint(0, &mut pkt); // packet id
        push_varint(protocol, &mut pkt);
        push_varint(host.len() as u32, &mut pkt);
        pkt.extend_from_slice(host.as_bytes());
        pkt.extend_from_slice(&port.to_be_bytes());
        push_varint(next_state, &mut pkt);

        let mut out = Vec::new();
        push_varint(pkt.len() as u32, &mut out);
        out.extend_from_slice(&pkt);
        out
    }

    fn mc_handshake_prelude_with_extra(host: &str, extra: &[u8], port: u16) -> Vec<u8> {
        let mut addr = Vec::new();
        addr.extend_from_slice(host.as_bytes());
        addr.extend_from_slice(extra);

        let mut pkt = Vec::new();
        push_varint(0, &mut pkt); // packet id
        push_varint(47, &mut pkt);
        push_varint(addr.len() as u32, &mut pkt);
        pkt.extend_from_slice(&addr);
        pkt.extend_from_slice(&port.to_be_bytes());
        push_varint(1, &mut pkt);

        let mut out = Vec::new();
        push_varint(pkt.len() as u32, &mut out);
        out.extend_from_slice(&pkt);
        out
    }

    fn mc_status_request_packet() -> Vec<u8> {
        vec![0x01, 0x00]
    }

    fn mc_ping_packet(payload: i64) -> Vec<u8> {
        let mut pkt = Vec::new();
        push_varint(1, &mut pkt); // packet id
        pkt.extend_from_slice(&payload.to_be_bytes());

        let mut out = Vec::new();
        push_varint(pkt.len() as u32, &mut out);
        out.extend_from_slice(&pkt);
        out
    }

    fn tls_client_hello_prelude(host: &str) -> Vec<u8> {
        // Minimal TLS record containing a single ClientHello with a single SNI hostname.
        let host_bytes = host.as_bytes();
        let name_len = host_bytes.len();

        let sni_list_len = 1 + 2 + name_len; // name_type + name_len + name
        let sni_ext_data_len = 2 + sni_list_len; // list_len + list
        let sni_ext_len = sni_ext_data_len;
        let ext_total = 4 + sni_ext_len; // ext_type + ext_len + ext_data

        let client_hello_len = 2 + 32 + // legacy_version + random
            1 + // session id len (empty)
            2 + 2 + // cipher_suites len + one suite
            1 + 1 + // compression methods
            2 + ext_total; // extensions total + extensions

        let handshake_len = client_hello_len;
        let record_len = 4 + handshake_len; // handshake header + body

        let mut out = Vec::with_capacity(5 + record_len);

        // TLS record header
        out.push(22); // handshake
        out.push(0x03);
        out.push(0x01);
        out.extend_from_slice(&(record_len as u16).to_be_bytes());

        // Handshake header
        out.push(1); // ClientHello
        out.push(((handshake_len >> 16) & 0xff) as u8);
        out.push(((handshake_len >> 8) & 0xff) as u8);
        out.push((handshake_len & 0xff) as u8);

        // ClientHello body
        out.extend_from_slice(&[0x03, 0x03]); // legacy_version
        out.extend_from_slice(&[0u8; 32]); // random

        out.push(0); // session id len

        out.extend_from_slice(&(2u16).to_be_bytes());
        out.extend_from_slice(&[0x00, 0x2f]); // TLS_RSA_WITH_AES_128_CBC_SHA

        out.push(1); // compression methods len
        out.push(0); // null compression

        out.extend_from_slice(&(ext_total as u16).to_be_bytes());

        // SNI extension
        out.extend_from_slice(&(0u16).to_be_bytes());
        out.extend_from_slice(&(sni_ext_len as u16).to_be_bytes());
        out.extend_from_slice(&(sni_list_len as u16).to_be_bytes());
        out.push(0); // host_name
        out.extend_from_slice(&(name_len as u16).to_be_bytes());
        out.extend_from_slice(host_bytes);

        out
    }

    #[test]
    fn minecraft_extracts_host() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let dir = root.join("middlewares");

        let mc = WasmMiddleware::from_wat_path("minecraft", &dir.join("minecraft.wat"))
            .expect("compile minecraft");

        let prelude = mc_handshake_prelude("play.example.com", 25565);

        let out = mc
            .apply(&prelude, &MiddlewareCtx::parse())
            .expect("parse")
            .host
            .expect("host");

        assert_eq!(out, "play.example.com");
    }

    #[test]
    fn minecraft_extracts_host_with_trailing_status_bytes() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let dir = root.join("middlewares");

        let mc = WasmMiddleware::from_wat_path("minecraft", &dir.join("minecraft.wat"))
            .expect("compile minecraft");

        let handshake = mc_handshake_prelude("play.example.com", 25565);
        let mut suffix = mc_status_request_packet();
        suffix.extend_from_slice(&mc_ping_packet(42));

        let mut prelude = handshake;
        prelude.extend_from_slice(&suffix);

        let out = mc
            .apply(&prelude, &MiddlewareCtx::parse())
            .expect("parse")
            .host
            .expect("host");

        assert_eq!(out, "play.example.com");
    }

    #[test]
    fn minecraft_extracts_host_with_nul_delimited_address_suffix() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let dir = root.join("middlewares");

        let mc = WasmMiddleware::from_wat_path("minecraft", &dir.join("minecraft.wat"))
            .expect("compile minecraft");

        let extra = b"\0FML3\0modded-marker";
        let handshake = mc_handshake_prelude_with_extra("play.example.com", extra, 25565);
        let suffix = mc_status_request_packet();

        let mut prelude = handshake;
        prelude.extend_from_slice(&suffix);

        let out = mc
            .apply(&prelude, &MiddlewareCtx::parse())
            .expect("parse")
            .host
            .expect("host");

        assert_eq!(out, "play.example.com");
    }

    #[test]
    fn tls_sni_handshake_and_streaming() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let dir = root.join("middlewares");

        let tls = WasmMiddleware::from_wat_path("tls_sni", &dir.join("tls_sni.wat"))
            .expect("compile tls_sni");

        let prelude = tls_client_hello_prelude("orig.example.com");

        let parsed = tls
            .apply(&prelude, &MiddlewareCtx::parse())
            .expect("parse")
            .host
            .expect("host");
        assert_eq!(parsed, "orig.example.com");

        let mut session = tls.create_session().expect("create session");
        assert_eq!(session.state(), SessionState::Handshake);

        // State 0: Handshake
        let res = session.poll(&prelude).expect("poll handshake");
        assert_eq!(
            res,
            PollResult::Handshake(HandshakeResult::RouteMatch {
                host: Some("orig.example.com".to_string()),
                rewrite: None,
            })
        );
        assert_eq!(session.state(), SessionState::Streaming);

        // State 1: Streaming - partial record (< 5 bytes)
        let incomplete = &prelude[..3];
        let res_inc = session.poll(incomplete).expect("poll stream incomplete");
        assert_eq!(res_inc, PollResult::Stream(StreamResult::NeedMoreData));

        // State 1: Streaming - full record
        let res_full = session.poll(&prelude).expect("poll stream full");
        assert_eq!(
            res_full,
            PollResult::Stream(StreamResult::Frame {
                len: prelude.len(),
                priority: FramePriority::Defer,
                payload: None,
                incompressible: false,
            })
        );

        // State 2: Streaming Egress - full record framing
        session.set_state(SessionState::StreamingEgress);
        let res_egress = session.poll(&prelude).expect("poll egress full");
        assert_eq!(
            res_egress,
            PollResult::Stream(StreamResult::Frame {
                len: prelude.len(),
                priority: FramePriority::Defer,
                payload: None,
                incompressible: false,
            })
        );
    }

    #[test]
    fn host_crypto_rsa_decrypt_direct_and_wasm() {
        use rsa::pkcs1::EncodeRsaPrivateKey;
        use rsa::pkcs8::EncodePrivateKey;

        let key = rsa::RsaPrivateKey::new(&mut rsa::rand_core::OsRng, 1024).expect("generate rsa");
        let pkcs1_der = key.to_pkcs1_der().expect("encode pkcs1");
        let pkcs8_der = key.to_pkcs8_der().expect("encode pkcs8");

        let plaintext = b"Hello RSA PKCS#1 v1.5 from Prism 2026!";
        let ciphertext = key
            .to_public_key()
            .encrypt(&mut rsa::rand_core::OsRng, rsa::Pkcs1v15Encrypt, plaintext)
            .expect("encrypt");

        // 1. Direct Rust tests:
        let dec1 = crypto_rsa_decrypt(pkcs1_der.as_bytes(), &ciphertext).expect("decrypt pkcs1");
        assert_eq!(&dec1, plaintext);

        let dec8 = crypto_rsa_decrypt(pkcs8_der.as_bytes(), &ciphertext).expect("decrypt pkcs8");
        assert_eq!(&dec8, plaintext);

        // Errors: invalid key (-1), bad ciphertext (-2)
        assert_eq!(crypto_rsa_decrypt(b"not-a-valid-key", &ciphertext), Err(-1));
        assert_eq!(
            crypto_rsa_decrypt(pkcs1_der.as_bytes(), b"invalid-cipher"),
            Err(-2)
        );

        // 2. Host function execution via WASM session:
        let test_wat = r#"(module
          (import "prism" "crypto_rsa_decrypt"
            (func $crypto_rsa_decrypt (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 2)
          (func (export "test_decrypt") (param $kp i32) (param $kl i32) (param $ip i32) (param $il i32) (param $op i32) (result i32)
            (call $crypto_rsa_decrypt (local.get $kp) (local.get $kl) (local.get $ip) (local.get $il) (local.get $op))
          )
        )"#;

        let engine = Engine::default();
        let module = Module::new(&engine, test_wat).expect("compile wat");
        let mut session = WasmProtocolSession::new(&engine, &module).expect("session");

        let key_bytes = pkcs1_der.as_bytes();
        let key_offset = 1000usize;
        let in_offset = 3000usize;
        let out_offset = 5000usize;

        session.write_memory(key_offset, key_bytes).unwrap();
        session.write_memory(in_offset, &ciphertext).unwrap();

        let test_fn: TypedFunc<(i32, i32, i32, i32, i32), i32> =
            session.get_typed_func("test_decrypt").unwrap();

        let written = test_fn
            .call(
                session.store_mut(),
                (
                    key_offset as i32,
                    key_bytes.len() as i32,
                    in_offset as i32,
                    ciphertext.len() as i32,
                    out_offset as i32,
                ),
            )
            .expect("call test_decrypt");

        assert_eq!(written, plaintext.len() as i32);

        let mut read_buf = vec![0u8; plaintext.len()];
        session.read_memory(out_offset, &mut read_buf).unwrap();
        assert_eq!(&read_buf, plaintext);
    }

    #[test]
    fn host_crypto_aes_cfb8_direct_and_wasm() {
        // NIST SP 800-38A test vector
        let key = [
            0x2b, 0x7e, 0x15, 0x16, 0x28, 0xae, 0xd2, 0xa6, 0xab, 0xf7, 0x15, 0x88, 0x09, 0xcf,
            0x4f, 0x3c,
        ];
        let iv = [
            0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08, 0x09, 0x0a, 0x0b, 0x0c, 0x0d,
            0x0e, 0x0f,
        ];
        let plaintext = [
            0x6b, 0xc1, 0xbe, 0xe2, 0x2e, 0x40, 0x9f, 0x96, 0xe9, 0x3d, 0x7e, 0x11, 0x73, 0x93,
            0x17, 0x2a,
        ];
        let expected_ciphertext = [
            0x3b, 0x79, 0x42, 0x4c, 0x9c, 0x0d, 0xd4, 0x36, 0xba, 0xce, 0x9e, 0x0e, 0xd4, 0x58,
            0x6a, 0x4f,
        ];

        // 1. Direct Rust encryption:
        let mut buf = plaintext;
        let mut cur_iv = iv;
        crypto_aes_cfb8(&key, &mut cur_iv, &mut buf, true).expect("encrypt");
        assert_eq!(buf, expected_ciphertext);
        assert_ne!(cur_iv, iv, "IV should be updated with feedback");

        // 2. Direct Rust decryption:
        let mut dec_iv = iv;
        crypto_aes_cfb8(&key, &mut dec_iv, &mut buf, false).expect("decrypt");
        assert_eq!(buf, plaintext);
        assert_eq!(
            dec_iv, cur_iv,
            "decryption IV update should match encryption IV update"
        );

        // 3. Host function execution via WASM session:
        let test_wat = r#"(module
          (import "prism" "crypto_aes_cfb8"
            (func $crypto_aes_cfb8 (param i32 i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 2)
          (func (export "test_aes") (param $kp i32) (param $ivp i32) (param $dp i32) (param $dl i32) (param $enc i32) (result i32)
            (call $crypto_aes_cfb8 (local.get $kp) (local.get $ivp) (local.get $dp) (local.get $dl) (local.get $enc))
          )
        )"#;

        let engine = Engine::default();
        let module = Module::new(&engine, test_wat).expect("compile wat");
        let mut session = WasmProtocolSession::new(&engine, &module).expect("session");

        let key_offset = 100usize;
        let iv_offset = 200usize;
        let data_offset = 300usize;

        session.write_memory(key_offset, &key).unwrap();
        session.write_memory(iv_offset, &iv).unwrap();
        session.write_memory(data_offset, &plaintext).unwrap();

        let test_fn: TypedFunc<(i32, i32, i32, i32, i32), i32> =
            session.get_typed_func("test_aes").unwrap();

        // Encrypt in WASM
        let res = test_fn
            .call(
                session.store_mut(),
                (
                    key_offset as i32,
                    iv_offset as i32,
                    data_offset as i32,
                    plaintext.len() as i32,
                    1,
                ),
            )
            .expect("call test_aes encrypt");
        assert_eq!(res, 0);

        let mut read_cipher = [0u8; 16];
        session.read_memory(data_offset, &mut read_cipher).unwrap();
        assert_eq!(read_cipher, expected_ciphertext);

        // Reset IV in WASM memory and decrypt in WASM
        session.write_memory(iv_offset, &iv).unwrap();
        let res2 = test_fn
            .call(
                session.store_mut(),
                (
                    key_offset as i32,
                    iv_offset as i32,
                    data_offset as i32,
                    plaintext.len() as i32,
                    0,
                ),
            )
            .expect("call test_aes decrypt");
        assert_eq!(res2, 0);

        let mut read_plain = [0u8; 16];
        session.read_memory(data_offset, &mut read_plain).unwrap();
        assert_eq!(read_plain, plaintext);
    }

    #[test]
    fn host_deflate_compress_decompress_direct_and_wasm() {
        let payload =
            b"Prism unified WASM protocol driver traffic optimizer compression test bytes!";

        // 1. Direct Rust compress and decompress:
        let compressed = deflate_compress(payload, 6).expect("compress");
        let decompressed = deflate_decompress(&compressed).expect("decompress");
        assert_eq!(&decompressed, payload);

        // 2. Direct raw deflate fallback:
        let raw_deflate = miniz_oxide::deflate::compress_to_vec(payload, 6);
        let raw_decompressed = deflate_decompress(&raw_deflate).expect("decompress raw");
        assert_eq!(&raw_decompressed, payload);

        // 3. Host function execution via WASM session:
        let test_wat = r#"(module
          (import "prism" "deflate_compress"
            (func $deflate_compress (param i32 i32 i32 i32 i32) (result i32)))
          (import "prism" "deflate_decompress"
            (func $deflate_decompress (param i32 i32 i32 i32) (result i32)))
          (memory (export "memory") 4)
          (func (export "test_compress") (param $ip i32) (param $il i32) (param $op i32) (param $omax i32) (param $lvl i32) (result i32)
            (call $deflate_compress (local.get $ip) (local.get $il) (local.get $op) (local.get $omax) (local.get $lvl))
          )
          (func (export "test_decompress") (param $ip i32) (param $il i32) (param $op i32) (param $omax i32) (result i32)
            (call $deflate_decompress (local.get $ip) (local.get $il) (local.get $op) (local.get $omax))
          )
        )"#;

        let engine = Engine::default();
        let module = Module::new(&engine, test_wat).expect("compile wat");
        let mut session = WasmProtocolSession::new(&engine, &module).expect("session");

        let in_offset = 1000usize;
        let comp_offset = 3000usize;
        let decomp_offset = 6000usize;

        session.write_memory(in_offset, payload).unwrap();

        let comp_fn: TypedFunc<(i32, i32, i32, i32, i32), i32> =
            session.get_typed_func("test_compress").unwrap();

        let decomp_fn: TypedFunc<(i32, i32, i32, i32), i32> =
            session.get_typed_func("test_decompress").unwrap();

        let comp_len = comp_fn
            .call(
                session.store_mut(),
                (
                    in_offset as i32,
                    payload.len() as i32,
                    comp_offset as i32,
                    1000,
                    6,
                ),
            )
            .expect("call compress");
        assert!(comp_len > 0);

        let decomp_len = decomp_fn
            .call(
                session.store_mut(),
                (comp_offset as i32, comp_len, decomp_offset as i32, 1000),
            )
            .expect("call decompress");
        assert_eq!(decomp_len, payload.len() as i32);

        let mut read_decomp = vec![0u8; payload.len()];
        session
            .read_memory(decomp_offset, &mut read_decomp)
            .unwrap();
        assert_eq!(&read_decomp, payload);
    }

    #[test]
    fn wasm_protocol_session_lifecycle_poll_and_set_data() {
        let test_wat = r#"(module
          (import "prism" "crypto_rsa_decrypt"
            (func $crypto_rsa_decrypt (param i32 i32 i32 i32 i32) (result i32)))
          (import "prism" "crypto_aes_cfb8"
            (func $crypto_aes_cfb8 (param i32 i32 i32 i32 i32) (result i32)))
          (import "prism" "deflate_decompress"
            (func $deflate_decompress (param i32 i32 i32 i32) (result i32)))
          (import "prism" "deflate_compress"
            (func $deflate_compress (param i32 i32 i32 i32 i32) (result i32)))

          (memory (export "memory") 4)

          (global $stored_data_len (mut i32) (i32.const 0))

          (func (export "set_data") (param $ptr i32) (param $len i32) (result i32)
            (global.set $stored_data_len (local.get $len))
            (i32.const 42)
          )

          (func $pack (param $action i32) (param $value i32) (result i64)
            (i64.or
              (i64.shl (i64.extend_i32_u (local.get $action)) (i64.const 32))
              (i64.extend_i32_u (local.get $value))
            )
          )

          (func (export "poll") (param $buf_ptr i32) (param $buf_len i32) (param $state i32) (result i64)
            ;; State 0: Handshake
            (if (i32.eq (local.get $state) (i32.const 0))
              (then
                ;; If len < 5, NeedMoreData (action 0)
                (if (i32.lt_s (local.get $buf_len) (i32.const 5))
                  (then (return (call $pack (i32.const 0) (i32.const 0))))
                )
                ;; If first byte is 0xFF, NoMatch (action 2)
                (if (i32.eq (i32.load8_u (local.get $buf_ptr)) (i32.const 0xFF))
                  (then (return (call $pack (i32.const 2) (i32.const 0))))
                )
                ;; RouteMatch (action 1):
                ;; Write host "game.prism.io" at offset 10000
                (i32.store8 (i32.const 10000) (i32.const 103)) ;; 'g'
                (i32.store8 (i32.const 10001) (i32.const 97))  ;; 'a'
                (i32.store8 (i32.const 10002) (i32.const 109)) ;; 'm'
                (i32.store8 (i32.const 10003) (i32.const 101)) ;; 'e'
                (i32.store8 (i32.const 10004) (i32.const 46))  ;; '.'
                (i32.store8 (i32.const 10005) (i32.const 112)) ;; 'p'
                (i32.store8 (i32.const 10006) (i32.const 114)) ;; 'r'
                (i32.store8 (i32.const 10007) (i32.const 105)) ;; 'i'
                (i32.store8 (i32.const 10008) (i32.const 115)) ;; 's'
                (i32.store8 (i32.const 10009) (i32.const 109)) ;; 'm'
                (i32.store8 (i32.const 10010) (i32.const 46))  ;; '.'
                (i32.store8 (i32.const 10011) (i32.const 105)) ;; 'i'
                (i32.store8 (i32.const 10012) (i32.const 111)) ;; 'o'

                ;; Write rewrite bytes "RW" at offset 12000
                (i32.store8 (i32.const 12000) (i32.const 82)) ;; 'R'
                (i32.store8 (i32.const 12001) (i32.const 87)) ;; 'W'

                ;; Write struct at offset 20000: host_ptr=10000, host_len=13, rw_ptr=12000, rw_len=2
                (i32.store (i32.const 20000) (i32.const 10000))
                (i32.store (i32.const 20004) (i32.const 13))
                (i32.store (i32.const 20008) (i32.const 12000))
                (i32.store (i32.const 20012) (i32.const 2))

                (return (call $pack (i32.const 1) (i32.const 20000)))
              )
            )

            ;; State 1: Streaming
            (if (i32.eq (local.get $state) (i32.const 1))
              (then
                ;; If len < 3, NeedMoreData (action 0)
                (if (i32.lt_s (local.get $buf_len) (i32.const 3))
                  (then (return (call $pack (i32.const 0) (i32.const 0))))
                )
                ;; If first byte is 0x01, FrameUrgent (action 2) with packet total len 16
                (if (i32.eq (i32.load8_u (local.get $buf_ptr)) (i32.const 1))
                  (then (return (call $pack (i32.const 2) (i32.const 16))))
                )
                ;; Else FrameDefer (action 1) with packet total len 32
                (return (call $pack (i32.const 1) (i32.const 32)))
              )
            )

            (call $pack (i32.const 0) (i32.const 0))
          )
        )"#;

        let mut session = WasmProtocolSession::from_wat(test_wat).expect("create session");
        assert_eq!(session.state(), SessionState::Handshake);
        assert!(session.has_poll());
        assert!(session.has_set_data());

        // Test set_data
        let res = session.set_data(b"secret_key_12345").unwrap();
        assert_eq!(res, 42);

        // Handshake: NeedMoreData
        match session.poll(b"123").unwrap() {
            PollResult::Handshake(HandshakeResult::NeedMoreData) => {}
            other => panic!("expected NeedMoreData, got {other:?}"),
        }
        assert_eq!(session.state(), SessionState::Handshake);

        // Handshake: NoMatch
        match session.poll(&[0xFF, 1, 2, 3, 4, 5]).unwrap() {
            PollResult::Handshake(HandshakeResult::NoMatch) => {}
            other => panic!("expected NoMatch, got {other:?}"),
        }
        assert_eq!(session.state(), SessionState::Handshake);

        // Handshake: RouteMatch
        match session.poll(b"valid_handshake").unwrap() {
            PollResult::Handshake(HandshakeResult::RouteMatch { host, rewrite }) => {
                assert_eq!(host.as_deref(), Some("game.prism.io"));
                assert_eq!(rewrite.as_deref(), Some(b"RW".as_slice()));
            }
            other => panic!("expected RouteMatch, got {other:?}"),
        }
        // Verify state automatically transitioned to Streaming
        assert_eq!(session.state(), SessionState::Streaming);

        // Streaming: NeedMoreData
        match session.poll(b"12").unwrap() {
            PollResult::Stream(StreamResult::NeedMoreData) => {}
            other => panic!("expected NeedMoreData, got {other:?}"),
        }

        // Streaming: FrameUrgent
        match session.poll(&[0x01, 0xAA, 0xBB, 0xCC]).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                assert_eq!(len, 16);
                assert_eq!(priority, FramePriority::Urgent);
            }
            other => panic!("expected FrameUrgent, got {other:?}"),
        }

        // Streaming: FrameDefer
        match session.poll(&[0x02, 0xAA, 0xBB, 0xCC]).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                assert_eq!(len, 32);
                assert_eq!(priority, FramePriority::Defer);
            }
            other => panic!("expected FrameDefer, got {other:?}"),
        }
    }

    #[test]
    fn minecraft_unified_wasm_driver_poll_and_set_data() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let wat_path = root.join("middlewares").join("minecraft.wat");
        assert!(wat_path.exists(), "minecraft.wat does not exist");

        let wat_bytes = fs::read(&wat_path).expect("read minecraft.wat");
        let mut session = WasmProtocolSession::from_wat(&wat_bytes).expect("session");
        // 1. Test set_data
        let test_key = b"RSA_PRIVATE_KEY_MOCK_DATA_1234567890";
        // Write test key at offset 1000
        session.write_memory(1000, test_key).unwrap();
        let res = session.set_data(test_key).unwrap();
        assert_eq!(res, 0);

        // Verify stored at 196608 and length at 196604
        let mut len_bytes = [0u8; 4];
        session.read_memory(196604, &mut len_bytes).unwrap();
        assert_eq!(u32::from_le_bytes(len_bytes), test_key.len() as u32);

        let mut read_key = vec![0u8; test_key.len()];
        session.read_memory(196608, &mut read_key).unwrap();
        assert_eq!(&read_key, test_key);

        // 2. Test poll state == 0 (Handshaking)
        let handshake = mc_handshake_prelude("play.example.com", 25565);

        // Partial buffer -> Action 0 (NEED_MORE_DATA)
        let partial_hs = &handshake[..handshake.len() - 5];
        match session.poll(partial_hs).unwrap() {
            PollResult::Handshake(HandshakeResult::NeedMoreData) => {}
            other => panic!("expected NeedMoreData, got {other:?}"),
        }

        // Complete handshake -> Action 1 (ROUTE_MATCH)
        match session.poll(&handshake).unwrap() {
            PollResult::Handshake(HandshakeResult::RouteMatch { host, rewrite: _ }) => {
                assert_eq!(host.as_deref(), Some("play.example.com"));
            }
            other => panic!("expected RouteMatch, got {other:?}"),
        }

        // State was automatically transitioned to Streaming; reset to Handshake for further handshake tests
        session.set_state(SessionState::Handshake);

        // Handshake with NUL-delimited extra data
        let extra = b"\0FML3\0modded-marker";
        let handshake_extra = mc_handshake_prelude_with_extra("mc.server.net", extra, 25565);
        match session.poll(&handshake_extra).unwrap() {
            PollResult::Handshake(HandshakeResult::RouteMatch { host, rewrite: _ }) => {
                assert_eq!(host.as_deref(), Some("mc.server.net"));
            }
            other => panic!("expected RouteMatch for handshake with extra, got {other:?}"),
        }

        // Invalid packet ID (e.g. 0x01 instead of 0x00) -> Action 2 (NO_MATCH)
        session.set_state(SessionState::Handshake);
        let mut bad_handshake = handshake.clone();
        bad_handshake[1] = 0x01;
        match session.poll(&bad_handshake).unwrap() {
            PollResult::Handshake(HandshakeResult::NoMatch) => {}
            other => panic!("expected NoMatch for bad handshake ID, got {other:?}"),
        }

        // 3. Test poll state == 1 (Streaming)
        session.set_state(SessionState::Streaming);
        session.set_conn_state(1);
        session.set_flow_direction(false);

        // Partial streaming packet -> Action 0 (NEED_MORE_DATA)
        let ping_pkt = mc_ping_packet(12345); // Status ping packet ID is 0x01 (urgent)
        let partial_ping = &ping_pkt[..ping_pkt.len() - 2];
        match session.poll(partial_ping).unwrap() {
            PollResult::Stream(StreamResult::NeedMoreData) => {}
            other => panic!("expected NeedMoreData for partial streaming, got {other:?}"),
        }

        // Urgent Ping packet -> Action 2 (FRAME_URGENT), Value = total packet bytes
        match session.poll(&ping_pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                assert_eq!(priority, FramePriority::Urgent);
                assert_eq!(len, ping_pkt.len());
            }
            other => panic!("expected FrameUrgent for Ping packet, got {other:?}"),
        }

        // Play-state keepalives: only the current state+direction ids are urgent.
        session.set_conn_state(4);
        session.set_flow_direction(true);
        for keepalive_id in [0x1F, 0x21] {
            let mut kp = Vec::new();
            push_varint(keepalive_id, &mut kp);
            kp.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
            let mut pkt = Vec::new();
            push_varint(kp.len() as u32, &mut pkt);
            pkt.extend_from_slice(&kp);

            match session.poll(&pkt).unwrap() {
                PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                    assert_eq!(
                        priority,
                        FramePriority::Urgent,
                        "expected Urgent for KeepAlive 0x{:02X}",
                        keepalive_id
                    );
                    assert_eq!(len, pkt.len());
                }
                other => panic!("expected FrameUrgent, got {other:?}"),
            }
        }
        session.set_flow_direction(false);
        for keepalive_id in [0x0F] {
            let mut kp = Vec::new();
            push_varint(keepalive_id, &mut kp);
            kp.extend_from_slice(&[0xaa, 0xbb, 0xcc, 0xdd]);
            let mut pkt = Vec::new();
            push_varint(kp.len() as u32, &mut pkt);
            pkt.extend_from_slice(&kp);
            match session.poll(&pkt).unwrap() {
                PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                    assert_eq!(priority, FramePriority::Urgent);
                }
                other => panic!("expected FrameUrgent, got {other:?}"),
            }
        }
        session.set_flow_direction(true);

        // Normal game packet: ID 0x27 (e.g. entity movement/block update) -> Action 1 (FRAME_DEFER)
        let mut normal = Vec::new();
        push_varint(0x27, &mut normal);
        normal.extend_from_slice(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]);
        let mut pkt = Vec::new();
        push_varint(normal.len() as u32, &mut pkt);
        pkt.extend_from_slice(&normal);

        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                assert_eq!(priority, FramePriority::Defer);
                assert_eq!(len, pkt.len());
            }
            other => panic!("expected FrameDefer for normal packet, got {other:?}"),
        }
    }

    #[test]
    fn test_dynamic_symbol_table_intern_and_resolve() {
        let mut table = DynamicSymbolTable::new(3);

        // 1. First insert: "minecraft:chat"
        let res1 = table.intern(b"minecraft:chat");
        assert_eq!((res1 >> 32) as i32, 1); // 1 = newly added (0x40)
        assert_eq!((res1 & 0xffff_ffff) as i32, 1); // index = 1

        // 2. Query existing: "minecraft:chat"
        let res2 = table.intern(b"minecraft:chat");
        assert_eq!((res2 >> 32) as i32, 0); // 0 = already exists (0x80)
        assert_eq!((res2 & 0xffff_ffff) as i32, 1); // index = 1

        // 3. Second insert: "minecraft:chunk"
        let res3 = table.intern(b"minecraft:chunk");
        assert_eq!((res3 >> 32) as i32, 1); // newly added
        assert_eq!((res3 & 0xffff_ffff) as i32, 1); // newly added at index 1

        // Now:
        // index 1 -> "minecraft:chunk"
        // index 2 -> "minecraft:chat"
        assert_eq!(table.resolve(1), Some(b"minecraft:chunk".as_slice()));
        assert_eq!(table.resolve(2), Some(b"minecraft:chat".as_slice()));

        // 4. Query "minecraft:chat" again -> index should now be 2
        let res4 = table.intern(b"minecraft:chat");
        assert_eq!((res4 >> 32) as i32, 0); // already exists
        assert_eq!((res4 & 0xffff_ffff) as i32, 2);

        // 5. Insert 3rd: "minecraft:block"
        let res5 = table.intern(b"minecraft:block");
        assert_eq!((res5 >> 32) as i32, 1);
        assert_eq!(table.len(), 3);

        // 6. Insert 4th: "minecraft:sound" -> exceeds capacity (3), "minecraft:chat" evicted
        let res6 = table.intern(b"minecraft:sound");
        assert_eq!((res6 >> 32) as i32, 1);
        assert_eq!(table.len(), 3);

        // "minecraft:sound" is at index 1
        assert_eq!(table.resolve(1), Some(b"minecraft:sound".as_slice()));
        // Oldest ("minecraft:chat") is evicted
        assert_eq!(table.resolve(4), None);
        // Interning "minecraft:chat" now treats it as newly added
        let res7 = table.intern(b"minecraft:chat");
        assert_eq!((res7 >> 32) as i32, 1);
    }

    #[test]
    fn test_host_sym_intern_and_resolve_via_wasm() {
        let wat = r#"
            (module
                (import "prism" "sym_intern" (func $sym_intern (param i32 i32) (result i64)))
                (import "prism" "sym_resolve" (func $sym_resolve (param i32 i32 i32) (result i32)))
                (memory (export "memory") 1)

                ;; test_intern: writes string to memory at 0, calls $sym_intern
                (func (export "test_intern") (param $str_len i32) (result i64)
                    (call $sym_intern (i32.const 0) (local.get $str_len))
                )

                ;; test_resolve: calls $sym_resolve, writes string to memory at 100
                (func (export "test_resolve") (param $index i32) (result i32)
                    (call $sym_resolve (local.get $index) (i32.const 100) (i32.const 100))
                )
            )
        "#;

        let engine = Engine::default();
        let mut store = Store::new(&engine, HostEnv::default());
        let module = Module::new(&engine, wat).unwrap();
        let linker = create_prism_linker(&engine).unwrap();
        let instance = linker.instantiate(&mut store, &module).unwrap();
        let memory = instance.get_memory(&mut store, "memory").unwrap();

        let test_intern: TypedFunc<i32, i64> =
            instance.get_typed_func(&mut store, "test_intern").unwrap();
        let test_resolve: TypedFunc<i32, i32> =
            instance.get_typed_func(&mut store, "test_resolve").unwrap();

        // Write "minecraft:brand" to memory at offset 0
        let symbol = b"minecraft:brand";
        memory.write(&mut store, 0, symbol).unwrap();

        // Call test_intern -> should be newly added (1 << 32) | 1
        let res = test_intern.call(&mut store, symbol.len() as i32).unwrap();
        assert_eq!((res >> 32) as i32, 1);
        assert_eq!((res & 0xffff_ffff) as i32, 1);

        // Call test_resolve(1) -> writes symbol to memory at offset 100
        let written = test_resolve.call(&mut store, 1).unwrap();
        assert_eq!(written, symbol.len() as i32);

        let mut read_back = vec![0u8; symbol.len()];
        memory.read(&store, 100, &mut read_back).unwrap();
        assert_eq!(&read_back, symbol);

        // Call test_intern again -> should return existing (0 << 32) | 1
        let res2 = test_intern.call(&mut store, symbol.len() as i32).unwrap();
        assert_eq!((res2 >> 32) as i32, 0);
        assert_eq!((res2 & 0xffff_ffff) as i32, 1);
    }

    #[tokio::test]
    async fn test_minecraft_deflate_packet_decompress_and_recompress_roundtrip() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let wat_path = root.join("middlewares").join("minecraft.wat");
        let wat_bytes = fs::read(&wat_path).expect("read minecraft.wat");
        let mut session = WasmProtocolSession::from_wat(&wat_bytes).expect("session");
        session.set_state(SessionState::Streaming);

        // Construct raw Minecraft packet: ID 0x27 + 500 bytes of repetitive data (simulating chunk)
        let mut raw_packet = Vec::new();
        push_varint(0x27, &mut raw_packet);
        for i in 0..500 {
            raw_packet.push((i % 256) as u8);
        }

        // Compress with Deflate (threshold = 256)
        let compressed_packet = deflate_recompress_packet(&raw_packet, 256);

        // Feed to session.poll
        let poll_res = session.poll(&compressed_packet).expect("poll");
        match poll_res {
            PollResult::Stream(StreamResult::Frame {
                len,
                priority,
                payload,
                ..
            }) => {
                assert_eq!(
                    len,
                    compressed_packet.len(),
                    "consumed_len must equal incoming wire bytes"
                );
                assert_eq!(priority, FramePriority::Defer);
                let framed_packet = payload.expect("must return framed uncompressed payload");
                let expected_uncompressed = frame_uncompressed_packet(&raw_packet);
                assert_eq!(
                    &framed_packet, &expected_uncompressed,
                    "decompressed payload must be fully framed uncompressed packet"
                );
            }
            other => panic!("expected Frame with payload, got {other:?}"),
        }

        // Test recompress_packet_stream (legacy reference)
        let framed_uncompressed = frame_uncompressed_packet(&raw_packet);
        let mut pending = framed_uncompressed.clone();
        let mut out = Vec::new();
        recompress_packet_stream(&mut pending, &mut out, 256)
            .await
            .unwrap();
        assert!(pending.is_empty());
        // Verify output packet is compressed with Deflate
        let (pkt_len, len_n) = read_varint(&out).unwrap();
        let (data_len, dl_n) = read_varint(&out[len_n..]).unwrap();
        assert_eq!(data_len as usize, raw_packet.len());
        let decomp = deflate_decompress(&out[len_n + dl_n..len_n + pkt_len as usize]).unwrap();
        assert_eq!(&decomp, &raw_packet);

        // Test generic WASM egress streaming transformation (WAT State 2: Egress)
        let mut egress_pending = framed_uncompressed.clone();
        let mut egress_out = Vec::new();
        session
            .process_egress_stream(&mut egress_pending, &mut egress_out)
            .await
            .unwrap();
        assert!(egress_pending.is_empty());
        assert_eq!(
            &egress_out, &out,
            "WASM egress output must match recompressed packet"
        );
    }

    fn mc_status_response(json: &str) -> Vec<u8> {
        let mut pkt = Vec::new();
        push_varint(0, &mut pkt);
        push_varint(json.len() as u32, &mut pkt);
        pkt.extend_from_slice(json.as_bytes());
        let mut out = Vec::new();
        push_varint(pkt.len() as u32, &mut out);
        out.extend_from_slice(&pkt);
        out
    }

    #[tokio::test]
    async fn test_minecraft_status_response_not_recompressed_before_set_compression() {
        let wat = get_default_middleware_wat("minecraft").unwrap();
        let mut session = WasmProtocolSession::from_wat(wat).expect("session");
        session.set_state(SessionState::StreamingEgress);
        session.set_flow_direction(true);
        session.set_conn_state(1);

        let json = format!("{{\"description\":\"{}\"}}", "a".repeat(400));
        let pkt = mc_status_response(&json);
        assert!(
            pkt.len() > 256,
            "status JSON must exceed the recompress threshold"
        );

        let mut pending = pkt.clone();
        let mut out = Vec::new();
        session
            .process_egress_stream(&mut pending, &mut out)
            .await
            .unwrap();
        assert!(pending.is_empty());
        assert_eq!(
            out, pkt,
            "status response must not be zlib-framed before Set Compression"
        );
    }

    #[test]
    fn test_minecraft_streaming_handshake_makes_status_ping_urgent() {
        let wat = get_default_middleware_wat("minecraft").unwrap();
        let mut session = WasmProtocolSession::from_wat(wat).expect("session");
        session.set_state(SessionState::Streaming);
        session.set_flow_direction(false);

        let handshake = mc_handshake_prelude("play.example.com", 25565);
        match session.poll(&handshake).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                assert_eq!(len, handshake.len());
                assert_eq!(priority, FramePriority::Defer);
            }
            other => panic!("expected handshake as streaming frame, got {other:?}"),
        }

        let ping = mc_ping_packet(42);
        match session.poll(&ping).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, priority, .. }) => {
                assert_eq!(len, ping.len());
                assert_eq!(priority, FramePriority::Urgent);
            }
            other => panic!("expected status ping to be urgent after handshake, got {other:?}"),
        }
    }

    #[test]
    fn test_minecraft_play_priority_classes_proto_772() {
        let wat = get_default_middleware_wat("minecraft").unwrap();
        let mut session = WasmProtocolSession::from_wat(wat).expect("session");
        session.set_state(SessionState::Streaming);
        session.set_flow_direction(false);

        let hs = mc_handshake_prelude_proto("play.example.com", 25565, 772, 2);
        match session.poll(&hs).unwrap() {
            PollResult::Stream(StreamResult::Frame { .. }) => {}
            other => panic!("expected handshake frame, got {other:?}"),
        }
        session.set_conn_state(4);

        let mut ka = Vec::new();
        push_varint(0x1B, &mut ka);
        ka.extend_from_slice(&[0u8; 8]);
        let mut pkt = Vec::new();
        push_varint(ka.len() as u32, &mut pkt);
        pkt.extend_from_slice(&ka);
        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                assert_eq!(priority, FramePriority::Urgent);
            }
            other => panic!("expected keepalive urgent, got {other:?}"),
        }

        let mut move_pkt = Vec::new();
        push_varint(0x1D, &mut move_pkt);
        move_pkt.extend_from_slice(&[0u8; 24]);
        let mut pkt = Vec::new();
        push_varint(move_pkt.len() as u32, &mut pkt);
        pkt.extend_from_slice(&move_pkt);
        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                assert_eq!(priority, FramePriority::High);
            }
            other => panic!("expected move high, got {other:?}"),
        }

        session.set_flow_direction(true);
        let mut chunk = Vec::new();
        push_varint(0x27, &mut chunk);
        chunk.extend_from_slice(&[0u8; 32]);
        let mut pkt = Vec::new();
        push_varint(chunk.len() as u32, &mut pkt);
        pkt.extend_from_slice(&chunk);
        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                assert_eq!(priority, FramePriority::Bulk);
            }
            other => panic!("expected chunk bulk, got {other:?}"),
        }

        let mut ka = Vec::new();
        push_varint(0x26, &mut ka);
        ka.extend_from_slice(&[0u8; 8]);
        let mut pkt = Vec::new();
        push_varint(ka.len() as u32, &mut pkt);
        pkt.extend_from_slice(&ka);
        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                assert_eq!(priority, FramePriority::Urgent);
            }
            other => panic!("expected clientbound keepalive urgent, got {other:?}"),
        }

        let mut border = Vec::new();
        push_varint(0x25, &mut border);
        border.extend_from_slice(&[0u8; 8]);
        let mut pkt = Vec::new();
        push_varint(border.len() as u32, &mut pkt);
        pkt.extend_from_slice(&border);
        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                assert_eq!(
                    priority,
                    FramePriority::Defer,
                    "initialize_border (0x25) must not be classified as keepalive"
                );
            }
            other => panic!("expected initialize_border defer, got {other:?}"),
        }

        let mut mv = Vec::new();
        push_varint(0x2E, &mut mv);
        mv.extend_from_slice(&[0u8; 8]);
        let mut pkt = Vec::new();
        push_varint(mv.len() as u32, &mut pkt);
        pkt.extend_from_slice(&mv);
        match session.poll(&pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { priority, .. }) => {
                assert_eq!(priority, FramePriority::High);
            }
            other => panic!("expected move_entity_pos high, got {other:?}"),
        }
    }

    #[test]
    fn test_minecraft_custom_payload_identifier_intern_roundtrip() {
        let wat = get_default_middleware_wat("minecraft").unwrap();
        let mut ingress = WasmProtocolSession::from_wat(wat).expect("ingress");
        let mut egress = WasmProtocolSession::from_wat(wat).expect("egress");
        for sess in [&mut ingress, &mut egress] {
            sess.set_state(SessionState::Streaming);
            sess.set_flow_direction(true);
            let hs = mc_handshake_prelude_proto("play.example.com", 25565, 772, 2);
            match sess.poll(&hs).unwrap() {
                PollResult::Stream(StreamResult::Frame { .. }) => {}
                other => panic!("expected handshake frame, got {other:?}"),
            }
            sess.set_conn_state(4);
        }

        fn custom_payload(ident: &[u8], data: &[u8]) -> Vec<u8> {
            let mut body = Vec::new();
            push_varint(0x18, &mut body);
            push_varint(ident.len() as u32, &mut body);
            body.extend_from_slice(ident);
            body.extend_from_slice(data);
            let mut pkt = Vec::new();
            push_varint(body.len() as u32, &mut pkt);
            pkt.extend_from_slice(&body);
            pkt
        }

        let first = custom_payload(b"demo:channel", &[1, 2, 3]);
        match ingress.poll(&first).unwrap() {
            PollResult::Stream(StreamResult::Frame { payload, .. }) => {
                assert!(payload.is_none(), "first occurrence stays as identifier string");
            }
            other => panic!("expected first custom payload frame, got {other:?}"),
        }
        match egress.poll(&first).unwrap() {
            PollResult::Stream(StreamResult::Frame { .. }) => {}
            other => panic!("expected egress intern of first identifier, got {other:?}"),
        }

        let second = custom_payload(b"demo:channel", &[4, 5, 6]);
        let compact = match ingress.poll(&second).unwrap() {
            PollResult::Stream(StreamResult::Frame {
                payload: Some(p), ..
            }) => p,
            other => panic!("expected compacted identifier rewrite, got {other:?}"),
        };
        assert!(
            compact.len() < second.len(),
            "interned identifier should shrink the packet"
        );

        egress.set_state(SessionState::StreamingEgress);
        match egress.poll(&compact).unwrap() {
            PollResult::Stream(StreamResult::Frame {
                payload: Some(expanded),
                ..
            }) => {
                assert_eq!(expanded, second, "egress must restore the identifier string");
            }
            other => panic!("expected expanded custom payload, got {other:?}"),
        }
    }

    #[tokio::test]
    async fn test_minecraft_set_compression_then_recompresses_play_packets() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..");
        let wat_bytes = fs::read(root.join("middlewares").join("minecraft.wat")).expect("read wat");
        let mut session = WasmProtocolSession::from_wat(&wat_bytes).expect("session");
        session.set_state(SessionState::Streaming);
        session.set_flow_direction(false);

        let mut login_hs = Vec::new();
        push_varint(0, &mut login_hs);
        push_varint(47, &mut login_hs);
        let host = b"play.example.com";
        push_varint(host.len() as u32, &mut login_hs);
        login_hs.extend_from_slice(host);
        login_hs.extend_from_slice(&25565u16.to_be_bytes());
        push_varint(2, &mut login_hs);
        let mut hs_pkt = Vec::new();
        push_varint(login_hs.len() as u32, &mut hs_pkt);
        hs_pkt.extend_from_slice(&login_hs);
        match session.poll(&hs_pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { .. }) => {}
            other => panic!("expected login handshake frame, got {other:?}"),
        }

        session.set_state(SessionState::StreamingEgress);
        session.set_conn_state(2);
        session.set_flow_direction(true);
        let mut set_comp = Vec::new();
        push_varint(3, &mut set_comp);
        push_varint(256, &mut set_comp);
        let mut set_comp_pkt = Vec::new();
        push_varint(set_comp.len() as u32, &mut set_comp_pkt);
        set_comp_pkt.extend_from_slice(&set_comp);
        match session.poll(&set_comp_pkt).unwrap() {
            PollResult::Stream(StreamResult::Frame { len, payload, .. }) => {
                assert_eq!(len, set_comp_pkt.len());
                assert!(payload.is_none(), "Set Compression itself must pass through");
            }
            other => panic!("expected Set Compression frame, got {other:?}"),
        }

        let mut raw_packet = Vec::new();
        push_varint(0x27, &mut raw_packet);
        for i in 0..500 {
            raw_packet.push((i % 256) as u8);
        }
        let framed = frame_uncompressed_packet(&raw_packet);
        let mut pending = framed.clone();
        let mut out = Vec::new();
        session
            .process_egress_stream(&mut pending, &mut out)
            .await
            .unwrap();
        assert!(pending.is_empty());
        assert_ne!(out, framed, "play packets must recompress after Set Compression");
        let (pkt_len, len_n) = read_varint(&out).unwrap();
        let (data_len, dl_n) = read_varint(&out[len_n..]).unwrap();
        assert_eq!(data_len as usize, raw_packet.len());
        let decomp = deflate_decompress(&out[len_n + dl_n..len_n + pkt_len as usize]).unwrap();
        assert_eq!(&decomp, &raw_packet);
    }

    #[test]
    fn test_minecraft_wat_discovery_targets_and_payload() {
        let wat = get_default_middleware_wat("minecraft").unwrap();
        let mut session = WasmProtocolSession::from_wat(wat).unwrap();

        // 1. Discovery targets
        let targets = session.discovery_targets().unwrap().unwrap();
        assert_eq!(
            targets,
            vec![
                "224.0.2.60:4445".to_string(),
                "255.255.255.255:4445".to_string(),
                "127.0.0.1:4445".to_string()
            ]
        );

        // 2. Discovery template
        let template = session.discovery_template().unwrap().unwrap();
        assert_eq!(template, "[MOTD]{prefix}{name}[/MOTD][AD]{port}[/AD]");

        // 3. Build discovery payload
        let payload_bytes = session
            .build_discovery_payload("生存服", 25565, "[Prism] ")
            .unwrap()
            .unwrap();
        let payload_str = String::from_utf8(payload_bytes).unwrap();
        assert_eq!(payload_str, "[MOTD][Prism] 生存服[/MOTD][AD]25565[/AD]");

        // 4. Default helper functions
        assert_eq!(
            get_default_discovery_targets(),
            vec![
                "224.0.2.60:4445".to_string(),
                "255.255.255.255:4445".to_string(),
                "127.0.0.1:4445".to_string()
            ]
        );
        let default_payload = build_default_discovery_payload("生存服", 25565, "[Prism] ").unwrap();
        assert_eq!(default_payload, "[MOTD][Prism] 生存服[/MOTD][AD]25565[/AD]");
    }

    #[test]
    fn test_component_syntax_parse() {
        let sample = r#"(component
            (type $Config (record
                (field "recompress-threshold" u32)
                (field "deflate-level" u8)
                (field "discovery-targets" string)
            ))
            (export "config" (type $Config))
            (core module $main
                (memory (export "memory") 1)
                (global $threshold (mut i32) (i32.const 256))
                (func (export "set_threshold") (param $val i32)
                    (global.set $threshold (local.get $val))
                )
                (func (export "get_threshold") (result i32)
                    (global.get $threshold)
                )
                (func (export "poll") (param $buf_ptr i32) (param $buf_len i32) (param $state i32) (result i64)
                    (i64.or
                        (i64.shl (i64.extend_i32_u (local.get $state)) (i64.const 32))
                        (i64.extend_i32_u (global.get $threshold))
                    )
                )
            )
            (core instance $inst (instantiate $main))
            (func (export "set-threshold") (param "val" u32)
                (canon lift (core func $inst "set_threshold"))
            )
            (func (export "get-threshold") (result u32)
                (canon lift (core func $inst "get_threshold"))
            )
            (func (export "poll") (param "buf-ptr" u32) (param "buf-len" u32) (param "state" u32) (result s64)
                (canon lift (core func $inst "poll"))
            )
            (export "main" (core module $main))
        )"#;
        let engine = Engine::default();
        let component =
            wasmtime::component::Component::new(&engine, sample).expect("parse component");
        let component_type = component.component_type();
        let mut found_config = false;
        for (name, item) in component_type.exports(&engine) {
            if name == "config" {
                if let wasmtime::component::types::ComponentItem::Type(
                    wasmtime::component::Type::Record(r),
                ) = item.ty
                {
                    assert_eq!(r.fields().count(), 3);
                    found_config = true;
                }
            }
        }
        assert!(found_config);

        // Instantiate and test execution!
        let mut store = Store::new(&engine, ());
        let linker = wasmtime::component::Linker::new(&engine);
        let instance = linker
            .instantiate(&mut store, &component)
            .expect("instantiate component");
        let core_module = instance
            .get_module(&mut store, "main")
            .expect("exported core module 'main'");
        let mut core_store = Store::new(&engine, ());
        let core_instance = wasmtime::Instance::new(&mut core_store, &core_module, &[])
            .expect("instantiate core module");
        let core_mem = core_instance
            .get_memory(&mut core_store, "memory")
            .expect("core memory");
        assert_eq!(core_mem.data_size(&core_store), 65536);
    }

    #[test]
    fn test_component_middleware_lifecycle() {
        let mc_wat = get_default_middleware_wat("minecraft").expect("minecraft.wat");
        let mw =
            WasmMiddleware::from_wat("minecraft", mc_wat).expect("compile minecraft component");
        let schema = mw.schema().expect("schema extracted");
        assert_eq!(schema.name, "minecraft");
        assert_eq!(schema.fields.len(), 4);

        let threshold_field = schema
            .fields
            .iter()
            .find(|f| f.key == "recompress-threshold")
            .unwrap();
        assert_eq!(threshold_field.field_type, ConfigFieldType::U32);

        // Create shared active session
        let session_handle = mw.create_shared_session().expect("create shared session");

        // Verify initial discovery targets
        {
            let mut sess = session_handle.lock().unwrap();
            let targets = sess.discovery_targets().unwrap().unwrap();
            assert!(targets.contains(&"224.0.2.60:4445".to_string()));
        }

        // Dynamically update config via set_dynamic_middleware_config
        let mut new_config = HashMap::new();
        new_config.insert("recompress-threshold".to_string(), serde_json::json!(512));
        new_config.insert(
            "discovery-targets".to_string(),
            serde_json::json!("10.0.0.1:4445,10.0.0.2:4445"),
        );
        set_dynamic_middleware_config("minecraft", new_config);

        // Verify that the active live session was hot-updated immediately!
        {
            let mut sess = session_handle.lock().unwrap();
            let targets = sess.discovery_targets().unwrap().unwrap();
            assert_eq!(targets, vec!["10.0.0.1:4445", "10.0.0.2:4445"]);
        }

        reset_dynamic_middleware_config("minecraft");
    }
}
