use crate::base::cmd::*;
use crate::base::{
    handle::Handle,
    state::AppState,
    tray::create_tray_icon,
    window::{manager::Manager as WM, schema::WindowType},
};
use crate::pi::PiBroker;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::{generate_handler, AppHandle, Builder, Manager, RunEvent};
use tauri_plugin_log::{Target, TargetKind};

pub fn generate_handlers(
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    generate_handler![get_broker_url]
}

pub fn configure(builder: Builder<tauri::Wry>) -> Builder<tauri::Wry> {
    let builder = builder.plugin(tauri_plugin_opener::init());

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
        app.manage(AppState::default());

        let pi_exe = crate::pi::ensure_pi_binary(app.handle())?;
        let pi_version = crate::pi::locked_pi_version().to_string();
        let dist_path = get_dist_path(app.handle());

        let dev_port = std::env::var("TAURI_ENV_DEBUG")
            .ok()
            .and_then(|v| if v == "true" { Some(1421u16) } else { None });

        let broker = PiBroker::start(pi_exe, pi_version, dist_path, dev_port)
            .map_err(|e| format!("Failed to start PiBroker: {}", e))?;

        let broker_url = broker.url();
        let broker_http_url = broker.http_url();

        let broker_arc = Arc::new(broker);
        app.manage(broker_arc);

        std::env::set_var("PI_BROKER_URL", &broker_url);
        std::env::set_var("PI_HTTP_URL", &broker_http_url);
        log::info!("Pi broker: WS={} HTTP={}", broker_url, broker_http_url);

        Handle::global().init(app.handle().clone());
        let _ = create_tray_icon(app, false);

        if dev_port.is_none() {
            let _ =
                WM::global().show_window(WindowType::Main, Some(&format!("{}chat", broker_http_url)));
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
        .map(|p| p.join("frontend"))
        .unwrap_or(dev_path)
}

pub fn app_event_handle(app_handle: &AppHandle, event: RunEvent) {
    match event {
        tauri::RunEvent::Ready | tauri::RunEvent::Resumed => {}
        tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
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
