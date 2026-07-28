//! REST API and shared handler logic for the gateway.

pub mod extensions;
pub mod pi;
pub mod project;
pub mod session;
pub mod system;

use serde::Serialize;

// ─── REST Response Types ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub pi_version: String,
    pub lan_urls: Vec<String>,
    pub broker_url: String,
    pub uptime_secs: u64,
}

#[derive(Serialize)]
pub struct LanInfoResponse {
    pub broker_ws_url: String,
    pub http_url: String,
    pub lan_urls: Vec<String>,
    pub qr_data: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub file_path: String,
    pub updated_at: u64,
    pub preview: String,
    pub cwd: String,
    // Runtime state (from session manager, None if not running)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,  // "active" | "idle" | "unloaded"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub thinking_level: Option<String>,
    #[serde(default)]
    pub message_count: u32,
    #[serde(default)]
    pub message_seq: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectGroup {
    pub path: String,
    pub dir_name: String,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Serialize)]
pub struct SessionsResponse {
    pub projects: Vec<ProjectGroup>,
}

#[derive(Serialize)]
pub struct GitBranchResponse {
    pub branch: Option<String>,
}

#[derive(Serialize)]
pub struct PiStatusResponse {
    pub running: bool,
    pub instance_id: Option<String>,
    pub session_path: Option<String>,
}
