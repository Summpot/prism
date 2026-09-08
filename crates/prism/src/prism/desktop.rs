//! Tauri desktop application integration for Prism client.
#![cfg(feature = "desktop")]

use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tauri::WindowEvent;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};

pub async fn run(config_path: Option<PathBuf>) -> anyhow::Result<()> {
    crate::prism::logging::init_desktop_or_test_subscriber();
    tracing::info!("prism: starting desktop GUI mode");

    // 1. Prepare Prism client controller and background Admin/Client API
    let (reload_tx, _) = tokio::sync::watch::channel(crate::prism::telemetry::ReloadSignal::new());
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let client_controller = Arc::new(crate::prism::tunnel::client::ClientController::new(None));

    let resolved_config = crate::prism::config::resolve_config_path(config_path).ok();
    let loaded_cfg = resolved_config
        .as_ref()
        .and_then(|r| crate::prism::config::load_config(&r.path).ok());

    let (auth_cfg, actual_config_path) = match (&resolved_config, &loaded_cfg) {
        (Some(r), Some(c)) => (c.auth.clone(), r.path.clone()),
        (Some(r), None) => (crate::prism::auth::AuthConfig::default(), r.path.clone()),
        _ => (
            crate::prism::auth::AuthConfig::default(),
            PathBuf::from("prism.toml"),
        ),
    };

    let workdir = directories::ProjectDirs::from("com", "prism", "prism")
        .map(|p| p.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));

    let auth_manager = Arc::new(crate::prism::auth::AuthManager::new(
        auth_cfg,
        Some(&workdir),
    ));

    let storage_path = workdir.join("prism.db");
    let storage = match crate::prism::storage::StorageEngine::open(&storage_path) {
        Ok(s) => Some(Arc::new(s)),
        Err(err) => {
            tracing::warn!(err = %err, "desktop: failed to open persistent storage; continuing without DB");
            None
        }
    };

    let admin_state = crate::prism::admin::AdminState {
        sessions: Arc::new(crate::prism::telemetry::SessionRegistry::new()),
        optimizer: Arc::new(crate::prism::telemetry::OptimizerStatsRegistry::new()),
        config_path: actual_config_path,
        reload_tx,
        tunnel: None,
        auth: crate::prism::admin::AdminAuth::default(),
        management: None,
        worker: None,
        client: Some(client_controller.clone()),
        auth_manager: Some(auth_manager),
        serve_frontend: false,
        storage,
    };

    // Bind embedded loopback admin/client server on 127.0.0.1:8080
    let listener = match tokio::net::TcpListener::bind("127.0.0.1:8080").await {
        Ok(l) => l,
        Err(_) => tokio::net::TcpListener::bind("127.0.0.1:0").await?,
    };
    let local_addr = listener.local_addr()?;
    tracing::info!(%local_addr, "prism desktop: embedded admin/client API listening on loopback");

    let admin_state_clone = admin_state.clone();
    tokio::spawn(async move {
        let _ = crate::prism::admin::serve_listener_with_shutdown(
            listener,
            admin_state_clone,
            shutdown_rx,
        )
        .await;
    });

    let client_ctrl_for_tray = client_controller.clone();

    #[tauri::command]
    fn open_external_url(url: String) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            std::process::Command::new("rundll32")
                .args(["url.dll,FileProtocolHandler", &url])
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "macos")]
        {
            std::process::Command::new("open")
                .arg(&url)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        #[cfg(target_os = "linux")]
        {
            std::process::Command::new("xdg-open")
                .arg(&url)
                .spawn()
                .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    // 2. Run Tauri desktop application
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .invoke_handler(tauri::generate_handler![open_external_url])
        .setup(move |app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_deep_link::DeepLinkExt;
                if let Err(err) = app.deep_link().register_all() {
                    tracing::warn!(err = %err, "failed to register deep link schemes");
                }
            }

            let main_window = app.get_webview_window("main").expect("main window exists");
            let _ = main_window.center();
            let _ = main_window.show();

            // Build Tray Menu
            let title_i = MenuItem::with_id(app, "title", "Prism Client", false, None::<&str>)?;
            let sep0 = tauri::menu::PredefinedMenuItem::separator(app)?;
            let show_i = MenuItem::with_id(app, "show", "Open Prism Client", true, None::<&str>)?;
            let hide_i = MenuItem::with_id(app, "hide", "Hide to Tray", true, None::<&str>)?;
            let sep1 = tauri::menu::PredefinedMenuItem::separator(app)?;
            let disconnect_i =
                MenuItem::with_id(app, "disconnect", "Disconnect Tunnel", true, None::<&str>)?;
            let sep2 = tauri::menu::PredefinedMenuItem::separator(app)?;
            let quit_i = MenuItem::with_id(app, "quit", "Quit Prism", true, None::<&str>)?;
            let menu = Menu::with_items(
                app,
                &[
                    &title_i,
                    &sep0,
                    &show_i,
                    &hide_i,
                    &sep1,
                    &disconnect_i,
                    &sep2,
                    &quit_i,
                ],
            )?;

            let tray_icon = app.default_window_icon().cloned().unwrap_or_else(|| {
                tauri::image::Image::from_bytes(include_bytes!("../../icons/icon.png"))
                    .expect("embedded icon")
            });

            let ctrl_clone = client_ctrl_for_tray.clone();
            let _tray = TrayIconBuilder::new()
                .icon(tray_icon)
                .menu(&menu)
                .show_menu_on_left_click(false)
                .tooltip("Prism Client")
                .on_menu_event(move |app, event| match event.id.as_ref() {
                    "show" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.show();
                            let _ = window.unminimize();
                            let _ = window.set_focus();
                        }
                    }
                    "hide" => {
                        if let Some(window) = app.get_webview_window("main") {
                            let _ = window.hide();
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
                .on_tray_icon_event(|tray, event| match event {
                    TrayIconEvent::Click {
                        button: tauri::tray::MouseButton::Left,
                        button_state: tauri::tray::MouseButtonState::Up,
                        ..
                    }
                    | TrayIconEvent::DoubleClick {
                        button: tauri::tray::MouseButton::Left,
                        ..
                    } => {
                        let app = tray.app_handle();
                        if let Some(window) = app.get_webview_window("main") {
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.hide();
                            } else {
                                let _ = window.show();
                                let _ = window.unminimize();
                                let _ = window.set_focus();
                            }
                        }
                    }
                    _ => {}
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
