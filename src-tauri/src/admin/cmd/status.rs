use tauri::{AppHandle, Manager};

use super::super::types::{AdminStatus, SessionInfo};
use crate::base::state::GatewaySlot;
use crate::pi_runtime;

#[tauri::command]
pub fn get_admin_status(
    app: AppHandle,
    gw: tauri::State<'_, GatewaySlot>,
) -> AdminStatus {
    let (pi_running, active_sessions, broker_ws_url, broker_http_url, uptime_secs) =
        if let Some(gw) = gw.inner().lock().as_ref() {
            let sessions: Vec<SessionInfo> = {
                let instances = gw.inner.instances.lock();
                instances.iter().map(|(id, inst)| SessionInfo {
                    instance_id: id.clone(),
                    session_path: inst.session_path.clone(),
                    cwd: inst.cwd.clone(),
                    state: if inst.running.load(std::sync::atomic::Ordering::SeqCst) {
                        "running".to_string()
                    } else {
                        "stopped".to_string()
                    },
                }).collect()
            };
            (
                gw.has_active_processes(),
                sessions,
                gw.ws_url(),
                gw.http_url(),
                gw.uptime_secs(),
            )
        } else {
            (false, Vec::new(), String::new(), String::new(), 0)
        };

    AdminStatus {
        pi_running,
        active_sessions,
        pi_version: pi_server::locked_pi_version().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        pi_binary_missing: !pi_runtime::is_pi_binary_available(&app),
        broker_ws_url,
        broker_http_url,
        uptime_secs,
        data_dir: app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| dirs::data_dir().unwrap_or_default().join("piter"))
            .display()
            .to_string(),
    }
}
