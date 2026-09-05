//! Tauri desktop application integration for Prism client.
#![cfg(feature = "desktop")]

use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tauri::WindowEvent;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};

pub async fn run() -> anyhow::Result<()> {
    let _ = tracing_subscriber::fmt::try_init();
    tracing::info!("prism: starting desktop GUI mode");

    // 1. Prepare Prism client controller and background Admin/Client API
    let (reload_tx, _) = tokio::sync::watch::channel(crate::prism::telemetry::ReloadSignal::new());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let client_controller = Arc::new(crate::prism::tunnel::client::ClientController::new(None));

    let admin_state = crate::prism::admin::AdminState {
        sessions: Arc::new(crate::prism::telemetry::SessionRegistry::new()),
        traffic: Arc::new(crate::prism::telemetry::TrafficStatsRegistry::new()),
        config_path: PathBuf::from("prism.toml"),
        reload_tx,
        tunnel: None,
        auth: crate::prism::admin::AdminAuth::default(),
        management: None,
        worker: None,
        client: Some(client_controller.clone()),
    };

    // Find free local port for embedded admin/client server
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    drop(listener);

    tokio::spawn(async move {
        let _ =
            crate::prism::admin::serve_with_shutdown(local_addr, admin_state, shutdown_rx).await;
    });

    let client_ctrl_for_tray = client_controller.clone();

    // 2. Run Tauri desktop application
    tauri::Builder::default()
        .setup(move |app| {
            let main_window = app.get_webview_window("main").expect("main window exists");

            // Navigate window to local client UI
            let url = format!("http://{local_addr}/client");
            main_window.navigate(url.parse().expect("valid url"))?;

            // Build Tray Menu
            let show_i = MenuItem::with_id(app, "show", "Open Prism Client", true, None::<&str>)?;
            let disconnect_i =
                MenuItem::with_id(app, "disconnect", "Disconnect Tunnel", true, None::<&str>)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&show_i, &disconnect_i, &quit_i])?;

            let ctrl_clone = client_ctrl_for_tray.clone();
            let _tray = TrayIconBuilder::new()
                .menu(&menu)
                .tooltip("Prism Client")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "disconnect" => {
                        let ctrl = ctrl_clone.clone();
                        tokio::spawn(async move {
                            ctrl.stop().await;
                        });
                    }
                    "quit" => {
                        app.exit(0);
                    }
                    _ => {}
                })
                .on_tray_icon_event(|tray, event| {
                    if let TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } = event
                    {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                })
                .build(app)?;

            Ok(())
        })
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                // Prevent window from closing completely so game session is not interrupted
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");

    let _ = shutdown_tx.send(true);
    Ok(())
}
