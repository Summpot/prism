mod prism;

use clap::Parser;

#[derive(Debug, Parser)]
#[command(
    name = "prism",
    version,
    about = "Prism - lightweight reverse proxy with tunnel mode"
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

    /// Run in headless server mode without desktop GUI, even if no config file was passed.
    #[arg(long)]
    headless: bool,

    /// Force launch desktop GUI client.
    #[arg(long)]
    gui: bool,

    /// Started automatically at system boot / startup.
    #[arg(long)]
    autostart: bool,

    /// Start silently in the background / minimized to system tray.
    #[arg(long)]
    silent: bool,

    /// Alias for --silent: start minimized to system tray.
    #[arg(long)]
    minimized: bool,

    /// Deep link URL or extra positional arguments (e.g. prism://...)
    #[arg(trailing_var_arg = true)]
    extra_args: Vec<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Quinn/reqwest pull both aws-lc-rs and ring into rustls. When more than one
    // crypto backend is enabled, rustls refuses to auto-select a process default
    // and panics on ServerConfig/ClientConfig builders used by QUIC tunnels.
    // Prefer ring to match the existing insecure-skip-verify path in quic transport.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls CryptoProvider");

    let cli = Cli::parse();

    if cli.gui || cli.autostart || cli.silent || cli.minimized {
        anyhow::bail!("The desktop GUI has migrated to Avalonia. Please run Prism.exe instead.");
    }

    prism::run(cli.config, cli.workdir, cli.middleware_dir).await
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_accepts_deep_link_url() {
        let args = ["prism", "prism://auth/callback?code=test12345"];
        let cli = Cli::try_parse_from(args).expect("Cli must accept deep link url");
        assert_eq!(cli.extra_args, vec!["prism://auth/callback?code=test12345"]);
    }

    #[test]
    fn test_cli_accepts_autostart_and_silent_flags() {
        let args = ["prism", "--autostart", "--silent"];
        let cli = Cli::try_parse_from(args).expect("Cli must accept autostart and silent flags");
        assert!(cli.autostart);
        assert!(cli.silent);
        assert!(!cli.minimized);

        let args2 = ["prism", "--minimized"];
        let cli2 = Cli::try_parse_from(args2).expect("Cli must accept minimized flag");
        assert!(cli2.minimized);
    }
}

