use std::sync::Arc;

use pi_server::gateway::GatewayState;
use pi_server::gateway::handlers::pi::restart_pi_instance;

#[tauri::command]
pub fn restart_pi(gw: tauri::State<'_, Option<Arc<GatewayState>>>) -> Result<String, String> {
    match gw.inner().as_ref() {
        Some(gw) => {
            // Collect all instance IDs first (to avoid holding lock during restart)
            let instance_ids: Vec<String> = {
                let instances = gw.inner.instances.lock();
                instances.keys().cloned().collect()
            };

            if instance_ids.is_empty() {
                return Ok("No active pi processes to restart".into());
            }

            log::info!("[admin] restarting {} pi processes", instance_ids.len());
            let mut restarted = 0;
            for id in &instance_ids {
                match restart_pi_instance(gw, id) {
                    Ok(_) => restarted += 1,
                    Err(e) => log::warn!("[admin] failed to restart instance {}: {}", id, e),
                }
            }
            Ok(format!("Restarted {}/{} pi processes", restarted, instance_ids.len()))
        }
        None => Err("Pi binary not available. Download it from Settings > Versions.".into()),
    }
}

#[tauri::command]
pub fn stop_pi(gw: tauri::State<'_, Option<Arc<GatewayState>>>) -> Result<String, String> {
    match gw.inner().as_ref() {
        Some(gw) => {
            log::info!("[admin] stopping all pi processes");
            gw.kill_all();
            Ok("pi processes stopped".into())
        }
        None => Err("Pi binary not available. Download it from Settings > Versions.".into()),
    }
}

#[tauri::command]
pub fn get_pi_agent_settings() -> Result<pi_server::PiAgentSettings, String> {
    pi_server::read_pi_settings()
}

/// Save Pi agent settings back to ~/.pi/agent/settings.json
#[tauri::command]
pub fn save_pi_agent_settings(settings: pi_server::PiAgentSettings) -> Result<(), String> {
    let path = pi_server::get_pi_agent_dir().join("settings.json");
    let json = serde_json::to_string_pretty(&settings)
        .map_err(|e| format!("Failed to serialize settings: {}", e))?;
    std::fs::write(&path, json)
        .map_err(|e| format!("Failed to write {}: {}", path.display(), e))?;
    log::info!("[admin] pi agent settings saved to {}", path.display());
    Ok(())
}
