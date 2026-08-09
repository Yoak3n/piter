//! WebSocket handler and message routing.
//!
//! Frontend uses `instance_id` for all session operations.
//! Real session file paths are internal to the backend.

mod helper;
mod broker;

pub use broker::{dispatch_control, send_get_messages, send_get_state};

use std::sync::Arc;

use axum::extract::ws;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::broker::types::PROTOCOL_VERSION;
use super::{GatewayState, helper::{notify_undeliverable, forward_to_instance}};
use helper::resolve_command_instance;


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
    let sessions = super::state::build_project_session_tree(&state);
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

    if msg_type == "broker_command" {
        broker::handler_broker_command(&state, text, &value, client_tx, client_id);
        return;
    }

    // ── Normal routing (by instanceId) ────────────────────────────────
    let Some(instance_id) = resolve_command_instance(&value, state) else {
        notify_undeliverable(client_tx, &value, "no_route");
        return;
    };

    forward_to_instance(text, &value, &instance_id, state, client_tx);
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
            let projects = super::state::build_project_session_tree(state);
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
                .filter(|s| s.activity != super::session_manager::SessionActivity::Unloaded)
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
        "fork_capability" => {
            // 撤回确认框需要知道该会话是否支持文件回滚（cwd 是否 git 仓库）。
            let iid = data.get("instanceId").and_then(Value::as_str).ok_or("missing instanceId")?;
            let cwd = state.session_manager.lock().sessions.get(iid).map(|s| s.cwd.clone());
            let rollback_available = cwd
                .map(|c| super::git::is_git_repo(std::path::Path::new(&c)))
                .unwrap_or(false);
            Ok(json!({ "rollbackAvailable": rollback_available }))
        }
        _ => Err(format!("unknown gateway command: {}", command)),
    }
}
