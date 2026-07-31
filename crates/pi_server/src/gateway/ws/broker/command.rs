use serde_json::{Value, json};
use tokio::sync::mpsc;

use super::super::{
    notify_undeliverable, forward_to_instance,
    helper::message::send_snapshot,
};
use crate::{
    GatewayState,
    gateway::{
        broadcast::push_sessions_list_to_clients, command, 
        session_manager::{SessionManager, SessionResult, SessionActivity},
        ws::helper::extract_cwd,
        handlers::session::load_session,
        project::resolve_project_extensions
    },
};

pub fn handler_broker_command(
    state: &GatewayState,
    raw_text: &str,
    value: &Value,
    client_tx: &mpsc::UnboundedSender<String>,
    client_id: u64,
) {
    let effective_type = value
        .pointer("/payload/type")
        .and_then(Value::as_str)
        .unwrap_or("");
    match effective_type {
        "new_session" => handle_new_session(state, value, client_tx, client_id),
        "switch_session" => handle_switch_session(state, raw_text, value, client_tx, client_id),
        "ack_review" => handle_ack_review(state, value, client_id),
        _ => {
            // Forward any other command (prompt, steer, etc.) to the target instance
            let iid = value
                .get("instanceId")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !iid.is_empty() {
                forward_to_instance(raw_text, value, iid, state, client_tx);
            } else {
                log::warn!("[gateway] broker_command: no instanceId for '{}'", effective_type);
                notify_undeliverable(client_tx, value, "missing_instanceId");
            }
        }
    }
}

/// When the frontend acknowledges a review (user switched to session or is viewing it),
/// transition WaitingReview → Idle so the session is available for RPC fallback.
fn handle_ack_review(
    state: &GatewayState,
    value: &Value,
    client_id: u64,
) {
    let iid = value
        .get("instanceId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if iid.is_empty() { return; }

    let mut mgr = state.session_manager.lock();
    if let Some(session) = mgr.sessions.get_mut(iid) {
        // Also register this client as a subscriber
        session.subscribers.insert(client_id);
        session.disconnected_since = None;

        if session.activity == SessionActivity::WaitingReview {
            session.activity = SessionActivity::Idle;
            mgr.mark_dirty();
            log::info!("[gateway] ack_review: session {} → Idle", iid);
        }
    }
}

fn handle_new_session(
    state: &GatewayState,
    value: &Value,
    client_tx: &mpsc::UnboundedSender<String>,
    client_id: u64,
) {
    let Some(cwd) = extract_cwd(&value) else {
        notify_undeliverable(client_tx, &value, "missing_or_invalid_cwd");
        return;
    };
    let name = value
        .pointer("/payload/name")
        .and_then(Value::as_str)
        .or_else(|| value.get("name").and_then(Value::as_str))
        .unwrap_or("New Project");

    // Extract model from payload: {id, provider} → "provider/id"
    let model_str = value
        .pointer("/payload/model")
        .and_then(|m| {
            let id = m.get("id").and_then(Value::as_str)?;
            let provider = m.get("provider").and_then(Value::as_str).unwrap_or("");
            if provider.is_empty() {
                Some(id.to_string())
            } else {
                Some(format!("{}/{}", provider, id))
            }
        });
    let model_ref = model_str.as_deref();

    match SessionManager::create_session(&state.session_manager, state, &cwd, name, client_id, model_ref) {
        Ok(instance_id) => {
            // Immediately push updated sessions list
            push_sessions_list_to_clients(state);

            // Fire-and-forget get_state so we learn sessionId/sessionFile/model ASAP.
            // The response is handled by the event loop (mod.rs §1c).
            command::send_get_state(state, &instance_id);

            // Send snapshot (empty for new session)
            let snapshot = json!({
                "type": "session_snapshot",
                "instanceId": instance_id,
                "messages": [],
                "messageSeq": 0,
            });
            let _ = client_tx.send(snapshot.to_string());
            // Also send the new_session response with instanceId
            let _ = client_tx.send(
                json!({
                    "type": "response",
                    "command": "new_session",
                    "success": true,
                    "instanceId": instance_id,
                })
                .to_string(),
            );
        }
        Err(e) => {
            log::error!("[gateway] create_session failed: {}", e);
            notify_undeliverable(client_tx, &value, "session_create_failed");
        }
    }
    return;
}

fn handle_switch_session(
    state: &GatewayState,
    raw_text: &str,
    value: &Value,
    client_tx: &mpsc::UnboundedSender<String>,
    client_id: u64,
) {
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

    let result = SessionManager::switch_session(
        &state.session_manager,
        iid,
        client_id,
    );

    match result {
        SessionResult::Switched {
            instance_id,
            messages,
            message_seq,
            ..
        } => {
            log::debug!("[gateway] switch_session: Switched to {}", instance_id);
            send_snapshot(client_tx, &instance_id, &messages, message_seq);
            // Forward switch_session to pi
            let (text, value, state, client_tx) = (
                raw_text.to_string(),
                value.clone(),
                state.clone(),
                client_tx.clone(),
            );
            tokio::spawn(async move {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                forward_to_instance(&text, &value, &instance_id, &state, &client_tx);
            });
        }
        SessionResult::NeedSpawn { .. } => {
            // Session exists in DB but not running — spawn with persisted instance_id
            log::info!(
                "[gateway] switch_session: instance {} not running, spawning",
                iid
            );
            // Get cwd and session_path from DB
            let db_session = state
                .db
                .all_sessions()
                .into_iter()
                .find(|s| s.instance_id == iid);
            let cwd = extract_cwd(&value).or_else(|| db_session.as_ref().map(|s| s.cwd.clone()));
            let Some(cwd) = cwd else {
                log::warn!("[gateway] switch_session: no cwd for instance {}", iid);
                notify_undeliverable(client_tx, &value, "missing_cwd");
                return;
            };
            let session_path = db_session.and_then(|s| s.session_path);
            let extensions = resolve_project_extensions(&state.db, &cwd, &cwd);
            // Load existing messages from session file (if it exists)
            let existing_messages: Vec<Value> = session_path
                .as_ref()
                .map(|sp| load_session(sp))
                .unwrap_or_default();
            let msg_seq = existing_messages.len() as u64;
            // Reuse the persisted instance_id, resume existing session file
            let resume_result = crate::gateway::handlers::pi::resume_session(
                state, iid, &cwd, session_path.as_deref(), None, &extensions,
            );
            match resume_result {
                Ok(new_iid) => {
                    // Register in routing table
                    state
                        .inner
                        .routes
                        .lock()
                        .insert(new_iid.clone(), new_iid.clone());
                    // Register in session manager with existing messages
                    SessionManager::register_instance(
                        &state.session_manager,
                        &new_iid,
                        &cwd,
                        client_id,
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
                    push_sessions_list_to_clients(state);
                    // Tell frontend the instance is ready with loaded messages
                    let snapshot = serde_json::json!({
                        "type": "session_snapshot",
                        "instanceId": new_iid,
                        "messages": existing_messages,
                        "messageSeq": msg_seq,
                    });
                    let _ = client_tx.send(snapshot.to_string());
                    // Forward switch_session after pi starts
                    let (text, value, state, client_tx) = (
                        raw_text.to_string(),
                        value.clone(),
                        state.clone(),
                        client_tx.clone(),
                    );
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
