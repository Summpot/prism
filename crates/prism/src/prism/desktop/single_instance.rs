//! Pure Rust cross-platform single instance protection and IPC message forwarding using `interprocess`.
#![cfg(feature = "desktop")]

use std::sync::Arc;
use interprocess::local_socket::tokio::{Listener, Stream};
use interprocess::local_socket::traits::tokio::{Listener as _, Stream as _};
use interprocess::local_socket::{GenericNamespaced, ListenerOptions, ToNsName};
use tauri::{AppHandle, Manager};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const SINGLE_INSTANCE_SOCKET_NAME: &str = "prism-desktop-single-instance";

#[cfg(target_os = "windows")]
fn wait_for_parent_exit(pid: u32, timeout_ms: u32) {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::Threading::{OpenProcess, WaitForSingleObject, PROCESS_SYNCHRONIZE};
    unsafe {
        let handle = OpenProcess(PROCESS_SYNCHRONIZE, 0, pid);
        if !handle.is_null() {
            WaitForSingleObject(handle, timeout_ms);
            CloseHandle(handle);
        }
    }
}

#[cfg(unix)]
fn wait_for_parent_exit(pid: u32, timeout_ms: u32) {
    let start = std::time::Instant::now();
    let timeout = std::time::Duration::from_millis(timeout_ms as u64);
    while start.elapsed() < timeout {
        let res = unsafe { libc::kill(pid as libc::pid_t, 0) };
        if res != 0 {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
}

#[cfg(not(any(target_os = "windows", unix)))]
fn wait_for_parent_exit(_pid: u32, timeout_ms: u32) {
    std::thread::sleep(std::time::Duration::from_millis(timeout_ms.min(1000) as u64));
}

pub async fn check_single_instance_or_forward(
    args: &[String],
) -> anyhow::Result<Option<Listener>> {
    // If this process was spawned by an auto-update restart, wait for the old instance to completely terminate
    if let Ok(pid_str) = std::env::var("PRISM_RESTART_PID") {
        if let Ok(pid) = pid_str.parse::<u32>() {
            tracing::info!(old_pid = pid, "waiting for previous Prism process to terminate before checking single instance");
            wait_for_parent_exit(pid, 5000);
            tokio::time::sleep(std::time::Duration::from_millis(150)).await;
        }
    }

    let ns_name = SINGLE_INSTANCE_SOCKET_NAME.to_ns_name::<GenericNamespaced>()?;

    // 1. Try to connect to an existing running instance first
    if let Ok(mut stream) = Stream::connect(ns_name.clone()).await {
        tracing::info!("another Prism instance is already running; forwarding args and exiting");
        let payload = serde_json::to_vec(args)?;
        let _ = stream.write_all(&payload).await;
        let _ = stream.flush().await;
        return Ok(None);
    }

    // 2. If connection failed, attempt to become the primary instance listener
    let opts = ListenerOptions::new().name(ns_name).try_overwrite(true);
    match opts.create_tokio() {
        Ok(listener) => Ok(Some(listener)),
        Err(err) => {
            tracing::warn!(error = %err, "failed to create single instance listener; continuing as primary");
            Ok(None)
        }
    }
}

pub fn spawn_ipc_handler(app: AppHandle, listener: Listener) {
    let app_handle = Arc::new(app);
    tauri::async_runtime::spawn(async move {
        loop {
            let Ok(mut stream) = listener.accept().await else {
                continue;
            };
            let app = app_handle.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while let Ok(n) = stream.read(&mut chunk).await {
                    if n == 0 {
                        break;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                    if buf.len() > 65536 {
                        break;
                    }
                }
                if let Ok(incoming_args) = serde_json::from_slice::<Vec<String>>(&buf) {
                    let is_silent = incoming_args.iter().any(|a| {
                        a == "--silent" || a == "--minimized" || a == "--autostart"
                    });
                    if !is_silent {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    for arg in &incoming_args {
                        if arg.starts_with("prism://") {
                            crate::prism::desktop::deep_link::dispatch_deep_link(&app, arg);
                        }
                    }
                }
            });
        }
    });
}
