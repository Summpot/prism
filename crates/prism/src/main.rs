#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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

    /// Run in headless server mode without desktop GUI, even if no config file was passed.
    #[arg(long)]
    headless: bool,

    /// Force launch desktop GUI client.
    #[arg(long)]
    gui: bool,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    #[cfg(target_os = "windows")]
    unsafe {
        unsafe extern "system" {
            fn AttachConsole(dwProcessId: u32) -> i32;
        }
        // Attach to calling terminal so CLI usage prints output
        AttachConsole(0xFFFFFFFF);
    }

    // Quinn/reqwest pull both aws-lc-rs and ring into rustls. When more than one
    // crypto backend is enabled, rustls refuses to auto-select a process default
    // and panics on ServerConfig/ClientConfig builders used by QUIC tunnels.
    // Prefer ring to match the existing insecure-skip-verify path in quic transport.
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("install rustls CryptoProvider");

    let cli = Cli::parse();

    #[cfg(feature = "desktop")]
    {
        let has_display = {
            #[cfg(target_os = "linux")]
            {
                std::env::var_os("DISPLAY").is_some()
                    || std::env::var_os("WAYLAND_DISPLAY").is_some()
            }
            #[cfg(not(target_os = "linux"))]
            {
                true
            }
        };

        let should_launch_gui = cli.gui || (cli.config.is_none() && !cli.headless && has_display);
        if should_launch_gui {
            #[cfg(target_os = "windows")]
            unsafe {
                unsafe extern "system" {
                    fn GetConsoleWindow() -> *mut std::ffi::c_void;
                    fn ShowWindow(hwnd: *mut std::ffi::c_void, nCmdShow: i32) -> i32;
                    fn GetConsoleProcessList(process_list: *mut u32, count: u32) -> u32;
                }
                let mut pids = [0u32; 2];
                let count = GetConsoleProcessList(pids.as_mut_ptr(), 2);
                if count <= 1 {
                    let hwnd = GetConsoleWindow();
                    if !hwnd.is_null() {
                        ShowWindow(hwnd, 0);
                    }
                }
            }
            return prism::desktop::run().await;
        }
    }

    #[cfg(not(feature = "desktop"))]
    {
        if cli.gui {
            anyhow::bail!("Prism was compiled without desktop GUI support");
        }
    }

    prism::run(cli.config, cli.workdir, cli.middleware_dir).await
}
