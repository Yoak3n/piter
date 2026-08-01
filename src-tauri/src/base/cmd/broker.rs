use crate::base::handle::Handle;
use crate::base::state::{AppState, GatewaySlot};
use tauri::Manager;

/// Return the broker HTTP URL (for desktop frontend to discover).
#[tauri::command]
pub fn get_broker_url() -> String {
    get_web_url()
}

pub fn get_web_url() -> String {
    // The live gateway is the source of truth (it can be started on demand
    // after pi is installed); AppState.web_url only reflects the initial one.
    if let Some(app) = Handle::global().app_handle() {
        if let Some(slot) = app.try_state::<GatewaySlot>() {
            if let Some(gw) = slot.lock().as_ref() {
                return gw.http_url();
            }
        }
        if let Some(state) = app.try_state::<AppState>() {
            return state.web_url.clone();
        }
    }
    String::new()
}
