use std::{
    path::Path,
    sync::Arc,
};

use anyhow::Context;
use thiserror::Error;
use wasmtime::{Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

use crate::prism::config;

#[derive(Debug, Error)]
pub enum ParseError {
    #[error("need more data")]
    NeedMoreData,
    #[error("no match")]
    NoMatch,
    #[error("fatal parse error: {0}")]
    Fatal(String),
}

pub trait HostParser: Send + Sync {
    fn name(&self) -> &str;
    fn parse(&self, prelude: &[u8]) -> Result<String, ParseError>;
}

pub type SharedHostParser = Arc<dyn HostParser>;

pub fn build_host_parser(parsers: &[config::RoutingParserConfig]) -> anyhow::Result<SharedHostParser> {
    let mut out: Vec<SharedHostParser> = Vec::new();
    for p in parsers {
        out.push(Arc::new(WasmHostParser::from_config(p)?) as SharedHostParser);
    }
    Ok(Arc::new(ChainHostParser::new(out)))
}

pub struct ChainHostParser {
    parsers: Vec<SharedHostParser>,
}

impl ChainHostParser {
    pub fn new(parsers: Vec<SharedHostParser>) -> Self {
        let parsers = parsers.into_iter().filter(|p| !p.name().is_empty()).collect();
        Self { parsers }
    }
}

impl HostParser for ChainHostParser {
    fn name(&self) -> &str {
        "chain"
    }

    fn parse(&self, prelude: &[u8]) -> Result<String, ParseError> {
        let mut need_more = false;
        for p in &self.parsers {
            match p.parse(prelude) {
                Ok(host) => {
                    let h = host.trim().to_string();
                    if h.is_empty() {
                        continue;
                    }
                    return Ok(h);
                }
                Err(ParseError::NeedMoreData) => {
                    need_more = true;
                    continue;
                }
                Err(ParseError::NoMatch) => continue,
                Err(e) => return Err(e),
            }
        }
        if need_more {
            Err(ParseError::NeedMoreData)
        } else {
            Err(ParseError::NoMatch)
        }
    }
}

pub struct WasmHostParser {
    name: String,
    path_hint: String,
    fn_name: String,
    max_output_len: u32,

    engine: Engine,
    module: Module,
}

impl WasmHostParser {
    pub fn from_config(cfg: &config::RoutingParserConfig) -> anyhow::Result<Self> {
        let path = cfg.path.trim();
        if path.is_empty() {
            anyhow::bail!("protocol: wasm routing parser missing path");
        }

        let wasm_bytes = if let Some(rest) = path.strip_prefix("builtin:") {
            builtin_wasm_bytes(rest.trim())
                .with_context(|| format!("protocol: unknown builtin wasm parser {rest:?}"))?
                .to_vec()
        } else {
            std::fs::read(Path::new(path)).with_context(|| format!("protocol: read wasm {}", path))?
        };

        let fn_name = cfg
            .function
            .clone()
            .unwrap_or_else(|| "prism_parse".to_string());
        let name = if !cfg.name.trim().is_empty() {
            cfg.name.trim().to_string()
        } else {
            format!("wasm:{path}")
        };

        let max_output_len = cfg.max_output_len.unwrap_or(255).max(1);

        // One engine per parser keeps plugin isolation simple.
        let engine = Engine::default();
        let module = Module::new(&engine, wasm_bytes).context("protocol: compile wasm module")?;

        Ok(Self {
            name,
            path_hint: path.to_string(),
            fn_name,
            max_output_len,
            engine,
            module,
        })
    }

    fn instantiate(&self) -> anyhow::Result<(Store<()>, Instance, Memory, TypedFunc<i32, i64>)> {
        let mut store = Store::new(&self.engine, ());
        let linker: Linker<()> = Linker::new(&self.engine);
        // No WASI imports are needed for the builtin parsers.

        let instance = linker
            .instantiate(&mut store, &self.module)
            .context("protocol: instantiate wasm")?;

        let memory = instance
            .get_memory(&mut store, "memory")
            .context("protocol: wasm missing exported memory 'memory'")?;

        let parse = instance
            .get_typed_func::<i32, i64>(&mut store, &self.fn_name)
            .with_context(|| format!("protocol: wasm missing export {:?}", self.fn_name))?;

        Ok((store, instance, memory, parse))
    }

    fn parse_impl(&self, prelude: &[u8]) -> Result<String, ParseError> {
        let (mut store, _inst, memory, parse) = self
            .instantiate()
            .map_err(|e| ParseError::Fatal(e.to_string()))?;

        // Ensure memory can fit prelude at offset 0.
        let need = prelude.len() as u64;
        let mem_size = memory.data_size(&store) as u64;
        if need > mem_size {
            let delta = need - mem_size;
            let pages_needed = (delta + 65535) / 65536;
            memory
                .grow(&mut store, pages_needed as u64)
                .map_err(|e| ParseError::Fatal(format!("wasm memory grow failed: {e}")))?;
        }

        if !prelude.is_empty() {
            memory
                .write(&mut store, 0, prelude)
                .map_err(|e| ParseError::Fatal(format!("wasm memory write failed: {e}")))?;
        }

        let out = parse
            .call(&mut store, prelude.len() as i32)
            .map_err(|e| ParseError::Fatal(format!("wasm parse call failed: {e}")))?;

        if out == 0 {
            return Err(ParseError::NeedMoreData);
        }
        if out == 1 {
            return Err(ParseError::NoMatch);
        }
        if out == -1 {
            return Err(ParseError::Fatal("wasm parser fatal error".into()));
        }

        let ptr = (out as u64 & 0xffff_ffff) as u32;
        let len = ((out as u64) >> 32) as u32;
        if len == 0 {
            return Err(ParseError::NoMatch);
        }
        if len > self.max_output_len {
            return Err(ParseError::Fatal(format!("wasm hostname too long ({len})")));
        }

        let mut buf = vec![0u8; len as usize];
        memory
            .read(&mut store, ptr as usize, &mut buf)
            .map_err(|e| ParseError::Fatal(format!("wasm memory read failed: {e}")))?;

        let host = String::from_utf8_lossy(&buf)
            .trim()
            .to_ascii_lowercase();
        if host.is_empty() {
            return Err(ParseError::NoMatch);
        }
        Ok(host)
    }
}

impl HostParser for WasmHostParser {
    fn name(&self) -> &str {
        &self.name
    }

    fn parse(&self, prelude: &[u8]) -> Result<String, ParseError> {
        self.parse_impl(prelude)
    }
}

fn normalize_builtin_name(name: &str) -> String {
    name.trim().to_ascii_lowercase().replace('-', "_")
}

fn builtin_wasm_bytes(name: &str) -> Option<&'static [u8]> {
    match normalize_builtin_name(name).as_str() {
        "minecraft_handshake" | "minecraft" | "mc" => Some(include_bytes!(
            "../../internal/protocol/wasm/minecraft_handshake.wasm"
        )),
        "tls_sni" | "sni" | "tls" => Some(include_bytes!("../../internal/protocol/wasm/tls_sni.wasm")),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn varint(mut v: i32) -> Vec<u8> {
        let mut out = Vec::new();
        loop {
            let mut temp = (v & 0x7f) as u8;
            v = ((v as u32) >> 7) as i32;
            if v != 0 {
                temp |= 0x80;
            }
            out.push(temp);
            if v == 0 {
                break;
            }
        }
        out
    }

    fn build_mc_handshake(host: &str, port: u16, proto_ver: i32, next_state: i32) -> Vec<u8> {
        // Packet: length VarInt, packetId VarInt(0), protoVer VarInt, serverAddr String, port u16be, nextState VarInt
        let mut inner = Vec::new();
        inner.extend(varint(0));
        inner.extend(varint(proto_ver));
        let hb = host.as_bytes();
        inner.extend(varint(hb.len() as i32));
        inner.extend(hb);
        inner.extend(port.to_be_bytes());
        inner.extend(varint(next_state));

        let mut out = Vec::new();
        out.extend(varint(inner.len() as i32));
        out.extend(inner);
        out
    }

    #[test]
    fn builtin_wasm_minecraft_parses() {
        let cfg = config::RoutingParserConfig {
            name: "minecraft_handshake".into(),
            path: "builtin:minecraft_handshake".into(),
            function: None,
            max_output_len: None,
        };
        let p = WasmHostParser::from_config(&cfg).expect("parser");

        let data = build_mc_handshake("Play.Example.Com", 25565, 763, 1);
        let host = p.parse(&data).expect("parse");
        assert_eq!(host, "play.example.com");

        for i in 0..data.len() - 1 {
            let err = p.parse(&data[..i]).unwrap_err();
            assert!(matches!(err, ParseError::NeedMoreData));
        }
    }
}
