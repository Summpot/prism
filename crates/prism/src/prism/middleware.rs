use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use anyhow::Context;
use thiserror::Error;
use wasmer::{imports, Engine, Instance, Memory, Module, Pages, Store, TypedFunction};

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
    /// The selected upstream address label (after any default port fill), if available.
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
    fn apply(&self, prelude: &[u8], ctx: &MiddlewareCtx) -> Result<MiddlewareOutput, MiddlewareError>;
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

        if let Ok(guard) = self.cache.lock() {
            if let Some(m) = guard.get(name) {
                return Ok(m.clone());
            }
        }

        let wat_path = self.wat_path_for(name);
        let mw = Arc::new(WasmMiddleware::from_wat_path(name, &wat_path)?) as SharedMiddleware;

        if let Ok(mut guard) = self.cache.lock() {
            guard.insert(name.to_string(), mw.clone());
        }

        Ok(mw)
    }
}

const DEFAULT_MIDDLEWARES: &[(&str, &str)] = &[
    (
        "minecraft_handshake",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../middlewares/minecraft_handshake.wat"
        )),
    ),
    (
        "tls_sni",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../middlewares/tls_sni.wat"
        )),
    ),
    (
        "host_to_upstream",
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../middlewares/host_to_upstream.wat"
        )),
    ),
];

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
                return Err(err)
                    .with_context(|| format!("middleware: create {}", path.display()));
            }
        }
    }

    Ok(created)
}

pub struct WasmMiddleware {
    name: String,
    path_hint: String,
    fn_name: String,
    engine: Engine,
    module: Module,
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

        let fn_name = "prism_mw_run".to_string();
        let engine = Engine::default();
        let store = Store::new(engine.clone());
        let module = Module::new(&store, wat_bytes).context("middleware: compile wat module")?;

        Ok(Self {
            name: name.to_string(),
            path_hint: path.display().to_string(),
            fn_name,
            engine,
            module,
        })
    }

    fn instantiate(
        &self,
    ) -> anyhow::Result<(
        Store,
        Instance,
        Memory,
        TypedFunction<(i32, i32), i64>,
    )> {
        let mut store = Store::new(self.engine.clone());
        let import_object = imports! {};

        let instance = Instance::new(&mut store, &self.module, &import_object)
            .context("middleware: instantiate wasm")?;

        let run: TypedFunction<(i32, i32), i64> = instance
            .exports
            .get_typed_function(&store, &self.fn_name)
            .with_context(|| format!("middleware: wasm missing export {:?}", self.fn_name))?;

        let memory = instance
            .exports
            .get_memory("memory")
            .map_err(|e| anyhow::anyhow!("middleware: wasm missing exported memory 'memory': {e}"))?
            .clone();

        Ok((store, instance, memory, run))
    }

    fn apply_impl(
        &self,
        prelude: &[u8],
        ctx: &MiddlewareCtx,
    ) -> Result<MiddlewareOutput, MiddlewareError> {
        let (mut store, _instance, memory, run) = self
            .instantiate()
            .map_err(|e| MiddlewareError::Fatal(e.to_string()))?;

        // Layout: [prelude @0] [ctx struct] [ctx strings]
        // ABI structs are little-endian.
        // Ctx struct (v1):
        //   u32 version (=1)
        //   u32 phase   (=0 parse, 1 rewrite)
        //   u32 upstream_ptr
        //   u32 upstream_len
        const CTX_STRUCT_LEN: u32 = 16;

        let mut cursor: u32 = ((prelude.len() as u32) + 7) & !7; // align8
        let ctx_ptr = cursor;
        cursor = cursor
            .checked_add(CTX_STRUCT_LEN)
            .ok_or_else(|| MiddlewareError::Fatal("ctx overflow".into()))?;

        let upstream = ctx
            .selected_upstream
            .as_deref()
            .unwrap_or_default()
            .as_bytes();

        let upstream_ptr = if !upstream.is_empty() {
            let p = cursor;
            cursor = cursor
                .checked_add(upstream.len() as u32)
                .ok_or_else(|| MiddlewareError::Fatal("ctx upstream overflow".into()))?;
            p
        } else {
            0
        };

        // Ensure memory can fit prelude+ctx at their offsets.
        let need = cursor as u64;
        let mut mem_size = memory.view(&store).data_size();
        if need > mem_size {
            let delta = need - mem_size;
            let pages_needed = (delta + 65535) / 65536;
            memory
                .grow(&mut store, Pages(pages_needed as u32))
                .map_err(|e| MiddlewareError::Fatal(format!("wasm memory grow failed: {e}")))?;
            mem_size = memory.view(&store).data_size();
        }

        // Always ensure a few pages so fixed-offset middlewares can place output safely.
        // (Rewrite outputs may be larger than the ctx struct region; wasm itself can't grow memory.)
        if mem_size < 4 * 65536 {
            let pages = ((4 * 65536 - mem_size) + 65535) / 65536;
            memory
                .grow(&mut store, Pages(pages as u32))
                .map_err(|e| MiddlewareError::Fatal(format!("wasm memory grow failed: {e}")))?;
        }

        if !prelude.is_empty() {
            memory
                .view(&store)
                .write(0, prelude)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm memory write prelude failed: {e}")))?;
        }

        if !upstream.is_empty() {
            memory
                .view(&store)
                .write(upstream_ptr as u64, upstream)
                .map_err(|e| {
                    MiddlewareError::Fatal(format!("wasm memory write upstream failed: {e}"))
                })?;
        }

        // Write ctx struct.
        let mut ctx_buf = [0u8; CTX_STRUCT_LEN as usize];
        ctx_buf[0..4].copy_from_slice(&1u32.to_le_bytes());
        ctx_buf[4..8].copy_from_slice(&(ctx.phase as u32).to_le_bytes());
        ctx_buf[8..12].copy_from_slice(&upstream_ptr.to_le_bytes());
        ctx_buf[12..16].copy_from_slice(&(upstream.len() as u32).to_le_bytes());

        memory
            .view(&store)
            .write(ctx_ptr as u64, &ctx_buf)
            .map_err(|e| MiddlewareError::Fatal(format!("wasm memory write ctx failed: {e}")))?;

            let out = run
                .call(&mut store, prelude.len() as i32, ctx_ptr as i32)
            .map_err(|e| MiddlewareError::Fatal(format!("wasm middleware call failed: {e}")))?;

        if out == 0 {
            return Err(MiddlewareError::NeedMoreData);
        }
        if out == 1 {
            return Err(MiddlewareError::NoMatch);
        }
        if out == -1 {
            return Err(MiddlewareError::Fatal("wasm middleware fatal error".into()));
        }

        let ptr = (out as u64 & 0xffff_ffff) as u32;
        let len = ((out as u64) >> 32) as u32;
        if len < 16 {
            return Err(MiddlewareError::Fatal(format!(
                "wasm middleware output too small (len={len}, path={})",
                self.path_hint
            )));
        }

        let view = memory.view(&store);
        let end = (ptr as u64)
            .checked_add(len as u64)
            .ok_or_else(|| MiddlewareError::Fatal("wasm output range overflow".into()))?;
        if end > view.data_size() {
            return Err(MiddlewareError::Fatal(format!(
                "wasm output out of bounds (ptr={ptr}, len={len}, mem={})",
                view.data_size()
            )));
        }

        let mut header = [0u8; 16];
        view.read(ptr as u64, &mut header)
            .map_err(|e| MiddlewareError::Fatal(format!("wasm memory read failed: {e}")))?;

        let host_ptr = u32::from_le_bytes(header[0..4].try_into().unwrap());
        let host_len = u32::from_le_bytes(header[4..8].try_into().unwrap());
        let rw_ptr = u32::from_le_bytes(header[8..12].try_into().unwrap());
        let rw_len = u32::from_le_bytes(header[12..16].try_into().unwrap());

        let mut out = MiddlewareOutput::default();

        if host_len > 0 {
            let host_end = (host_ptr as u64)
                .checked_add(host_len as u64)
                .ok_or_else(|| MiddlewareError::Fatal("host range overflow".into()))?;
            if host_end > view.data_size() {
                return Err(MiddlewareError::Fatal("host out of bounds".into()));
            }
            let mut buf = vec![0u8; host_len as usize];
            view.read(host_ptr as u64, &mut buf)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm host read failed: {e}")))?;
            let host = String::from_utf8_lossy(&buf).trim().to_ascii_lowercase();
            if !host.is_empty() {
                out.host = Some(host);
            }
        }

        if rw_len > 0 {
            let rw_end = (rw_ptr as u64)
                .checked_add(rw_len as u64)
                .ok_or_else(|| MiddlewareError::Fatal("rewrite range overflow".into()))?;
            if rw_end > view.data_size() {
                return Err(MiddlewareError::Fatal("rewrite out of bounds".into()));
            }
            let mut buf = vec![0u8; rw_len as usize];
            view.read(rw_ptr as u64, &mut buf)
                .map_err(|e| MiddlewareError::Fatal(format!("wasm rewrite read failed: {e}")))?;
            out.rewrite = Some(buf);
        }

        if out.host.is_none() && out.rewrite.is_none() {
            return Err(MiddlewareError::NoMatch);
        }

        Ok(out)
    }
}

