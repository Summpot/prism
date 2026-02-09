pub mod admin;
pub mod app;
pub mod config;
pub mod logging;
pub mod net;
pub mod protocol;
pub mod proxy;
pub mod router;
pub mod telemetry;
pub mod tunnel;
pub mod runtime_paths;

pub async fn run(
    config_path: Option<std::path::PathBuf>,
    workdir: Option<std::path::PathBuf>,
    routing_parser_dir: Option<std::path::PathBuf>,
) -> anyhow::Result<()> {
    app::run(config_path, workdir, routing_parser_dir).await
}
