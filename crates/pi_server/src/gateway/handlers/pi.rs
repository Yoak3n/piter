//! Pi control handlers: RPC, ephemeral, status, restart, stop, settings.
//! Also contains instance management helpers (spawn, kill).

use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::response::Json;
use serde_json::Value;
use uuid::Uuid;

use super::PiStatusResponse;
use crate::broker::types::PendingRpc;
use crate::gateway::broadcast::push_sessions_list_to_clients;
use crate::gateway::GatewayState;

use crate::gateway::session_manager::SessionActivity;

// Commands allowed to run without an explicit instanceId (no-arg fallback).
const RPC_FALLBACK_WHITELIST: &[&str] = &[
    "get_available_models",
    "get_state",
    "set_model",
    "cycle_model",
];

pub fn get_pi_status(state: &GatewayState) -> PiStatusResponse {
    let instances = state.inner.instances.lock();
    if let Some((id, inst)) = instances.iter().next() {
        return PiStatusResponse {
            running: inst.running.load(Ordering::SeqCst),
            instance_id: Some(id.clone()),
            session_path: inst.session_path.clone(),
        };
    }
    PiStatusResponse {
        running: false,
        instance_id: None,
        session_path: None,
    }
}

pub fn get_pi_settings() -> Result<serde_json::Value, String> {
    match crate::broker::util::read_pi_settings() {
        Ok(settings) => Ok(serde_json::json!({
            "default_provider": settings.default_provider,
            "default_model": settings.default_model,
            "default_thinking_level": settings.default_thinking_level,
            "packages": settings.packages,
        })),
        Err(e) => Err(e),
    }
}

pub fn stop_pi_instance(state: &GatewayState, instance_id: &str) -> bool {
    kill_instance_for_gateway(state, instance_id)
}

pub fn restart_pi_instance(state: &GatewayState, instance_id: &str) -> Result<String, String> {
    // Gather all state from memory before killing the process
    let (cwd, instance_session_path, model_id, model_provider, session_file) = {
        let instances = state.inner.instances.lock();
        let mgr = state.session_manager.lock();
        let inst = instances.get(instance_id);
        let ps = mgr.sessions.get(instance_id).and_then(|s| s.pi_state.as_ref());
        (
            inst.map(|i| i.cwd.clone()),
            inst.and_then(|i| i.session_path.clone()),
            ps.and_then(|p| p.model_id.clone()),
            ps.and_then(|p| p.model_provider.clone()),
            ps.and_then(|p| p.session_file.clone()),
        )
    };

    let Some(cwd) = cwd else {
        return Err("instance not found".into());
    };

    // Resolve the effective extension whitelist for this project (global ∪
    // project − excluded). Fall back to global-only when no project is linked.
    let project_id = state
        .db
        .get_session_project(instance_id)
        .or_else(|| {
            state
                .db
                .list_projects(true)
                .into_iter()
                .find(|p| p.cwd == cwd)
                .map(|p| p.id)
        });
    let extensions = match project_id {
        Some(pid) => {
            crate::gateway::project::effective_project_extensions(&state.db, &pid, &cwd)
        }
        None => crate::gateway::project::effective_global_extensions(&state.db, &cwd),
    };
    let effective_session_path = instance_session_path.or(session_file);
    let model_str = format_model_arg(&model_id, &model_provider);

    kill_instance_for_gateway(state, instance_id);

    resume_session(
        state,
        instance_id,
        &cwd,
        effective_session_path.as_deref(),
        model_str.as_deref(),
        &extensions,
    )
}

// ─── REST handlers ──────────────────────────────────────────────────────────

pub async fn rpc_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<Value>,
) -> Json<Value> {
    rpc_to_instance(&state, body).await
}

pub async fn rpc_ephemeral_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<Value>,
) -> Json<Value> {
    let cwd = body.get("cwd").and_then(|v| v.as_str()).unwrap_or(".");

    let instance_id = match spawn_ephemeral_for_gateway(&state, cwd) {
        Ok(id) => id,
        Err(e) => return Json(serde_json::json!({"success": false, "error": e})),
    };

    let mut command = body.get("command").cloned().unwrap_or(body.clone());

    let request_id = command
        .get("id")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let id = Uuid::new_v4().to_string();
            command["id"] = Value::String(id.clone());
            id
        });

    let (tx, rx) = tokio::sync::oneshot::channel();
    state.inner.pending_rpc.lock().insert(
        request_id.clone(),
        PendingRpc { sender: tx },
    );

    {
        if let Some(tx) = state.instance_stdin_tx(&instance_id) {
            let _ = tx.send(command.to_string());
        }
    }

    let result = match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(response)) => Json(serde_json::to_value(response).unwrap_or_default()),
        Ok(Err(_)) => Json(serde_json::json!({"success": false, "error": "response channel closed"})),
        Err(_) => {
            state.inner.pending_rpc.lock().remove(&request_id);
            Json(serde_json::json!({"success": false, "error": "timeout waiting for pi response"}))
        }
    };

    kill_instance_for_gateway(&state, &instance_id);
    result
}

