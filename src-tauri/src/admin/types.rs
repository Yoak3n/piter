use serde::{Deserialize, Serialize};

fn default_theme() -> String {
    "system".into()
}
fn default_language() -> String {
    "system".into()
}
fn default_true() -> bool {
    true
}
fn default_timeout() -> u64 {
    300
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdminConfig {
    #[serde(default)]
    pub app: AppSettings,
    #[serde(default)]
    pub pi: PiSettings,
}

impl Default for AdminConfig {
    fn default() -> Self {
        Self {
            app: AppSettings::default(),
            pi: PiSettings::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    #[serde(default = "default_theme")]
    pub theme: String,
    /// "system" | "zh" | "en" — follows the OS locale when "system".
    #[serde(default = "default_language")]
    pub language: String,
    #[serde(default = "default_true")]
    pub auto_start: bool,
    #[serde(default = "default_true")]
    pub start_minimized: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            theme: default_theme(),
            language: default_language(),
            auto_start: true,
            start_minimized: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PiSettings {
    #[serde(default = "default_timeout")]
    pub request_timeout_secs: u64,
    #[serde(default = "default_true")]
    pub auto_restart_on_crash: bool,
}

impl Default for PiSettings {
    fn default() -> Self {
        Self {
            request_timeout_secs: default_timeout(),
            auto_restart_on_crash: true,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionInfo {
    pub instance_id: String,
    pub session_path: Option<String>,
    pub cwd: String,
    pub state: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct AdminStatus {
    pub pi_running: bool,
    pub active_sessions: Vec<SessionInfo>,
    pub pi_version: String,
    pub app_version: String,
    pub pi_binary_missing: bool,
    pub broker_ws_url: String,
    pub broker_http_url: String,
    pub uptime_secs: u64,
    pub data_dir: String,
}
