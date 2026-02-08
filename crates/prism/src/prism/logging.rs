use std::{collections::HashMap, io, path::Path};

use anyhow::Context;
use opentelemetry::{global, KeyValue};
use opentelemetry::trace::TracerProvider as _;
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{trace as sdktrace, Resource};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter, Layer};

use crate::prism::config;

#[derive(Debug)]
pub struct LoggingRuntime {
    _guard: WorkerGuard,
    otel_provider: Option<sdktrace::SdkTracerProvider>,
}

impl Drop for LoggingRuntime {
    fn drop(&mut self) {
        if let Some(provider) = self.otel_provider.take() {
            // Best-effort flush for batch exporters.
            // (This is intentionally best-effort; shutdown failures shouldn't crash on exit.)
            let _ = provider.shutdown();
        }
    }
}

pub fn init(logging: &config::LoggingConfig, otel: &config::OpenTelemetryConfig) -> anyhow::Result<LoggingRuntime> {
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

    let (otel_provider, otel_layer) = build_otel_layer(otel)?;

    tracing_subscriber::registry()
        .with(otel_layer)
        .with(filter)
        .with(base_fmt)
        .init();

    Ok(LoggingRuntime {
        _guard: guard,
        otel_provider,
    })
}

fn build_otel_layer(
    otel: &config::OpenTelemetryConfig,
) -> anyhow::Result<(
    Option<sdktrace::SdkTracerProvider>,
    Option<impl Layer<tracing_subscriber::Registry> + Send + Sync + 'static>,
)> {
    if !otel.enabled {
        return Ok((None, None));
    }

    let endpoint = otel.otlp_endpoint.trim();
    if endpoint.is_empty() {
        anyhow::bail!("opentelemetry: enabled but otlp_endpoint is empty");
    }

    let mut headers: HashMap<String, String> = HashMap::new();
    for (k, v) in &otel.headers {
        let k = k.trim();
        if k.is_empty() {
            continue;
        }
        headers.insert(k.to_string(), v.clone());
    }

    let resource = Resource::builder()
        .with_attribute(KeyValue::new(SERVICE_NAME, otel.service_name.clone()))
        .build();

    // Traces: tracing spans -> OpenTelemetry traces.
    // opentelemetry-otlp >= 0.28 uses exporter/provider builders (no new_pipeline/new_exporter).
    let exporter = match otel.protocol.trim().to_ascii_lowercase().as_str() {
        "grpc" => {
            let mut b = opentelemetry_otlp::SpanExporter::builder().with_tonic();
            b = b.with_endpoint(endpoint);
            b = b.with_timeout(otel.timeout);
            b.build()?
        }
        "http" | "http/protobuf" => {
            let mut b = opentelemetry_otlp::SpanExporter::builder().with_http();
            b = b.with_endpoint(endpoint);
            b = b.with_timeout(otel.timeout);
            if !headers.is_empty() {
                b = b.with_headers(headers);
            }
            b.build()?
        }
        other => anyhow::bail!("opentelemetry: unsupported protocol {other:?} (expected grpc|http)"),
    };

    let provider = sdktrace::SdkTracerProvider::builder()
        .with_resource(resource)
        .with_batch_exporter(exporter)
        .build();

    global::set_tracer_provider(provider.clone());
    let tracer = provider.tracer("prism");

    let layer = tracing_opentelemetry::layer().with_tracer(tracer);
    Ok((Some(provider), Some(layer)))
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