pub async fn pi_status_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<PiStatusResponse> {
    Json(get_pi_status(&state))
}

pub async fn pi_restart_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let instance_id = body
        .get("instanceId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let Some(instance_id) = instance_id else {
        return Json(serde_json::json!({"success": false, "error": "instanceId required"}));
    };

    match restart_pi_instance(&state, &instance_id) {
        Ok(id) => Json(serde_json::json!({"success": true, "instanceId": id})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

pub async fn pi_stop_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let instance_id = body
        .get("instanceId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let Some(instance_id) = instance_id else {
        return Json(serde_json::json!({"success": false, "error": "instanceId required"}));
    };

    if stop_pi_instance(&state, &instance_id) {
        log::info!("[gateway] pi instance {} stopped via API", instance_id);
        // Mark the session unloaded (process is gone) and push the latest
        // sessions list so clients see the stopped state immediately.
        state
            .session_manager
            .lock()
            .mark_unloaded(std::slice::from_ref(&instance_id));
        push_sessions_list_to_clients(&state);
        Json(serde_json::json!({"success": true}))
    } else {
        Json(serde_json::json!({"success": false, "error": "instance not found"}))
    }
}

pub async fn pi_settings_handler() -> Json<serde_json::Value> {
    match get_pi_settings() {
        Ok(settings) => {
            let mut map = serde_json::json!({"success": true});
            if let Some(obj) = settings.as_object() {
                for (k, v) in obj {
                    map[k] = v.clone();
                }
            }
            Json(map)
        }
        Err(e) => Json(serde_json::json!({"success": false, "error": e})),
    }
}

/// Model capability catalog: read pi's persisted model store from disk and
/// return `{ success, models: [{ id, provider, input }] }`. Works without a
/// running pi process so the frontend can warm its vision-capability registry
/// immediately at startup.
pub async fn pi_model_catalog_handler() -> Json<serde_json::Value> {
    match crate::broker::util::read_pi_model_catalog() {
        Ok(models) => Json(serde_json::json!({ "success": true, "models": models })),
        Err(e) => Json(serde_json::json!({ "success": false, "error": e })),
    }
}

// ─── RPC core logic ────────────────────────────────────────────────────────

async fn rpc_to_instance(state: &Arc<GatewayState>, mut body: Value) -> Json<Value> {
    let instance_id = body
        .get("instanceId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    // Resolve target: explicit instanceId → any running instance → spawn ephemeral
    let (target_id, ephemeral) = if let Some(ref id) = instance_id {
        if state.inner.instances.lock().contains_key(id) {
            (id.clone(), false)
        } else {
            // If an instance is specified but not actually running, an error is returned
            return Json(serde_json::json!({"success": false, "error": "instance not found"}));
        }
    } else {
        // No instanceId — check whitelist first
        let cmd_type = body.get("type").and_then(Value::as_str).unwrap_or("");
        if !RPC_FALLBACK_WHITELIST.contains(&cmd_type) {
            return Json(serde_json::json!({
                "success": false,
                "error": format!("instanceId required for '{}' command", cmd_type)
            }));
        }

        // Prefer an Idle instance; fall back to any non-Unloaded; spawn ephemeral as last resort
        let preferred = {
            let mgr = state.session_manager.lock();
            let instances = state.inner.instances.lock();
            // First pass: prefer Idle activity (pi is not busy)
            let idle = mgr.sessions.iter()
                .find(|(_, s)| s.activity == SessionActivity::Idle && instances.contains_key(&s.instance_id))
                .map(|(id, _)| id.clone());
            idle.or_else(|| {
                // Second pass: any non-Unloaded instance
                mgr.sessions.iter()
                    .find(|(_, s)| s.activity != SessionActivity::Unloaded && instances.contains_key(&s.instance_id))
                    .map(|(id, _)| id.clone())
            })
        };

        if let Some(id) = preferred {
            log::info!("[gateway] rpc: no instanceId, reusing instance {}", id);
            (id, false)
        } else {
            // No suitable instances — spawn ephemeral
            match spawn_ephemeral_for_gateway(state, ".") {
                Ok(id) => {
                    log::info!("[gateway] rpc: no instanceId, spawned ephemeral {}", id);
                    (id, true)
                }
                Err(e) => return Json(serde_json::json!({"success": false, "error": e})),
            }
        }
    };

    let request_id = body
        .get("id")
        .and_then(Value::as_str)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            let id = Uuid::new_v4().to_string();
            body["id"] = Value::String(id.clone());
            id
        });

    let (tx, rx) = tokio::sync::oneshot::channel();
    state.inner.pending_rpc.lock().insert(
        request_id.clone(),
        PendingRpc { sender: tx },
    );

    {
        let Some(tx) = state.instance_stdin_tx(&target_id) else {
            state.inner.pending_rpc.lock().remove(&request_id);
            if ephemeral { kill_instance_for_gateway(&state, &target_id); }
            return Json(serde_json::json!({"success": false, "error": "instance gone"}));
        };
        if tx.send(body.to_string()).is_err() {
            state.inner.pending_rpc.lock().remove(&request_id);
            if ephemeral { kill_instance_for_gateway(&state, &target_id); }
            return Json(serde_json::json!({"success": false, "error": "failed to send command"}));
        }
    }

    let result = match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(response)) => Json(serde_json::to_value(response).unwrap_or_default()),
        Ok(Err(_)) => Json(serde_json::json!({"success": false, "error": "response channel closed"})),
        Err(_) => {
            state.inner.pending_rpc.lock().remove(&request_id);
            Json(serde_json::json!({"success": false, "error": "timeout waiting for pi response"}))
        }
    };

    if ephemeral { kill_instance_for_gateway(&state, &target_id); }
    result
}

