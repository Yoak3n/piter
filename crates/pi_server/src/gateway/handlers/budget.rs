//! Monthly budget REST handlers.
//!
//! - `GET /api/budget`        → `{ success, budgetCents, resetDay, enabled }`
//! - `PUT /api/budget`        → body `{ budget_cents, reset_day, enabled }`
//! - `GET /api/budget/status` → `{ used, budget, percent, tier, resetDay,
//!                                 cycleStart, cycleEnd }` (camelCase)

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::response::Json;
use serde_json::Value;

use crate::budget;
use crate::gateway::GatewayState;

pub async fn get_budget_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let cfg = state.db.get_budget_config();
    Json(serde_json::json!({
        "success": true,
        "budgetCents": cfg.budget_cents,
        "resetDay": cfg.reset_day,
        "enabled": cfg.enabled,
    }))
}

pub async fn put_budget_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let Some(budget_cents) = body.get("budget_cents").and_then(Value::as_i64) else {
        return Json(serde_json::json!({"success": false, "error": "missing budget_cents"}));
    };
    let reset_day = body.get("reset_day").and_then(Value::as_i64).unwrap_or(1);
    let enabled = body.get("enabled").and_then(Value::as_bool).unwrap_or(false);

    match state.db.set_budget_config(budget_cents, reset_day, enabled) {
        Ok(()) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn budget_status_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let cfg = state.db.get_budget_config();
    // Only piter-managed sessions count (mirrors the stats dashboard).
    let files: Vec<PathBuf> = state
        .db
        .all_sessions()
        .into_iter()
        .filter_map(|s| s.session_path)
        .map(PathBuf::from)
        .collect();

    // Aggregation parses every session file — keep it off the async runtime.
    let result = tokio::task::spawn_blocking(move || {
        budget::budget_status(files, cfg.budget_cents, cfg.reset_day as u32, cfg.enabled)
    })
    .await
    .map_err(|e| format!("budget task join error: {e}"));

    match result {
        Ok(Ok(status)) => match serde_json::to_value(status) {
            Ok(v) => Json(v),
            Err(e) => Json(serde_json::json!({"error": format!("serialize budget: {e}")})),
        },
        Ok(Err(e)) => Json(serde_json::json!({"error": e})),
        Err(e) => Json(serde_json::json!({"error": e})),
    }
}
