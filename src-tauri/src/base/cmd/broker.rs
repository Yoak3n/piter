use crate::base::handle::Handle;
use crate::base::state::AppState;
use tauri::Manager;

/// Return the broker HTTP URL (for desktop frontend to discover).
#[tauri::command]
pub fn get_broker_url() -> String {
    get_web_url()
}

pub fn get_web_url() -> String {
    Handle::global()
        .app_handle()
        .and_then(|app| app.try_state::<AppState>().map(|s| s.web_url.clone()))
        .unwrap_or_default()
}
