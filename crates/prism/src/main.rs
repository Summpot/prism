mod prism;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "prism",
    version,
    about = "Prism - lightweight Minecraft reverse proxy"
)]
struct Cli {
    /// Path to Prism config file (.toml/.yaml/.yml). If omitted, uses PRISM_CONFIG; then auto-detects prism.toml > prism.yaml > prism.yml from CWD; then falls back to the OS default path (Linux: /etc/prism/prism.toml; others: user config dir).
    #[arg(long, env = "PRISM_CONFIG")]
    config: Option<std::path::PathBuf>,

    /// Prism working directory (runtime state). Defaults to /var/lib/prism on Linux; on other OSes defaults to the per-user data dir (via directories::ProjectDirs).
    #[arg(long, env = "PRISM_WORKDIR")]
    workdir: Option<std::path::PathBuf>,

    /// Directory to load middleware .wat files from. Defaults to "<config_dir>/middlewares" (Linux default: /etc/prism/middlewares).
    #[arg(long, env = "PRISM_MIDDLEWARE_DIR")]
    middleware_dir: Option<std::path::PathBuf>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    prism::run(cli.config, cli.workdir, cli.middleware_dir).await
}
