use crate::admin::cmd::*;
use crate::admin::config::ConfigManager;
use crate::base::cmd::*;
use crate::base::{
    handle::Handle,
    state::{AppState, GatewaySlot},
    tray::create_tray_icon,
    window::{
        manager::Manager as WM,
        schema::{WindowState, WindowType},
    },
};
use crate::pi::{try_resolve_pi_binary, locked_pi_version};

use tauri::{AppHandle, Builder, Emitter, Listener, Manager, RunEvent, generate_handler};
use tauri_plugin_log::{Target, TargetKind};

use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;

pub fn generate_handlers(
) -> impl Fn(tauri::ipc::Invoke<tauri::Wry>) -> bool + Send + Sync + 'static {
    generate_handler![
        get_broker_url,
        navigate_to_admin,
        navigate_to_web,
        minimize_window,
        toggle_maximize_window,
        close_window,
        is_maximized_window,
        show_window,
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
        uninstall_pi,
        list_pi_packages,
        install_pi_package,
        remove_pi_package,
        get_extension_overview,
        get_project_extension_overview,
        set_global_extensions,
        set_project_added_extensions,
        set_project_excluded_extensions,
        start_pi_gateway,
        list_pi_auth_status,
        set_pi_api_key,
        remove_pi_api_key,
        get_pi_models_config,
        save_pi_models_config,
        get_cost_dashboard,
        check_for_update,
        install_update
    ]
}

pub fn configure(builder: Builder<tauri::Wry>) -> Builder<tauri::Wry> {
    // Single-instance: prevent duplicate launches by focusing the existing
    // main window when a second instance is started.
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_single_instance::init(
        |app, _args, _cwd| {
            log::info!("[single-instance] second launch detected, focusing main window");
            let _ = WM::global().show_window(WindowType::Main, None);
            let _ = app;
        },
    ));

    let builder = builder.plugin(tauri_plugin_opener::init());

    let builder = builder.plugin(tauri_plugin_dialog::init());

    // D3: Linux（AUR 场景）关闭内置 updater，更新由 pacman 管理；
    // Windows 保留 tauri-plugin-updater。
    #[cfg(not(target_os = "linux"))]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    let builder = builder.plugin(
        tauri_plugin_log::Builder::new()
            .targets([
                Target::new(TargetKind::Stdout),
                Target::new(TargetKind::Webview),
                Target::new(TargetKind::Folder {
                    path: app_data_dir_path(),
                    file_name: Some("app".into()),
                }),
            ])
            .level(log::LevelFilter::Info)
            .timezone_strategy(tauri_plugin_log::TimezoneStrategy::UseLocal)
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

        ConfigManager::init(&app.handle());

        // Apply persisted app settings that have OS-level effects at startup.
        let admin_config = ConfigManager::global().get_config();
        ConfigManager::apply_autostart(&app.handle(), admin_config.app.auto_start);

        // Gateway state lives in a replaceable slot so it can also be started
        // on demand after pi is installed mid-session (see start_pi_gateway).
        let gw_slot: GatewaySlot = Arc::new(Mutex::new(None));
        let web_url = match try_start_gateway(app.handle()) {
            Ok(Some((gw, url))) => {
                *gw_slot.lock() = Some(gw);
                url
            }
            Ok(None) => {
                log::warn!("[gateway] skipped (pi binary not available)");
                log::warn!("[gateway] start it from Settings > Versions after downloading pi");
                String::new()
            }
            Err(e) => {
                log::error!("{}", e);
                String::new()
            }
        };

        app.manage(AppState {
            pi_version: pi_version.clone(),
            web_url: web_url.clone(),
            ..Default::default()
        });
        app.manage(gw_slot);

        Handle::global().init(app.handle().clone());
        let _ = create_tray_icon(app, false);

        // Auto-update check (release builds only; dev builds skip the chain).
        // Linux 关闭内置 updater（D3：AUR 场景由 pacman 管理更新）。
        #[cfg(all(not(debug_assertions), not(target_os = "linux")))]
        crate::updater::spawn_update_check(app.handle().clone());

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

        // 窗口控制事件（与 navigate-to-* 同一思路：绕过命令 ACL）。
        // chat 前端运行在网关的远程源（http://127.0.0.1:PORT），Tauri 的命令 ACL
        // 只放行本地源，invoke 自定义命令会被静默拒绝；事件通道不受限制。
        // 收到事件后照旧走 WindowManager，保证 WM 缓存状态与实际窗口一致。
        app.listen("window-minimize", |_event| {
            let _ = WM::global().minimized_window(WindowType::Main);
        });
        app.listen("window-toggle-maximize", |_event| {
            let _ = WM::global().toggle_maximize_window(WindowType::Main);
            emit_maximized_state();
        });
        app.listen("window-close", |_event| {
            let _ = WM::global().close_window(WindowType::Main);
        });
        app.listen("window-query-maximized", |_event| {
            emit_maximized_state();
        });

        // "Start minimized": launch into the tray without showing the window.
        if !admin_config.app.start_minimized {
            // Pi 未安装时 gateway 未启动（web_url 为空）：此时若仍加载
            // "/chat" 会解析到 frontendDist 下不存在的目录 → 404 白屏。
            // 回退到内置 admin 面板，引导用户在 Settings > Versions 下载 pi。
            let url = if web_url.is_empty() {
                "/".to_string()
            } else {
                crate::base::cmd::web_url_with_prefs(&format!("{}chat", web_url))
            };
            let _ = WM::global().show_window(WindowType::Main, Some(&url));
        }

        Ok(())
    })
}