// ─── Instance management ───────────────────────────────────────────────────

/// Format model_id + model_provider into "provider/id" for `--model` CLI arg.
pub fn format_model_arg(model_id: &Option<String>, model_provider: &Option<String>) -> Option<String> {
    match (model_id, model_provider) {
        (Some(id), Some(provider)) => Some(format!("{}/{}", provider, id)),
        (Some(id), None) => Some(id.clone()),
        _ => None,
    }
}

pub fn spawn_persistent_for_gateway(
    state: &GatewayState,
    cwd: &str,
    extensions: &[String],
    model: Option<&str>,
) -> Result<String, String> {
    let mut builder = state
        .spawn()
        .cwd(cwd)
        .extensions(extensions);
    if let Some(m) = model {
        builder = builder.model(m);
    }
    let instance_id = builder.run()?;

    {
    // Register instance_id as a route immediately (sessionFile/sessionId added later by get_state)
        state
            .inner
            .routes
            .lock()
            .insert(instance_id.clone(), instance_id.clone());
    }

    log::info!(
        "[gateway] persistent instance {} spawned (cwd={})",
        instance_id, cwd
    );

    // Notify clients that a new pi instance started
    let pi_started = serde_json::json!({
        "type": "pi_started",
        "instanceId": instance_id,
        "cwd": cwd,
    });
    let _ = state.event_tx.send(pi_started.to_string());

    Ok(instance_id)
}

/// Resume an existing session: spawn pi with the persisted instance_id, session file, and model.
/// Used by `restart_pi_instance` and `switch_session` (NeedSpawn path).
pub fn resume_session(
    state: &GatewayState,
    instance_id: &str,
    cwd: &str,
    session_path: Option<&str>,
    model: Option<&str>,
    extensions: &[String],
) -> Result<String, String> {
    let mut builder = state
        .spawn()
        .cwd(cwd)
        .extensions(extensions)
        .id(instance_id);
    if let Some(sp) = session_path {
        builder = builder.session_path(sp);
    }
    if let Some(m) = model {
        builder = builder.model(m);
    }
    builder.run()
}

fn spawn_ephemeral_for_gateway(
    state: &GatewayState,
    cwd: &str,
) -> Result<String, String> {
    let id = state.spawn_ephemeral().cwd(cwd).run()?;
    log::info!("[gateway] ephemeral instance {} spawned", id);
    Ok(id)
}

pub fn kill_instance_for_gateway(state: &GatewayState, instance_id: &str) -> bool {
    let mut instances = state.inner.instances.lock();
    if let Some(mut inst) = instances.remove(instance_id) {
        inst.running.store(false, Ordering::SeqCst);
        let _ = inst.child.kill();

        // Remove all route entries pointing to this instance
        let mut routes = state.inner.routes.lock();
        routes.retain(|_, v| v != instance_id);
        drop(routes);

        log::info!("[gateway] instance {} killed", instance_id);
        true
    } else {
        false
    }
}
