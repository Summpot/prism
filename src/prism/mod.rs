pub mod admin;
pub mod app;
pub mod config;
pub mod logging;
pub mod protocol;
pub mod proxy;
pub mod router;
pub mod telemetry;

pub async fn run(config_path: Option<std::path::PathBuf>) -> anyhow::Result<()> {
    app::run(config_path).await
}
