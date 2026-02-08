use std::{
    collections::VecDeque,
    io,
    path::Path,
    sync::{Arc, Mutex, OnceLock},
};

use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::Layer;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use crate::prism::config;

static GLOBAL_LOG_BUFFER: OnceLock<Arc<LogBuffer>> = OnceLock::new();

#[derive(Debug)]
pub struct LogBuffer {
    cap: usize,
    inner: Mutex<VecDeque<String>>,
}

impl LogBuffer {
    pub fn new(cap: usize) -> Self {
        Self {
            cap: cap.max(1),
            inner: Mutex::new(VecDeque::new()),
        }
    }

    pub fn push_line(&self, line: String) {
        let mut g = self.inner.lock().unwrap();
        while g.len() >= self.cap {
            g.pop_front();
        }
        g.push_back(line);
    }

    pub fn tail(&self, limit: usize) -> Vec<String> {
        let g = self.inner.lock().unwrap();
        let limit = limit.min(g.len());
        g.iter().rev().take(limit).cloned().collect()
    }
}

#[derive(Debug)]
pub struct LoggingRuntime {
    _guard: WorkerGuard,
}

pub fn global_log_buffer() -> Option<Arc<LogBuffer>> {
    GLOBAL_LOG_BUFFER.get().cloned()
}

pub fn init(cfg: &config::LoggingConfig) -> anyhow::Result<LoggingRuntime> {
    let level = cfg.level.trim().to_ascii_lowercase();
    let fmt = cfg.format.trim().to_ascii_lowercase();
    let out = cfg.output.trim();

    let filter = EnvFilter::try_from_default_env()
        .or_else(|_| {
            let directive = match level.as_str() {
                "debug" => "debug",
                "info" => "info",
                "warn" => "warn",
                "error" => "error",
                _ => "info",
            };
            EnvFilter::try_new(directive)
        })
        .context("logging: init filter")?;

    let (writer, guard) = make_writer(out)?;

    let base_fmt = tracing_subscriber::fmt::layer()
        .with_writer(writer)
        .with_ansi(fmt == "text")
        .with_target(true)
        .with_file(cfg.add_source)
        .with_line_number(cfg.add_source);

    let base_fmt = if fmt == "json" {
        base_fmt.json().boxed()
    } else {
        base_fmt.boxed()
    };

    let buf_layer = if cfg.admin_buffer.enabled {
        let buf = Arc::new(LogBuffer::new(cfg.admin_buffer.size.max(1)));
        let _ = GLOBAL_LOG_BUFFER.set(buf.clone());

        Some(
            tracing_subscriber::fmt::layer()
                .with_writer(LogBufferMakeWriter { buf })
                .with_ansi(false)
                .with_target(true)
                .with_file(cfg.add_source)
                .with_line_number(cfg.add_source)
                .json(),
        )
    } else {
        None
    };

    tracing_subscriber::registry()
        .with(filter)
        .with(base_fmt)
        .with(buf_layer)
        .init();

    Ok(LoggingRuntime { _guard: guard })
}

fn make_writer(output: &str) -> anyhow::Result<(tracing_appender::non_blocking::NonBlocking, WorkerGuard)> {
    match output {
        "stderr" => Ok(tracing_appender::non_blocking(io::stderr())),
        "stdout" => Ok(tracing_appender::non_blocking(io::stdout())),
        "discard" => Ok(tracing_appender::non_blocking(io::sink())),
        other => {
            let p = Path::new(other);
            if let Some(parent) = p.parent() {
                if !parent.as_os_str().is_empty() {
                    std::fs::create_dir_all(parent)
                        .with_context(|| format!("logging: mkdir {}", parent.display()))?;
                }
            }
            let file = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(p)
                .with_context(|| format!("logging: open {}", p.display()))?;
            Ok(tracing_appender::non_blocking(file))
        }
    }
}

#[derive(Clone)]
struct LogBufferMakeWriter {
    buf: Arc<LogBuffer>,
}

impl<'a> tracing_subscriber::fmt::MakeWriter<'a> for LogBufferMakeWriter {
    type Writer = LogBufferWriter;

    fn make_writer(&'a self) -> Self::Writer {
        LogBufferWriter {
            buf: self.buf.clone(),
            pending: Vec::with_capacity(512),
        }
    }
}

struct LogBufferWriter {
    buf: Arc<LogBuffer>,
    pending: Vec<u8>,
}

impl io::Write for LogBufferWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.pending.extend_from_slice(buf);

        // Drain complete lines.
        while let Some(pos) = self.pending.iter().position(|b| *b == b'\n') {
            let line = self.pending.drain(..=pos).collect::<Vec<u8>>();
            let s = String::from_utf8_lossy(&line).trim_end().to_string();
            if !s.is_empty() {
                self.buf.push_line(s);
            }
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}
