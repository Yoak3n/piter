use super::super::config::ConfigManager;
use super::super::types::AdminConfig;
use tauri::AppHandle;

#[tauri::command]
pub fn get_admin_config() -> AdminConfig {
    ConfigManager::global().get_config()
}

#[tauri::command]
pub fn update_admin_config(app: AppHandle, config: AdminConfig) -> Result<AdminConfig, String> {
    let saved = ConfigManager::global().update_config(config)?;
    // Make the auto-start toggle effective immediately.
    ConfigManager::apply_autostart(&app, saved.app.auto_start);
    Ok(saved)
}
