use crate::base::window::{manager::Manager as WM, schema::WindowType};

use super::broker::get_web_url;

/// Navigate the main window to the Tauri admin panel (built-in frontend).
#[tauri::command]
pub fn navigate_to_admin() -> Result<(), String> {
    do_navigate_to_admin()
}

/// Navigate the main window to the web frontend (served by gateway).
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
    let url = tauri::Url::parse(&http_url)
        .map_err(|e| format!("Invalid web URL: {}", e))?;
    log::info!("[nav] navigating to web: {}", http_url);
    let _ = window.navigate(url);
    Ok(())
}
