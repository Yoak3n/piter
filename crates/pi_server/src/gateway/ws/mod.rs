//! WebSocket handler and message routing.
//!
//! Frontend uses `instance_id` for all session operations.
//! Real session file paths are internal to the backend.

mod helper;


use std::sync::Arc;

use axum::extract::ws;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use super::GatewayState;
use crate::broker::types::PROTOCOL_VERSION;
use helper::{extract_cwd, resolve_command_instance};

pub async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: ws::WebSocket, state: Arc<GatewayState>) {
    let client_id = state
        .inner
        .next_client_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();

    state.ui_clients.lock().insert(client_id, client_tx.clone());

    // Capability handshake
    let _ = client_tx.send(
        json!({
            "type": "capabilities",
            "protocolVersion": PROTOCOL_VERSION,
            "client_id": client_id,
        })
        .to_string(),
    );

    // Send sessions list (DB-backed, auto-link already done at DB open)
    let sessions = super::build_project_session_tree(&state);
    if let Ok(json) = serde_json::to_string(&sessions) {
        let _ = client_tx.send(format!(r#"{{"type":"sessions_list","projects":{}}}"#, json));
    }

    let send_task = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            if ws_tx.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(ws::Message::Text(text))) => {
                        route_ui_message(&text, &state, &client_tx, client_id);
                    }
                    Some(Ok(ws::Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
        }
    }

    state.ui_clients.lock().remove(&client_id);
    state.session_manager.lock().deactivate_all_for_client(client_id);
    send_task.abort();
}

// ─── Message Routing ────────────────────────────────────────────────────────

fn route_ui_message(
    text: &str,
    state: &Arc<GatewayState>,
    client_tx: &mpsc::UnboundedSender<String>,
    client_id: u64,
) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        return;
    };
    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");

    // ── broker_control ──
    if msg_type == "broker_control" {
        let value = value.clone();
        let client_tx = client_tx.clone();
        tokio::spawn(async move { dispatch_control(value, &client_tx).await });
        return;
    }

    // ── gateway_command ──
    if msg_type == "gateway_command" {
        let request_id = value.get("requestId").and_then(Value::as_str).unwrap_or("").to_string();
        let command = value.get("command").and_then(Value::as_str).unwrap_or("");
        let data = value.get("data").cloned().unwrap_or(Value::Null);
        let result = dispatch_gateway_command(command, &data, state);
        let response = match result {
            Ok(d) => json!({"type": "gateway_response", "requestId": request_id, "success": true, "data": d}),
            Err(e) => json!({"type": "gateway_response", "requestId": request_id, "success": false, "error": e}),
        };
        let _ = client_tx.send(response.to_string());
        return;
    }

    let effective_type = if msg_type == "broker_command" {
        value.pointer("/payload/type").and_then(Value::as_str).unwrap_or("")
    } else {
        msg_type
    };

    // ── new_session: spawn pi, return instanceId ──────────────────────
    if effective_type == "new_session" {
        let Some(cwd) = extract_cwd(&value) else {
            notify_undeliverable(client_tx, &value, "missing_or_invalid_cwd");
            return;
        };
        let name = value.pointer("/payload/name").and_then(Value::as_str)
            .or_else(|| value.get("name").and_then(Value::as_str))
            .unwrap_or("New Project");

        match super::session_manager::SessionManager::create_session(
            &state.session_manager, state, &cwd, name, client_id,
        ) {
            Ok(instance_id) => {
                // Immediately push updated sessions list
                super::push_sessions_list_to_clients(state);

                // Fire-and-forget get_state so we learn sessionId/sessionFile/model ASAP.
                // The response is handled by the event loop (mod.rs §1c).
                {
                    let instances = state.inner.instances.lock();
                    if let Some(inst) = instances.get(&instance_id) {
                        if let Some(tx) = &inst.stdin_tx {
                            let _ = tx.send(serde_json::json!({"type": "get_state"}).to_string());
                        }
                    }
                }

                // Send snapshot (empty for new session)
                let snapshot = json!({
                    "type": "session_snapshot",
                    "instanceId": instance_id,
                    "messages": [],
                    "messageSeq": 0,
                });
                let _ = client_tx.send(snapshot.to_string());
                // Also send the new_session response with instanceId
                let _ = client_tx.send(json!({
                    "type": "response",
                    "command": "new_session",
                    "success": true,
                    "instanceId": instance_id,
                }).to_string());
            }
            Err(e) => {
                log::error!("[gateway] create_session failed: {}", e);
                notify_undeliverable(client_tx, &value, "session_create_failed");
            }
        }
        return;
    }

    // ── switch_session: by instanceId ────────────────────────────────
    if effective_type == "switch_session" {
        let iid = value
            .get("instanceId")
            .and_then(Value::as_str)
            .or_else(|| value.pointer("/payload/instanceId").and_then(Value::as_str));

        log::debug!("[gateway] switch_session: raw value={}", value);
        log::debug!("[gateway] switch_session: resolved iid={:?}", iid);

        let Some(iid) = iid else {
            log::warn!("[gateway] switch_session: missing_instanceId");
            notify_undeliverable(client_tx, &value, "missing_instanceId");
            return;
        };

        let result = super::session_manager::SessionManager::switch_session(
            &state.session_manager, iid, client_id,
        );

        match result {
            super::session_manager::SessionResult::Switched {
                instance_id, messages, message_seq, ..
            } => {
                log::debug!("[gateway] switch_session: Switched to {}", instance_id);
                send_snapshot(client_tx, &instance_id, &messages, message_seq);
                // Forward switch_session to pi
                let (text, value, state, client_tx) = (text.to_string(), value.clone(), state.clone(), client_tx.clone());
                tokio::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                    forward_to_instance(&text, &value, &instance_id, &state, &client_tx);
                });
            }
            super::session_manager::SessionResult::NeedSpawn { .. } => {
                // Session exists in DB but not running — spawn with persisted instance_id
                log::info!("[gateway] switch_session: instance {} not running, spawning", iid);
                // Get cwd and session_path from DB
                let db_session = state.db.all_sessions().into_iter()
                    .find(|s| s.instance_id == iid);
                let cwd = extract_cwd(&value).or_else(|| db_session.as_ref().map(|s| s.cwd.clone()));
                let Some(cwd) = cwd else {
                    log::warn!("[gateway] switch_session: no cwd for instance {}", iid);
                    notify_undeliverable(client_tx, &value, "missing_cwd");
                    return;
                };
                let session_path = db_session.and_then(|s| s.session_path);
                let extensions = super::project::resolve_project_extensions(&state.db, &cwd, &cwd);
                // Load existing messages from session file (if it exists)
                let existing_messages: Vec<Value> = session_path.as_ref()
                    .map(|sp| super::handlers::session::load_session(sp))
                    .unwrap_or_default();
                let msg_seq = existing_messages.len() as u64;
                // Reuse the persisted instance_id, resume existing session file
                let mut builder = state.spawn().cwd(&cwd).extensions(&extensions).id(iid);
                if let Some(ref sp) = session_path {
                    builder = builder.session_path(sp);
                }
                match builder.run() {
                    Ok(new_iid) => {
                        // Register in routing table
                        state.inner.routes.lock().insert(new_iid.clone(), new_iid.clone());
                        // Register in session manager with existing messages
                        super::session_manager::SessionManager::register_instance(
                            &state.session_manager, &new_iid, &cwd, client_id,
                        );
                        // Inject loaded messages into the managed session
                        {
                            let mut mgr = state.session_manager.lock();
                            if let Some(session) = mgr.sessions.get_mut(&new_iid) {
                                session.messages = existing_messages.clone();
                                session.message_seq = msg_seq;
                            }
                        }
                        // Immediately push updated sessions list
                        super::push_sessions_list_to_clients(state);
                        // Tell frontend the instance is ready with loaded messages
                        let snapshot = serde_json::json!({
                            "type": "session_snapshot",
                            "instanceId": new_iid,
                            "messages": existing_messages,
                            "messageSeq": msg_seq,
                        });
                        let _ = client_tx.send(snapshot.to_string());
                        // Forward switch_session after pi starts
                        let (text, value, state, client_tx) = (text.to_string(), value.clone(), state.clone(), client_tx.clone());
                        tokio::spawn(async move {
                            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
                            forward_to_instance(&text, &value, &new_iid, &state, &client_tx);
                        });
                    }
                    Err(e) => {
                        log::error!("[gateway] spawn for switch_session failed: {}", e);
                        notify_undeliverable(client_tx, &value, "spawn_failed");
                    }
                }
            }
        }
        return;
    }

    // ── Normal routing (by instanceId) ────────────────────────────────
    let Some(instance_id) = resolve_command_instance(&value, state) else {
        log::warn!("[gateway] no route for command: {}", effective_type);
        notify_undeliverable(client_tx, &value, "no_route");
        return;
    };

    forward_to_instance(text, &value, &instance_id, state, client_tx);
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn send_snapshot(
    client_tx: &mpsc::UnboundedSender<String>,
    instance_id: &str,
    messages: &[Value],
    message_seq: u64,
) {
    let msg = json!({
        "type": "session_snapshot",
        "instanceId": instance_id,
        "messages": messages,
        "messageSeq": message_seq,
    });
    log::info!("[gateway] send_snapshot: iid={}, msgs={}, seq={}", instance_id, messages.len(), message_seq);
    if client_tx.send(msg.to_string()).is_err() {
        log::warn!("[gateway] send_snapshot: client_tx send FAILED — channel closed");
    }
}

