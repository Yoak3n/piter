//! Project handlers: CRUD, pin, archive.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path as AxumPath, Query};
use axum::response::Json;
use serde_json::Value;

use crate::gateway::db::Db;
use crate::gateway::project::Project;
use crate::gateway::GatewayState;

// ─── Shared logic (callable from WS) ───────────────────────────────────────

pub fn list_projects(db: &Db, include_archived: bool) -> Vec<Project> {
    crate::gateway::project::list_projects(db, include_archived)
}

pub fn create_project(
    db: &Db,
    name: &str,
    cwd: &str,
    extensions: Vec<String>,
) -> Result<Project, String> {
    crate::gateway::project::create_project(db, name, cwd, extensions)
}

pub fn update_project(
    db: &Db,
    id: &str,
    name: Option<&str>,
    extensions: Option<Vec<String>>,
) -> Result<Project, String> {
    crate::gateway::project::update_project(db, id, name, extensions)
}

pub fn delete_project(db: &Db, id: &str) -> Result<(), String> {
    crate::gateway::project::delete_project(db, id)
}

pub fn pin_project(db: &Db, id: &str, pinned: i32) -> Result<(), String> {
    db.set_pinned(id, pinned)
}

pub fn archive_project(db: &Db, id: &str, archived: bool) -> Result<(), String> {
    db.set_archived(id, archived)
}

// ─── REST handlers ──────────────────────────────────────────────────────────

pub async fn projects_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    Query(params): Query<HashMap<String, String>>,
) -> Json<Value> {
    let include_archived = params.get("archived").map(|v| v == "true").unwrap_or(false);
    let projects = list_projects(&state.db, include_archived);
    Json(serde_json::json!({ "success": true, "projects": projects }))
}

pub async fn create_project_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Json(serde_json::json!({"success": false, "error": "missing name"})),
    };
    let cwd = match body.get("cwd").and_then(|v| v.as_str()) {
        Some(c) => c,
        None => return Json(serde_json::json!({"success": false, "error": "missing cwd"})),
    };
    let extensions = body
        .get("extensions")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
        .unwrap_or_default();

    match create_project(&state.db, name, cwd, extensions) {
        Ok(project) => Json(serde_json::json!({"success": true, "project": project})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn update_project_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::extract::Path(id): AxumPath<String>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let name = body.get("name").and_then(|v| v.as_str());
    let extensions = body
        .get("extensions")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect::<Vec<_>>());

    match update_project(&state.db, &id, name, extensions) {
        Ok(project) => Json(serde_json::json!({"success": true, "project": project})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn delete_project_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::extract::Path(id): AxumPath<String>,
) -> Json<Value> {
    match delete_project(&state.db, &id) {
        Ok(()) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn pin_project_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::extract::Path(id): AxumPath<String>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let pinned = body.get("pinned").and_then(Value::as_i64).unwrap_or(1) as i32;
    match pin_project(&state.db, &id, pinned) {
        Ok(()) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn archive_project_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::extract::Path(id): AxumPath<String>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let archived = body.get("archived").and_then(Value::as_bool).unwrap_or(true);
    match archive_project(&state.db, &id, archived) {
        Ok(()) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}
