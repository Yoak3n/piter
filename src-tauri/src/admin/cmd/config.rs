use super::super::config::ConfigManager;
use super::super::types::AdminConfig;

#[tauri::command]
pub fn get_admin_config() -> AdminConfig {
    ConfigManager::global().get_config()
}

#[tauri::command]
pub fn update_admin_config(config: AdminConfig) -> Result<AdminConfig, String> {
    ConfigManager::global().update_config(config)
}
