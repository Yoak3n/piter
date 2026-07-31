use once_cell::sync::OnceCell;
use std::fs;
use std::path::PathBuf;
use tauri::Manager;

use super::types::AdminConfig;

pub struct ConfigManager {
    config_path: PathBuf,
}

impl ConfigManager {
    /// Initialize the manager with the app's data directory. Must be called
    /// once during app setup.
    pub fn init(app: &tauri::AppHandle) -> &'static Self {
        let config_path = app
            .path()
            .app_data_dir()
            .unwrap_or_else(|_| fallback_config_path())
            .join("config.json");
        Self::get_or_init(config_path)
    }

    pub fn global() -> &'static Self {
        Self::get_or_init(fallback_config_path())
    }

    fn get_or_init(config_path: PathBuf) -> &'static Self {
        static INSTANCE: OnceCell<ConfigManager> = OnceCell::new();
        INSTANCE.get_or_init(|| {
            let mgr = Self { config_path };
            if !mgr.config_path.exists() {
                let _ = mgr.write_defaults();
            }
            mgr
        })
    }

    pub fn get_config(&self) -> AdminConfig {
        match fs::read_to_string(&self.config_path) {
            Ok(json) => serde_json::from_str(&json).unwrap_or_default(),
            Err(_) => AdminConfig::default(),
        }
    }

    pub fn update_config(&self, config: AdminConfig) -> Result<AdminConfig, String> {
        let json = serde_json::to_string_pretty(&config)
            .map_err(|e| format!("Failed to serialize config: {}", e))?;

        let tmp = self.config_path.with_extension("json.tmp");
        fs::write(&tmp, &json)
            .map_err(|e| format!("Failed to write config: {}", e))?;
        fs::rename(&tmp, &self.config_path)
            .map_err(|e| format!("Failed to save config: {}", e))?;

        log::info!("[admin] config saved");
        Ok(config)
    }

    fn write_defaults(&self) -> Result<(), String> {
        if let Some(parent) = self.config_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|e| format!("Failed to create config dir: {}", e))?;
        }
        let defaults = AdminConfig::default();
        let json = serde_json::to_string_pretty(&defaults)
            .map_err(|e| format!("Failed to serialize defaults: {}", e))?;
        fs::write(&self.config_path, &json)
            .map_err(|e| format!("Failed to write defaults: {}", e))?;
        Ok(())
    }
}

/// Fallback config path used when the app handle is not yet available.
/// Uses the same `%APPDATA%\<identifier>` location as the app data dir.
fn fallback_config_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_default()
        .join(crate::identifier())
}
