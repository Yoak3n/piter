//! Extensions and session manager config handlers.

use std::collections::HashMap;
use std::sync::Arc;

use axum::response::Json;
use serde_json::Value;

use crate::gateway::GatewayState;

// ─── Shared logic (callable from WS) ───────────────────────────────────────

pub fn get_global_extensions(db: &crate::gateway::db::Db) -> Vec<String> {
    db.get_global_extensions()
}

pub fn set_global_extensions(db: &crate::gateway::db::Db, extensions: &[String]) -> Result<(), String> {
    db.set_global_extensions(extensions)
}

pub fn get_session_config(state: &GatewayState) -> (u64,) {
    let sm = state.session_manager.lock();
    (sm.idle_timeout.as_secs(),)
}

pub fn update_session_config(state: &GatewayState, idle_timeout_secs: u64) {
    state.session_manager.lock().set_idle_timeout(idle_timeout_secs);
}

// ─── REST handlers ──────────────────────────────────────────────────────────

pub async fn global_extensions_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let extensions = get_global_extensions(&state.db);
    Json(serde_json::json!({ "success": true, "extensions": extensions }))
}

pub async fn update_global_extensions_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let extensions = match body.get("extensions").and_then(|v| v.as_array()) {
        Some(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect::<Vec<_>>(),
        None => {
            return Json(serde_json::json!({"success": false, "error": "missing extensions"}));
        }
    };

    match set_global_extensions(&state.db, &extensions) {
        Ok(()) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn session_config_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let (timeout,) = get_session_config(&state);
    Json(serde_json::json!({
        "success": true,
        "idle_timeout_secs": timeout,
    }))
}

pub async fn update_session_config_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    if let Some(timeout) = body.get("idle_timeout_secs").and_then(Value::as_u64) {
        update_session_config(&state, timeout);
    }
    Json(serde_json::json!({"success": true}))
}
