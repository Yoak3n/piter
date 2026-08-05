use crate::base::window::{manager::Manager as WM, schema::WindowType};

use super::broker::get_web_url;

/// Append the saved theme + language as query params so the web frontend can
/// apply them on first load without relying on IPC (remote origins).
pub fn web_url_with_prefs(http_url: &str) -> String {
    let app = crate::admin::config::ConfigManager::global()
        .get_config()
        .app;
    tauri::Url::parse_with_params(
        http_url,
        &[("theme", app.theme), ("lang", app.language)],
    )
    .map(|u| u.to_string())
    .unwrap_or_else(|_| http_url.to_string())
}

/// Navigate the main window to the Tauri admin panel (built-in frontend).
#[tauri::command]
pub fn navigate_to_admin() -> Result<(), String> {
    do_navigate_to_admin()
}

/// Navigate the main window to the chat frontend (served by gateway at /chat).
#[tauri::command]
pub fn navigate_to_web() -> Result<(), String> {
    do_navigate_to_web()
}

pub fn do_navigate_to_admin() -> Result<(), String> {
    let window = WM::global()
        .get_window(WindowType::Main)
        .ok_or("Main window not found")?;
    let admin_url = if cfg!(debug_assertions) {
        "http://localhost:1420/"
    } else if cfg!(target_os = "windows") {
        // Tauri v2 on Windows serves assets over WebView2's virtual host
        // mapping: `http://tauri.localhost` (not https).
        "http://tauri.localhost/"
    } else {
        "tauri://localhost/"
    };
    let url = tauri::Url::parse(admin_url)
        .map_err(|e| format!("Invalid admin URL: {}", e))?;
    log::info!("[nav] navigating to admin: {}", admin_url);
    let _ = window.navigate(url);
    Ok(())
}

pub fn do_navigate_to_web() -> Result<(), String> {
    let http_url = get_web_url();
    if http_url.is_empty() {
        return Err("Web URL not available — gateway not started".into());
    }
    let window = WM::global()
        .get_window(WindowType::Main)
        .ok_or("Main window not found")?;
    let url = tauri::Url::parse(&web_url_with_prefs(&format!("{}chat", http_url)))
        .map_err(|e| format!("Invalid web URL: {}", e))?;
    log::info!("[nav] navigating to chat: {}", url);
    let _ = window.navigate(url);
    Ok(())
}
