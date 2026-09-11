//! Tauri desktop application integration for Prism client.
#![cfg(feature = "desktop")]

use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tauri::WindowEvent;
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent};

#[derive(Clone)]
pub struct DesktopClientState {
    pub client: Arc<crate::prism::tunnel::client::ClientController>,
    pub storage: Option<Arc<crate::prism::storage::StorageEngine>>,
}

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

    let workdir = crate::prism::runtime_paths::resolve_desktop_data_dir();

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

    if let Some(ref storage_engine) = storage {
        let snap = storage_engine.get_client_config_snapshot();
        if snap.active_config.auto_connect && !snap.active_config.server_addr.trim().is_empty() {
            let payload = crate::prism::admin::StartClientRequest {
                server_addr: snap.active_config.server_addr.clone(),
                transport: snap.active_config.transport.clone(),
                auth_token: snap.active_config.auth_token.clone(),
                listen_addr: snap.active_config.listen_addr.clone(),
                middleware: None,
                fake_lan_broadcast: snap.active_config.fake_lan_broadcast,
                motd_prefix: "[Prism] ".into(),
                optimizer: None,
                profile_id: snap.active_profile_id.clone(),
                profile_name: Some(snap.active_config.profile_name.clone()),
            };
            let client = client_controller.clone();
            let storage_clone = storage.clone();
            tokio::spawn(async move {
                if let Err(err) =
                    crate::prism::admin::do_client_start(&client, storage_clone.as_deref(), payload)
                        .await
                {
                    tracing::warn!(err = %err, "desktop: auto-connect failed");
                }
            });
        }
    }

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
        storage: storage.clone(),
    };

    // Bind embedded loopback admin API on ephemeral port for internal tasks
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let local_addr = listener.local_addr()?;
    tracing::info!(%local_addr, "prism desktop: internal loopback admin API started");

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
    let storage_for_tray = storage.clone();

    let desktop_client_state = DesktopClientState {
        client: client_controller.clone(),
        storage: storage.clone(),
    };

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

    #[tauri::command]
    async fn client_status(
        state: tauri::State<'_, DesktopClientState>,
    ) -> Result<serde_json::Value, String> {
        Ok(
            crate::prism::admin::do_client_status(Some(&state.client), state.storage.as_deref())
                .await,
        )
    }

    #[tauri::command]
    async fn client_start(
        state: tauri::State<'_, DesktopClientState>,
        payload: crate::prism::admin::StartClientRequest,
    ) -> Result<(), String> {
        crate::prism::admin::do_client_start(&state.client, state.storage.as_deref(), payload).await
    }

    #[tauri::command]
    async fn client_stop(state: tauri::State<'_, DesktopClientState>) -> Result<(), String> {
        crate::prism::admin::do_client_stop(&state.client, state.storage.as_deref()).await
    }

    #[tauri::command]
    fn client_get_profiles(
        state: tauri::State<'_, DesktopClientState>,
    ) -> Result<Vec<crate::prism::admin::ClientProfile>, String> {
        Ok(crate::prism::admin::do_client_get_profiles(
            state.storage.as_deref(),
        ))
    }

    #[tauri::command]
    fn client_save_profiles(
        state: tauri::State<'_, DesktopClientState>,
        profiles: Vec<crate::prism::admin::ClientProfile>,
    ) -> Result<(), String> {
        crate::prism::admin::do_client_save_profiles(state.storage.as_deref(), &profiles)
    }

    #[tauri::command]
    fn client_get_config(
        state: tauri::State<'_, DesktopClientState>,
    ) -> Result<crate::prism::storage::ClientConfigResponse, String> {
        Ok(crate::prism::admin::do_client_get_config(
            state.storage.as_deref(),
        ))
    }

    #[tauri::command]
    fn client_save_config(
        state: tauri::State<'_, DesktopClientState>,
        payload: crate::prism::admin::SaveConfigRequest,
    ) -> Result<(), String> {
        crate::prism::admin::do_client_save_config(state.storage.as_deref(), payload)
    }

    #[tauri::command]
    fn client_reset_stats(state: tauri::State<'_, DesktopClientState>) -> Result<(), String> {
        crate::prism::admin::do_client_reset_stats(state.storage.as_deref())
    }

    #[tauri::command]
    async fn client_logs(
        state: tauri::State<'_, DesktopClientState>,
        limit: Option<usize>,
    ) -> Result<Vec<crate::prism::tunnel::client::ClientLogEntry>, String> {
        let limit = limit.unwrap_or(200).clamp(1, 1000);
        Ok(crate::prism::admin::do_client_logs(Some(&state.client), limit).await)
    }

    #[tauri::command]
    async fn client_clear_logs(state: tauri::State<'_, DesktopClientState>) -> Result<(), String> {
        crate::prism::admin::do_client_clear_logs(Some(&state.client)).await;
        Ok(())
    }

    // 2. Run Tauri desktop application
    tauri::Builder::default()
        .manage(desktop_client_state)
        .plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.show();
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
            #[cfg(desktop)]
            {
                use tauri::Emitter;
                for arg in &args {
                    if arg.starts_with("prism://") {
                        if let Ok(url) = url::Url::parse(arg) {
                            let _ = app.emit("deep-link://new-url", vec![url]);
                        }
                    }
                }
            }
        }))
        .plugin(tauri_plugin_deep_link::init())
        .invoke_handler(tauri::generate_handler![
            open_external_url,
            client_status,
            client_start,
            client_stop,
            client_get_profiles,
            client_save_profiles,
            client_get_config,
            client_save_config,
            client_reset_stats,
            client_logs,
            client_clear_logs,
        ])
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
            let storage_for_tray_clone = storage_for_tray.clone();
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
                        let storage_clone = storage_for_tray_clone.clone();
                        tokio::spawn(async move {
                            let _ = crate::prism::admin::do_client_stop(
                                &ctrl,
                                storage_clone.as_deref(),
                            )
                            .await;
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
