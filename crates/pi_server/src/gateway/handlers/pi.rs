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
use crate::gateway::GatewayState;

// ─── Shared logic (callable from WS) ───────────────────────────────────────

pub fn get_pi_status(state: &GatewayState) -> PiStatusResponse {
    let active_id = state.inner.active_instance.lock().clone();
    if let Some(ref id) = active_id {
        let instances = state.inner.instances.lock();
        if let Some(inst) = instances.get(id) {
            return PiStatusResponse {
                running: inst.running.load(Ordering::SeqCst),
                instance_id: Some(id.clone()),
                session_path: inst.session_path.clone(),
            };
        }
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
    let cwd = {
        let instances = state.inner.instances.lock();
        instances
            .get(instance_id)
            .map(|i| i.cwd.clone())
    };

    let Some(cwd) = cwd else {
        return Err("instance not found".into());
    };

    kill_instance_for_gateway(state, instance_id);

    // Restart: use empty extensions (project context would need to be looked up)
    let extensions = Vec::new();
    spawn_persistent_for_gateway(state, &cwd, &extensions)
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
        let instances = state.inner.instances.lock();
        if let Some(inst) = instances.get(&instance_id) {
            if let Some(stdin_tx) = &inst.stdin_tx {
                let _ = stdin_tx.send(command.to_string());
            }
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
        .map(|s| s.to_string())
        .or_else(|| state.inner.active_instance.lock().clone());

    let Some(instance_id) = instance_id else {
        return Json(serde_json::json!({"success": false, "error": "no instance specified"}));
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
        .map(|s| s.to_string())
        .or_else(|| state.inner.active_instance.lock().clone());

    let Some(instance_id) = instance_id else {
        return Json(serde_json::json!({"success": false, "error": "no instance specified"}));
    };

    if stop_pi_instance(&state, &instance_id) {
        log::info!("[gateway] pi instance {} stopped via API", instance_id);
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

// ─── RPC core logic ────────────────────────────────────────────────────────

async fn rpc_to_instance(state: &Arc<GatewayState>, mut body: Value) -> Json<Value> {
    let instance_id = body
        .get("instanceId")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let target_id = if let Some(ref id) = instance_id {
        if state.inner.instances.lock().contains_key(id) {
            Some(id.clone())
        } else {
            return Json(serde_json::json!({"success": false, "error": "instance not found"}));
        }
    } else {
        state.inner.active_instance.lock().clone()
    };

    let Some(target_id) = target_id else {
        return Json(serde_json::json!({"success": false, "error": "no active pi instance"}));
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
        let instances = state.inner.instances.lock();
        let Some(instance) = instances.get(&target_id) else {
            state.inner.pending_rpc.lock().remove(&request_id);
            return Json(serde_json::json!({"success": false, "error": "instance gone"}));
        };
        if let Some(tx) = &instance.stdin_tx {
            let cmd_str = body.to_string();
            if tx.send(cmd_str).is_err() {
                state.inner.pending_rpc.lock().remove(&request_id);
                return Json(serde_json::json!({"success": false, "error": "failed to send command"}));
            }
        }
    }

    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(response)) => Json(serde_json::to_value(response).unwrap_or_default()),
        Ok(Err(_)) => Json(serde_json::json!({"success": false, "error": "response channel closed"})),
        Err(_) => {
            state.inner.pending_rpc.lock().remove(&request_id);
            Json(serde_json::json!({"success": false, "error": "timeout waiting for pi response"}))
        }
    }
}

// ─── Instance management ───────────────────────────────────────────────────

pub fn spawn_persistent_for_gateway(
    state: &GatewayState,
    cwd: &str,
    extensions: &[String],
) -> Result<String, String> {
    let instance_id = state
        .spawn()
        .cwd(cwd)
        .extensions(extensions)
        .run()?;

    // Register instance_id as a route immediately (sessionFile/sessionId added later by get_state)
    state
        .inner
        .routes
        .lock()
        .insert(instance_id.clone(), instance_id.clone());

    {
        let mut active = state.inner.active_instance.lock();
        if active.is_none() {
            *active = Some(instance_id.clone());
        }
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

fn spawn_ephemeral_for_gateway(
    state: &GatewayState,
    cwd: &str,
) -> Result<String, String> {
    let id = state.spawn_ephemeral().cwd(cwd).run()?;
    log::info!("[gateway] ephemeral instance {} spawned", id);
    Ok(id)
}

fn kill_instance_for_gateway(state: &GatewayState, instance_id: &str) -> bool {
    let mut instances = state.inner.instances.lock();
    if let Some(mut inst) = instances.remove(instance_id) {
        inst.running.store(false, Ordering::SeqCst);
        let _ = inst.child.kill();

        if let Some(ref sp) = inst.session_path {
            state.inner.routes.lock().remove(sp);
        }

        {
            let mut active = state.inner.active_instance.lock();
            if active.as_deref() == Some(instance_id) {
                *active = instances
                    .iter()
                    .filter(|(_, i)| i.persistent)
                    .map(|(id, _)| id.clone())
                    .next();
            }
        }

        log::info!("[gateway] instance {} killed", instance_id);
        true
    } else {
        false
    }
}
