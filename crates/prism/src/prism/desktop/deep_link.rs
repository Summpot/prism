//! Pure Rust cross-platform Deep Link registration and dispatching using `sysuri`.
#![cfg(feature = "desktop")]

use std::sync::Mutex;
use tauri::{AppHandle, Emitter, Manager};

static INITIAL_DEEP_LINK: Mutex<Option<String>> = Mutex::new(None);

pub fn set_initial_deep_link(url: String) {
    let mut lock = INITIAL_DEEP_LINK.lock().unwrap();
    if lock.is_none() {
        *lock = Some(url);
    }
}

pub fn take_initial_deep_link() -> Option<String> {
    INITIAL_DEEP_LINK.lock().unwrap().take()
}

pub fn register_protocol() -> anyhow::Result<()> {
    let current_exe = std::env::current_exe()?;
    let scheme = sysuri::UriScheme::new("prism", "Prism Protocol", current_exe);
    if let Err(err) = sysuri::register(&scheme) {
        tracing::warn!(err = %err, "sysuri: failed to register prism:// scheme with OS");
    }
    Ok(())
}

pub fn dispatch_deep_link(app: &AppHandle, url_str: &str) {
    let trimmed = url_str.trim();
    if !trimmed.starts_with("prism://") {
        return;
    }
    tracing::info!(url = %trimmed, "deep_link: dispatching incoming url to frontend");
    // 1. Emit standard Tauri event
    let _ = app.emit("prism://deep-link", trimmed);
    // 2. Also dispatch DOM CustomEvent directly into webview for convenience
    if let Some(window) = app.get_webview_window("main") {
        let script = format!(
            "window.dispatchEvent(new CustomEvent('prism-deep-link', {{ detail: {} }}));",
            serde_json::to_string(trimmed).unwrap_or_else(|_| format!("\"{}\"", trimmed))
        );
        let _ = window.eval(&script);
    }
}