/// Try to start the gateway if pi is installed. Returns the gateway instance
/// and its HTTP URL, or `None` when pi isn't available yet (not an error —
/// callers can retry via `start_pi_gateway` once pi is installed).
pub fn try_start_gateway(app: &AppHandle) -> Result<Option<(Arc<pi_server::gateway::GatewayState>, String)>, String> {
    let pi_exe = match try_resolve_pi_binary(app) {
        Ok(exe) => {
            log::info!("[pi] binary resolved at {}", exe.display());
            exe
        }
        Err(e) => {
            log::debug!("[pi] binary not found locally: {}", e);
            return Ok(None);
        }
    };

    let pi_version = locked_pi_version().to_string();
    let dist_path = get_dist_path(app);
    let app_data_dir = app
        .path()
        .app_data_dir()
        .unwrap_or_else(|_| app_data_dir_path());
    // dev 构建固定用 1421；release 不传端口，由 gateway 使用默认端口 31421
    //（被占用时回退随机端口），避免与 dev 端口冲突。
    let dev_port = std::env::var("TAURI_ENV_DEBUG")
        .ok()
        .and_then(|v| if v == "true" { Some(1421u16) } else { None });

    match pi_server::gateway::GatewayState::start_gateway(
        pi_exe,
        pi_version,
        dist_path,
        dev_port,
        None,
        app_data_dir,
    ) {
        Ok((gw, _port)) => {
            let url = gw.http_url();
            log::info!("Pi gateway: WS={} HTTP={}", gw.ws_url(), url);
            Ok(Some((gw, url)))
        }
        Err(e) => Err(format!("[gateway] failed to start: {}", e)),
    }
}

fn get_dist_path(app: &AppHandle) -> PathBuf {
    // Dev builds serve the vite-built frontend from the workspace, but release
    // builds must only use the resource dir bundled next to the executable —
    // a compile-time path must never leak into a shipped binary.
    #[cfg(debug_assertions)]
    {
        let dev_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("chat")
            .join("dist");
        if dev_path.exists() {
            return dev_path;
        }
    }
    app.path()
        .resource_dir()
        .map(|p| p.join("chat"))
        .unwrap_or_else(|_| PathBuf::from("."))
}

/// Piter's app data directory: `%APPDATA%\<identifier>` (e.g.
/// `C:\Users\<user>\AppData\Roaming\com.yoa.piter`).
fn app_data_dir_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join(crate::identifier())
}

/// 通知前端当前最大化状态（标题栏图标同步，事件通道不受命令 ACL 限制）。
fn emit_maximized_state() {
    match WM::global().get_window(WindowType::Main) {
        Some(win) => {
            let maximized = win.is_maximized().unwrap_or(false);
            let _ = win.emit("window-maximized-changed", maximized);
        }
        None => log::warn!("emit_maximized_state: main window not found"),
    }
}

pub fn app_event_handle(app_handle: &AppHandle, event: RunEvent) {
    match event {
        tauri::RunEvent::Ready | tauri::RunEvent::Resumed => {}
        tauri::RunEvent::ExitRequested { api, code, .. } if code.is_none() => {
            if let Some(slot) = app_handle.try_state::<GatewaySlot>() {
                if let Some(gw) = slot.lock().as_ref() {
                    log::info!("[app] stopping all pi processes before exit");
                    gw.kill_all();
                }
            }
            api.prevent_exit();
        }
        tauri::RunEvent::WindowEvent { label, event, .. } => match event {
            tauri::WindowEvent::Resized(_) => {
                // 拖拽边缘 / 顶栏双击等导致最大化状态变化时，同步标题栏图标
                emit_maximized_state();
            }
            tauri::WindowEvent::CloseRequested { api, .. } => {
                api.prevent_close();
                let window = app_handle.get_webview_window(&label).unwrap();
                // 同步 WM 缓存状态：所有关闭路径（标题栏 invoke close → WM::close_window →
                // window.close()、Alt+F4 等）最终都汇聚到 CloseRequested，在此统一同步为 Hidden，
                // 否则托盘 toggle / are_all_windows_closed 会误判"窗口仍可见"。
                if let Some(wt) = WindowType::from_label(&label) {
                    WM::global().update_window_state(wt, WindowState::Hidden);
                }
                // 窗口关闭（隐藏到托盘）：通知前端主动断开 WS，
                // 让订阅清理走 onclose → subscribers.remove → disconnected_since 最优路径，
                // 而不是等轻量模式 10min 计时后 destroy_window 才触发。
                let _ = window.emit("piter-window-hidden", ());
                let _ = window.hide();
            }
            tauri::WindowEvent::Focused(true) => {
                // 窗口从托盘恢复可见：同步 WM 缓存状态 + 通知前端复位并重连 WS。
                // 桌面 WebView 隐藏窗口不触发 visibilitychange（窗口隐藏≠页面不可见），
                // 恢复侧必须由这里显式发信号，与隐藏侧的 piter-window-hidden 对称。
                if let Some(window) = app_handle.get_webview_window(&label) {
                    if let Some(wt) = WindowType::from_label(&label) {
                        WM::global().update_window_state(wt, WindowState::VisibleFocused);
                    }
                    let _ = window.emit("piter-window-shown", ());
                }
            }
            tauri::WindowEvent::Focused(false) => {}
            tauri::WindowEvent::Destroyed => {}
            _ => {}
        },
        _ => {}
    }
}
