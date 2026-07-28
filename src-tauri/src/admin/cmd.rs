use std::sync::Arc;

use pi_server::gateway::GatewayState;

use super::config::ConfigManager;
use super::types::{AdminConfig, AdminStatus};

#[tauri::command]
pub fn get_admin_config() -> AdminConfig {
    ConfigManager::global().get_config()
}

#[tauri::command]
pub fn update_admin_config(config: AdminConfig) -> Result<AdminConfig, String> {
    ConfigManager::global().update_config(config)
}

#[tauri::command]
pub fn get_admin_status(
    gw: tauri::State<'_, Arc<GatewayState>>,
) -> AdminStatus {
    AdminStatus {
        pi_running: gw.has_active_processes(),
        pi_instance_id: gw.active_instance_id(),
        pi_session_path: gw.active_session_path(),
        pi_version: pi_server::locked_pi_version().to_string(),
        app_version: env!("CARGO_PKG_VERSION").to_string(),
        broker_ws_url: gw.ws_url(),
        broker_http_url: gw.http_url(),
        uptime_secs: gw.uptime_secs(),
        data_dir: dirs::data_dir()
            .unwrap_or_default()
            .join("piter")
            .display()
            .to_string(),
    }
}

#[tauri::command]
pub fn restart_pi(gw: tauri::State<'_, Arc<GatewayState>>) -> String {
    log::info!("[admin] restarting all pi processes");
    gw.kill_all();
    "pi processes restarted".into()
}

#[tauri::command]
pub fn stop_pi(gw: tauri::State<'_, Arc<GatewayState>>) -> String {
    log::info!("[admin] stopping all pi processes");
    gw.kill_all();
    "pi processes stopped".into()
}

#[tauri::command]
pub fn get_pi_agent_settings() -> Result<pi_server::PiAgentSettings, String> {
    pi_server::read_pi_settings()
}
