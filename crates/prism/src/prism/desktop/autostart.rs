//! Pure Rust cross-platform autostart integration using the `auto-launch` crate.
#![cfg(feature = "desktop")]

use auto_launch::AutoLaunch;

pub fn create_auto_launcher() -> anyhow::Result<AutoLaunch> {
    let current_exe = std::env::current_exe()?;
    let current_exe_str = current_exe
        .to_str()
        .ok_or_else(|| anyhow::anyhow!("current executable path contains invalid UTF-8"))?;

    let launcher = auto_launch::AutoLaunchBuilder::new()
        .set_app_name("Prism")
        .set_app_path(current_exe_str)
        .set_args(&["--autostart", "--minimized"])
        .build()?;
    Ok(launcher)
}

pub fn is_autostart_enabled() -> bool {
    create_auto_launcher()
        .and_then(|l| Ok(l.is_enabled()?))
        .unwrap_or(false)
}

pub fn set_autostart(enable: bool) -> anyhow::Result<()> {
    let launcher = create_auto_launcher()?;
    if enable {
        launcher.enable()?;
    } else {
        launcher.disable()?;
    }
    Ok(())
}
