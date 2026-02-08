use std::{io, path::Path};

use anyhow::Context;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::prism::config;

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

    tracing_subscriber::registry()
        .with(filter)
        .with(base_fmt)
        .init();

    Ok(LoggingRuntime {
        _guard: guard,
    })
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