fn forward_to_instance(
    text: &str,
    value: &Value,
    instance_id: &str,
    state: &Arc<GatewayState>,
    client_tx: &mpsc::UnboundedSender<String>,
) {
    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let forward_text = if msg_type == "broker_command" {
        value.get("payload")
            .and_then(|p| serde_json::to_string(p).ok())
            .unwrap_or_else(|| text.to_string())
    } else {
        text.to_string()
    };

    let instances = state.inner.instances.lock();
    if let Some(instance) = instances.get(instance_id) {
        if let Some(tx) = &instance.stdin_tx {
            if tx.send(forward_text).is_err() {
                drop(instances);
                notify_undeliverable(client_tx, value, "upstream_unavailable");
            }
        }
    } else {
        drop(instances);
        notify_undeliverable(client_tx, value, "upstream_unavailable");
    }
}

fn notify_undeliverable(client_tx: &mpsc::UnboundedSender<String>, value: &Value, reason: &str) {
    let request_id = value.get("id").and_then(Value::as_str).unwrap_or("");
    let command = value
        .pointer("/payload/type")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or("");
    let _ = client_tx.send(json!({
        "type": "command_undeliverable",
        "protocolVersion": PROTOCOL_VERSION,
        "requestId": request_id,
        "command": command,
        "reason": reason,
    }).to_string());
}