impl Middleware for WasmMiddleware {
    fn name(&self) -> &str {
        &self.name
    }

    fn apply(&self, prelude: &[u8], ctx: &MiddlewareCtx) -> Result<MiddlewareOutput, MiddlewareError> {
        self.apply_impl(prelude, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_test_dir(name: &str) -> PathBuf {
        let mut p = std::env::temp_dir();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        p.push(format!("prism_mw_test_{name}_{}_{}", std::process::id(), now));
        fs::create_dir_all(&p).expect("mkdir");
        p
    }

    // Minimal middleware: if phase==parse, always return host="x" and rewrite="abc".
    const TEST_WAT: &str = r#"(module
    (memory (export "memory") 2)

  (func $pack (param $ptr i32) (param $len i32) (result i64)
    (i64.or
      (i64.extend_i32_u (local.get $ptr))
      (i64.shl (i64.extend_i32_u (local.get $len)) (i64.const 32))
    )
  )

    (func (export "prism_mw_run") (param $n i32) (param $ctx i32) (result i64)
    (local $phase i32)
    (local.set $phase (i32.load (i32.add (local.get $ctx) (i32.const 4))))

    ;; out struct at 65536
    ;; struct { host_ptr, host_len, rw_ptr, rw_len }
    (if (i32.eq (local.get $phase) (i32.const 0))
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
        (return (call $pack (i32.const 65536) (i32.const 16)))
      )
    )

    ;; rewrite phase: no-op
    (i64.const 1)
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
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("..").join("..");
        let dir = root.join("middlewares");

        for name in ["minecraft_handshake", "tls_sni", "host_to_upstream"] {
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
        let custom = dir.join("minecraft_handshake.wat");
        fs::write(&custom, "(module)\n").expect("write custom");
        let _ = materialize_default_middlewares(&dir).expect("materialize 3");
        let now = fs::read_to_string(&custom).expect("read custom");
        assert_eq!(now, "(module)\n");

        let _ = fs::remove_dir_all(&dir);
    }
}
