//! Session handlers: list, load, delete, create, rename + display helpers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::Query;
use axum::response::Json;
use serde_json::Value;

use super::SessionsResponse;
use crate::gateway::{GatewayState, state::build_project_session_tree};

// ─── Shared logic (callable from WS) ───────────────────────────────────────

pub fn load_session(file_path: &str) -> Vec<Value> {
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return vec![],
    };
    let mut messages = Vec::new();
    for line in content.lines() {
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("message") {
                if let Some(msg) = val.get("message") {
                    messages.push(msg.clone());
                }
            }
        }
    }
    messages
}

pub fn delete_session(instance_id: &str, state: &Arc<GatewayState>) -> Result<(), String> {
    // 1. Try to get the session file path from DB or in-memory pi_state
    let session_file = state.db.get_session_path(instance_id)
        .or_else(|| {
            let mgr = state.session_manager.lock();
            mgr.sessions.get(instance_id)
                .and_then(|s| s.pi_state.as_ref())
                .and_then(|p| p.session_file.clone())
        });

    // 2. Kill running pi process and remove from routes
    super::pi::kill_instance_for_gateway(state, instance_id);

    // 3. Remove from session manager (in-memory state)
    {
        let mut mgr = state.session_manager.lock();
        mgr.sessions.remove(instance_id);
        mgr.pending_links.remove(instance_id);
    }

    // 4. Remove DB record (by instance_id)
    let _ = state.db.delete_session_by_instance(instance_id);

    // 5. Try to delete the session file on disk
    if let Some(sf) = session_file {
        let _ = std::fs::remove_file(&sf);
    }

    // 6. Broadcast updated session list
    super::super::push_sessions_list_to_clients(state);
    Ok(())
}

/// Get the session directory for a working directory.
pub fn create_session(
    cwd: &str,
    _name: &str,
    _project_id: Option<&str>,
    _state: &GatewayState,
) -> Result<String, String> {
    Ok(session_dir_for(cwd))
}

pub fn rename_session(file_path: &str, new_name: &str) -> Result<(), String> {
    let content = std::fs::read_to_string(file_path)
        .map_err(|e| e.to_string())?;

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    if let Some(first) = lines.first_mut() {
        if let Ok(mut val) = serde_json::from_str::<Value>(first) {
            val["name"] = Value::String(new_name.to_string());
            *first = val.to_string();
        }
    }

    let new_content = lines.join("\n") + "\n";
    std::fs::write(file_path, new_content).map_err(|e| e.to_string())
}

// ─── Session path generation ────────────────────────────────────────────────

/// Get the session directory for a given working directory.
///
/// Returns the pi-encoded directory path (e.g., `~/.pi/agent/sessions/--E--Project-...--`).
/// This is passed to pi via `--session-dir`. Pi creates the actual `.jsonl` files inside.
pub fn session_dir_for(cwd: &str) -> String {
    let dir = get_sessions_dir().join(encode_project_name(cwd));
    let _ = std::fs::create_dir_all(&dir);
    dir.to_string_lossy().to_string()
}

// ─── REST handlers ──────────────────────────────────────────────────────────

pub async fn sessions_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<SessionsResponse> {
    let projects = build_project_session_tree(&state);
    Json(SessionsResponse { projects })
}

pub async fn load_session_handler(
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<Value>> {
    let file_path = match params.get("path") {
        Some(p) => p,
        None => return Json(vec![]),
    };
    Json(load_session(file_path))
}

pub async fn delete_session_handler(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let instance_id = match params.get("instanceId").or_else(|| params.get("path")) {
        Some(p) => p,
        None => return Json(serde_json::json!({"success": false, "error": "missing instanceId"})),
    };
    match delete_session(instance_id, &state) {
        Ok(()) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn create_session_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");
    let name = body.get("name").and_then(|v| v.as_str()).unwrap_or("New Session");
    let project_id = body.get("projectId").and_then(|v| v.as_str());

    match create_session(cwd, name, project_id, &state) {
        Ok(file_path) => Json(serde_json::json!({
            "success": true, "id": file_path, "file_path": file_path,
        })),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn rename_session_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let file_path = match body.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Json(serde_json::json!({"success": false, "error": "missing path"})),
    };
    let new_name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Json(serde_json::json!({"success": false, "error": "missing name"})),
    };
    match rename_session(file_path, new_name) {
        Ok(()) => {
            // The sessions list is built from in-memory state + DB, not the
            // session file, so mirror the new name there too. Otherwise the
            // sidebar would keep showing the old title after a refresh.
            if let Some(instance_id) = state.db.session_id_for_path(file_path) {
                let _ = state.db.set_session_name(&instance_id, new_name);
                state
                    .session_manager
                    .lock()
                    .set_session_name(&instance_id, new_name.to_string());
            }
            super::super::push_sessions_list_to_clients(&state);
            Json(serde_json::json!({"success": true}))
        }
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

// ─── Session file helpers ───────────────────────────────────────────────────

pub fn get_sessions_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".pi")
        .join("agent")
        .join("sessions")
}

pub fn decode_project_name(encoded: &str) -> String {
    let trimmed = encoded
        .strip_prefix("--")
        .and_then(|s| s.strip_suffix("--"))
        .unwrap_or(encoded);
    #[cfg(windows)]
    {
        if let Some(pos) = trimmed.find("--") {
            let drive = &trimmed[..pos];
            let rest = &trimmed[pos + 2..];
            format!("{}:\\{}", drive, rest.replace('-', "\\"))
        } else {
            trimmed.replace('-', "\\")
        }
    }
    #[cfg(not(windows))]
    {
        format!("/{}", trimmed.replace('-', "/"))
    }
}

pub fn encode_project_name(path: &str) -> String {
    #[cfg(windows)]
    {
        let step1 = path.replace(":\\", "--");
        let step2 = step1.replace('\\', "-");
        format!("--{}--", step2)
    }
    #[cfg(not(windows))]
    {
        let stripped = path.strip_prefix('/').unwrap_or(path);
        format!("--{}--", stripped.replace('/', "-"))
    }
}

pub fn format_timestamp(secs: u64) -> String {
    use chrono::{DateTime, Utc};
    let dt = DateTime::from_timestamp(secs as i64, 0).unwrap_or_else(|| Utc::now());
    dt.to_rfc3339()
}

pub fn generate_qr_svg(data: &str) -> String {
    use qrcode::QrCode;
    use qrcode::render::svg as qr_svg;
    match QrCode::new(data) {
        Ok(code) => code
            .render()
            .min_dimensions(200, 200)
            .dark_color(qr_svg::Color("#000000"))
            .light_color(qr_svg::Color("#ffffff"))
            .build(),
        Err(_) => String::new(),
    }
}