async fn dispatch_control(value: Value, client_tx: &mpsc::UnboundedSender<String>) {
    let request_id = value.get("requestId").and_then(Value::as_str).unwrap_or("").to_string();
    let command = value.get("command").and_then(Value::as_str).unwrap_or("").to_string();

    let response = match command.as_str() {
        "ping" => json!({"type": "control_response", "requestId": request_id, "ok": true, "result": {"pong": true}}),
        "info" => json!({"type": "control_response", "requestId": request_id, "ok": true, "result": {
            "version": env!("CARGO_PKG_VERSION"),
            "features": ["rpc", "ws", "lan", "health", "multi_instance"],
        }}),
        _ => json!({"type": "control_response", "requestId": request_id, "ok": false, "error": format!("Unknown command: {}", command)}),
    };

    let _ = client_tx.send(response.to_string());
}




// ─── Gateway Command Dispatch ───────────────────────────────────────────────

fn dispatch_gateway_command(
    command: &str,
    data: &Value,
    state: &Arc<GatewayState>,
) -> Result<Value, String> {
    match command {
        "list_projects" => {
            let archived = data.get("archived").and_then(Value::as_bool).unwrap_or(false);
            Ok(json!({ "projects": super::handlers::project::list_projects(&state.db, archived) }))
        }
        "create_project" => {
            let name = data.get("name").and_then(Value::as_str).ok_or("missing name")?;
            let cwd = data.get("cwd").and_then(Value::as_str).ok_or("missing cwd")?;
            let exts = data.get("extensions").and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect())
                .unwrap_or_default();
            Ok(json!({ "project": super::handlers::project::create_project(&state.db, name, cwd, exts)? }))
        }
        "update_project" => {
            let id = data.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let name = data.get("name").and_then(Value::as_str);
            let exts = data.get("extensions").and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|v| v.as_str().map(|s| s.to_string())).collect());
            Ok(json!({ "project": super::handlers::project::update_project(&state.db, id, name, exts)? }))
        }
        "delete_project" => {
            let id = data.get("id").and_then(Value::as_str).ok_or("missing id")?;
            super::handlers::project::delete_project(&state.db, id)?;
            Ok(json!({}))
        }
        "pin_project" => {
            let id = data.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let pinned = data.get("pinned").and_then(Value::as_i64).unwrap_or(1) as i32;
            super::handlers::project::pin_project(&state.db, id, pinned)?;
            Ok(json!({}))
        }
        "archive_project" => {
            let id = data.get("id").and_then(Value::as_str).ok_or("missing id")?;
            let archived = data.get("archived").and_then(Value::as_bool).unwrap_or(true);
            super::handlers::project::archive_project(&state.db, id, archived)?;
            Ok(json!({}))
        }
        "list_sessions" => {
            let projects = super::build_project_session_tree(state);
            Ok(serde_json::json!({ "projects": projects }))
        }
        "delete_session" => {
            let iid = data.get("instanceId").or_else(|| data.get("path")).and_then(Value::as_str).ok_or("missing instanceId")?;
            super::handlers::session::delete_session(iid, state)?;
            Ok(json!({}))
        }
        "rename_session" => {
            let path = data.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let name = data.get("name").and_then(Value::as_str).ok_or("missing name")?;
            super::handlers::session::rename_session(path, name)?;
            Ok(json!({}))
        }
        "get_messages" => {
            let iid = data.get("instanceId").and_then(Value::as_str).ok_or("missing instanceId")?;
            let mgr = state.session_manager.lock();
            let session = mgr.sessions.get(iid).ok_or("instance not found")?;
            let mut msgs = session.messages.clone();
            // Include partial message if streaming
            if let Some(ref partial) = session.partial_message {
                msgs.push(partial.clone());
            }
            Ok(json!({
                "instanceId": iid,
                "messages": msgs,
                "messageSeq": session.message_seq,
            }))
        }
        "get_active_sessions" => {
            let mgr = state.session_manager.lock();
            let active: Vec<_> = mgr.sessions.values()
                .filter(|s| matches!(s.state, super::session_manager::SessionState::Active | super::session_manager::SessionState::Idle { .. }))
                .map(|s| json!({
                    "instanceId": s.instance_id,
                    "cwd": s.cwd,
                    "messageCount": s.messages.len(),
                    "messageSeq": s.message_seq,
                    "hasSubscribers": !s.subscribers.is_empty(),
                    "piState": s.pi_state.as_ref().map(|ps| serde_json::to_value(ps).unwrap_or_default()),
                }))
                .collect();
            Ok(json!({ "sessions": active }))
        }
        "get_health" => {
            Ok(serde_json::to_value(super::handlers::system::get_health(state)).unwrap_or_default())
        }
        "get_lan_info" => {
            Ok(serde_json::to_value(super::handlers::system::get_lan_info(state)).unwrap_or_default())
        }
        _ => Err(format!("unknown gateway command: {}", command)),
    }
}
