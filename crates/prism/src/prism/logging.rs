use std::collections::VecDeque;
use std::sync::{LazyLock, RwLock};
use std::time::SystemTime;
use std::{io, path::Path};

use anyhow::Context;
use serde::{Deserialize, Serialize};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{EnvFilter, Layer, layer::SubscriberExt, util::SubscriberInitExt};

use crate::prism::config;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogEntry {
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

static GLOBAL_LOG_BUFFER: LazyLock<RwLock<VecDeque<LogEntry>>> =
    LazyLock::new(|| RwLock::new(VecDeque::with_capacity(1000)));

pub fn now_timestamp() -> String {
    let now = SystemTime::now();
    let duration = now
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let total_secs = duration.as_secs();
    let millis = duration.subsec_millis();
    let hours = (total_secs / 3600) % 24;
    let mins = (total_secs / 60) % 60;
    let secs = total_secs % 60;
    format!("{:02}:{:02}:{:02}.{:03}", hours, mins, secs, millis)
}

pub fn append_log_entry(level: String, target: String, message: String) {
    if let Ok(mut guard) = GLOBAL_LOG_BUFFER.write() {
        if guard.len() >= 1000 {
            guard.pop_front();
        }
        guard.push_back(LogEntry {
            timestamp: now_timestamp(),
            level,
            target,
            message,
        });
    }
}

pub fn get_recent_logs(limit: usize) -> Vec<LogEntry> {
    if let Ok(guard) = GLOBAL_LOG_BUFFER.read() {
        let start = if guard.len() > limit {
            guard.len() - limit
        } else {
            0
        };
        guard.iter().skip(start).cloned().collect()
    } else {
        Vec::new()
    }
}

pub fn clear_recent_logs() {
    if let Ok(mut guard) = GLOBAL_LOG_BUFFER.write() {
        guard.clear();
    }
}

/// Standard tracing_subscriber Layer that collects tracing events into the in-memory buffer.
pub struct MemoryLogLayer;

struct FieldVisitor {
    message: String,
    fields: Vec<String>,
}

impl FieldVisitor {
    fn new() -> Self {
        Self {
            message: String::new(),
            fields: Vec::new(),
        }
    }

    fn finish(self) -> String {
        if self.fields.is_empty() {
            self.message
        } else if self.message.is_empty() {
            self.fields.join(" ")
        } else {
            format!("{} ({})", self.message, self.fields.join(" "))
        }
    }
}

impl tracing::field::Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            let mut s = format!("{value:?}");
            if s.starts_with('"') && s.ends_with('"') && s.len() >= 2 {
                s = s[1..s.len() - 1].to_string();
            }
            self.message = s;
        } else {
            self.fields.push(format!("{}={:?}", field.name(), value));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={}", field.name(), value));
        }
    }

    fn record_error(
        &mut self,
        field: &tracing::field::Field,
        value: &(dyn std::error::Error + 'static),
    ) {
        if field.name() == "message" {
            self.message = value.to_string();
        } else {
            self.fields.push(format!("{}={}", field.name(), value));
        }
    }
}

impl<S> Layer<S> for MemoryLogLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let meta = event.metadata();
        let level = meta.level().to_string();
        let target = meta.target().to_string();

        let mut visitor = FieldVisitor::new();
        event.record(&mut visitor);

        append_log_entry(level, target, visitor.finish());
    }
}

#[derive(Debug)]
pub struct LoggingRuntime {
    _guard: WorkerGuard,
}

pub fn init(logging: &config::LoggingConfig) -> anyhow::Result<LoggingRuntime> {
    let level = logging.level.trim().to_ascii_lowercase();
    let fmt = logging.format.trim().to_ascii_lowercase();
    let out = logging.output.trim();

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
        .with_file(logging.add_source)
        .with_line_number(logging.add_source);

    let base_fmt = if fmt == "json" {
        base_fmt.json().boxed()
    } else {
        base_fmt.boxed()
    };

    let memory_layer = MemoryLogLayer;

    tracing_subscriber::registry()
        .with(filter)
        .with(base_fmt)
        .with(memory_layer)
        .init();

    Ok(LoggingRuntime { _guard: guard })
}

pub fn init_desktop_or_test_subscriber() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));
    let fmt_layer = tracing_subscriber::fmt::layer();
    let memory_layer = MemoryLogLayer;

    let _ = tracing_subscriber::registry()
        .with(filter)
        .with(fmt_layer)
        .with(memory_layer)
        .try_init();
}

fn make_writer(
    output: &str,
) -> anyhow::Result<(tracing_appender::non_blocking::NonBlocking, WorkerGuard)> {
    match output {
        "stderr" => Ok(tracing_appender::non_blocking(io::stderr())),
        "stdout" => Ok(tracing_appender::non_blocking(io::stdout())),
        "discard" => Ok(tracing_appender::non_blocking(io::sink())),
        other => {
            let p = Path::new(other);
            if let Some(parent) = p.parent()
                && !parent.as_os_str().is_empty()
            {
                std::fs::create_dir_all(parent)
                    .with_context(|| format!("logging: mkdir {}", parent.display()))?;
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
