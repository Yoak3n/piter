use crate::admin::cmd::*;
use crate::base::cmd::*;
use crate::base::{
    handle::Handle,
    state::AppState,
    tray::create_tray_icon,
    window::{manager::Manager as WM, schema::WindowType},
};
use crate::pi::{try_resolve_pi_binary, locked_pi_version};

use tauri::{AppHandle, Builder, Listener, Manager, RunEvent, generate_handler};
use tauri_plugin_log::{Target, TargetKind};

use std::path::PathBuf;
use std::sync::Arc;

pub fn generate_handlers(
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    generate_handler![
        get_broker_url,
        navigate_to_admin,
        navigate_to_web,
        get_admin_config,
        update_admin_config,
        get_admin_status,
        restart_pi,
        stop_pi,
        get_pi_agent_settings,
        save_pi_agent_settings,
        open_path,
        get_pi_install_info,
        download_pi_version,
        uninstall_pi
    ]
}

pub fn configure(builder: Builder<tauri::Wry>) -> Builder<tauri::Wry> {
    let builder = builder.plugin(tauri_plugin_opener::init());

    let builder = builder.plugin(tauri_plugin_dialog::init());

    let builder = builder.plugin(
        tauri_plugin_log::Builder::new()
            .targets([
                Target::new(TargetKind::Stdout),
                Target::new(TargetKind::Webview),
                Target::new(TargetKind::Folder {
                    path: dirs::data_dir()
                        .unwrap_or_default()
                        .join("piter")
                        .join("logs"),
                    file_name: Some("app".into()),
                }),
            ])
            .level(log::LevelFilter::Info)
            .build(),
    );

    #[cfg(not(any(target_os = "android", target_os = "ios")))]
    let builder = {
        use tauri_plugin_autostart::MacosLauncher;
        builder.plugin(tauri_plugin_autostart::init(
            MacosLauncher::LaunchAgent,
            Some(vec!["--autostart"]),
        ))
    };

    builder.setup(|app| {
        let pi_version = locked_pi_version().to_string();

        crate::admin::config::ConfigManager::init();

        // Try to resolve pi from local sources (no auto-download).
        // If pi is not available, the app still starts — the user can
        // download it from the Versions tab in Settings.
        let pi_available = match try_resolve_pi_binary(app.handle()) {
            Ok(exe) => {
                log::info!("[pi] binary resolved at {}", exe.display());
                true
            }
            Err(e) => {
                log::warn!("[pi] binary not found locally: {}", e);
                log::warn!("[pi] pi features unavailable until downloaded from Settings > Versions");
                false
            }
        };

        // 获取资源路径
        let dist_path = get_dist_path(app.handle());

        let dev_port = std::env::var("TAURI_ENV_DEBUG")
            .ok()
            .and_then(|v| if v == "true" { Some(1421u16) } else { None });

        // Only start the gateway if pi binary is available.
        let gw_state: Option<Arc<pi_server::gateway::GatewayState>> = if pi_available {
            let resources_pi = if cfg!(debug_assertions) {
                std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                    .join("resources")
                    .join("pi")
                    .join(pi_server::pi_binary_name())
            } else {
                app.handle()
                    .path()
                    .resource_dir()
                    .unwrap_or_else(|_| std::path::PathBuf::from("."))
                    .join("pi")
                    .join(pi_server::pi_binary_name())
            };
            match pi_server::gateway::GatewayState::start_gateway(
                resources_pi, pi_version.clone(), dist_path, dev_port, None,
            ) {
                Ok((gw, _port)) => {
                    log::info!("Pi gateway: WS={} HTTP={}", gw.ws_url(), gw.http_url());
                    Some(gw)
                }
                Err(e) => {
                    log::error!("[gateway] failed to start: {}", e);
                    None
                }
            }
        } else {
            log::warn!("[gateway] skipped (pi binary not available)");
            None
        };

        // Derive web_url from gateway (or empty if not started).
        let web_url = gw_state
            .as_ref()
            .map(|gw| gw.http_url())
            .unwrap_or_default();

        app.manage(AppState {
            pi_version: pi_version.clone(),
            web_url: web_url.clone(),
            ..Default::default()
        });
        // Manage as Option so commands can gracefully handle the missing case.
        app.manage(gw_state);

        Handle::global().init(app.handle().clone());
        let _ = create_tray_icon(app, false);

        // Listen for navigation events from remote frontend (bypasses command ACL).
        app.listen("navigate-to-admin", |_event| {
            if let Err(e) = super::cmd::do_navigate_to_admin() {
                log::error!("[nav] failed to navigate to admin: {}", e);
            }
        });
        app.listen("navigate-to-web", |_event| {
            if let Err(e) = super::cmd::do_navigate_to_web() {
                log::error!("[nav] failed to navigate to web: {}", e);
            }
        });

        if dev_port.is_none() && !web_url.is_empty() {
            let _ =
                WM::global().show_window(WindowType::Main, Some(&format!("{}chat", web_url)));
        }

        Ok(())
    })
}

fn get_dist_path(app: &AppHandle) -> PathBuf {
    let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .join("web")
        .join("dist");
    if dev_path.exists() {
        return dev_path;
    }
    app.path()
        .resource_dir()
        .map(|p| p.join("web-frontend"))
        .unwrap_or(dev_path)
}

pub fn app_event_handle(app_handle: &AppHandle, event: RunEvent) {
    match event {
        tauri::RunEvent::Ready | tauri::RunEvent::Resumed => {}
        tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
            if let Some(gw) = app_handle.try_state::<Option<Arc<crate::pi::GatewayState>>>() {
                if let Some(gw) = gw.inner().as_ref() {
                    log::info!("[app] stopping all pi processes before exit");
                    gw.kill_all();
                }
            }
            api.prevent_exit();
        }
        tauri::RunEvent::WindowEvent { label, event, .. } => match event {
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let window = app_handle.get_webview_window(&label).unwrap();
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(true) => {}
            tauri::WindowEvent::Focused(false) => {}
            tauri::WindowEvent::Destroyed => {}
            _ => {}
        },
        _ => {}
    }
}
