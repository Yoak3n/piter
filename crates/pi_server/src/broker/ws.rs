//! WebSocket handler and message routing.
//!
//! Architecture: one pi process per session. Each session file gets its own
//! dedicated pi process. Switching sessions means switching which pi process
//! is "active" — it never reuses a process that's busy with another session.

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::ws;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

use super::types::{BrokerState, PROTOCOL_VERSION};

pub async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(socket: ws::WebSocket, state: Arc<BrokerState>) {
    let client_id = state.inner.next_client_id.fetch_add(1, Ordering::Relaxed);
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();

    state.inner.ui_clients.lock().unwrap().insert(client_id, client_tx.clone());

    // Send capabilities
    let native = state.inner.control_handler.lock().unwrap().is_some();
    let capabilities = serde_json::json!({
        "type": "capabilities",
        "protocolVersion": PROTOCOL_VERSION,
        "native": native,
    });
    let _ = client_tx.send(capabilities.to_string());

    // Spawn task to send messages to client
    let send_task = tokio::spawn(async move {
        while let Some(msg) = client_rx.recv().await {
            if ws_tx.send(ws::Message::Text(msg)).await.is_err() {
                break;
            }
        }
    });

    // Subscribe to events
    let mut event_rx = state.event_tx.subscribe();

    loop {
        tokio::select! {
            msg = ws_rx.next() => {
                match msg {
                    Some(Ok(ws::Message::Text(text))) => {
                        handle_client_message(&text, &state).await;
                    }
                    Some(Ok(ws::Message::Close(_))) | None => {
                        log::debug!("[broker] ws client closed connection");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::debug!("[broker] ws recv error: {}", e);
                        break;
                    }
                }
            }
            result = event_rx.recv() => {
                match result {
                    Ok(event) => {
                        if client_tx.send(event).is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("[broker] ws lagged {} events, skipping", n);
                        continue;
                    }
                }
            }
        }
    }

    state.inner.ui_clients.lock().unwrap().remove(&client_id);
    send_task.abort();
}

async fn handle_client_message(text: &str, state: &Arc<BrokerState>) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        log::warn!("[broker] invalid UI message");
        return;
    };

    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");

    // Handle broker_control commands
    if msg_type == "broker_control" {
        dispatch_control(value, state).await;
        return;
    }

    match msg_type {
        // ── new_session: spawn a fresh pi process for a brand-new session ──
        "new_session" => {
            let cwd = value.get("cwd").and_then(Value::as_str).unwrap_or(".");
            let port = state.next_port();
            match state.spawn_pi(cwd, port, None) {
                Ok(()) => {
                    state.set_active_port(port);
                    // port_sessions will be set when pi outputs the response with sessionFile
                    log::info!("[broker] new_session: spawned pi on port {}", port);
                    forward_to_port(text, state, port);
                }
                Err(e) => {
                    log::error!("[broker] failed to spawn pi for new_session: {}", e);
                }
            }
        }

        // ── switch_session: find existing pi or spawn a new one ──
        "switch_session" => {
            let session_file = value.get("sessionFile").and_then(Value::as_str);
            if let Some(sf) = session_file {
                // Check if a pi is already running for this session
                if let Some(port) = state.inner.routes.lock().get(sf).copied() {
                    if state.inner.pi_processes.lock().contains_key(&port) {
                        state.set_active_port(port);
                        log::info!(
                            "[broker] switch_session: reusing pi on port {} for {}",
                            port, sf
                        );
                        forward_to_port(text, state, port);
                        return;
                    }
                    // Route exists but process is dead — clean up stale route
                    state.inner.routes.lock().remove(sf);
                }

                // No pi for this session — spawn a new one
                let port = state.next_port();
                match state.spawn_pi(".", port, Some(sf)) {
                    Ok(()) => {
                        state.set_active_port(port);
                        state.inner.routes.lock().insert(sf.to_string(), port);
                        state.inner.port_sessions.lock().insert(port, sf.to_string());
                        log::info!(
                            "[broker] switch_session: spawned new pi on port {} for {}",
                            port, sf
                        );
                        // Don't forward switch_session to the new pi — it was
                        // spawned with --session already. Instead, the frontend
                        // will receive events from the new pi directly.
                    }
                    Err(e) => {
                        log::error!(
                            "[broker] failed to spawn pi for switch_session {}: {}",
                            sf, e
                        );
                    }
                }
            }
        }

        // ── prompt and other commands: route to the correct pi ──
        _ => {
            if let Some(port) = resolve_command_port(&value, state) {
                forward_to_port(text, state, port);
            } else {
                log::warn!("[broker] no route for UI command type={}", msg_type);
            }
        }
    }
}

fn forward_to_port(text: &str, state: &Arc<BrokerState>, port: u16) {
    let processes = state.inner.pi_processes.lock();
    if let Some(process) = processes.get(&port) {
        if let Some(tx) = &process.stdin_tx {
            let _ = tx.send(text.to_string());
        }
    } else {
        log::warn!("[broker] no pi process on port {}", port);
    }
}

async fn dispatch_control(value: Value, state: &Arc<BrokerState>) {
    // Send control response to ALL connected UI clients
    let request_id = value
        .get("requestId")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let command = value
        .get("command")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    let response = match command.as_str() {
        "ping" => serde_json::json!({
            "type": "control_response",
            "requestId": request_id,
            "ok": true,
            "result": { "pong": true },
        }),
        "info" => serde_json::json!({
            "type": "control_response",
            "requestId": request_id,
            "ok": true,
            "result": {
                "version": env!("CARGO_PKG_VERSION"),
                "features": ["rpc", "ws", "lan", "health", "multi_process"],
                "runtimes": ["rust", "pi-rpc"],
            },
        }),
        _ => serde_json::json!({
            "type": "control_response",
            "requestId": request_id,
            "ok": false,
            "error": format!("Unknown command: {}", command),
        }),
    };

    // Broadcast to all UI clients
    let _ = state.event_tx.send(response.to_string());
}

fn resolve_command_port(value: &Value, state: &Arc<BrokerState>) -> Option<u16> {
    // Try to find the target port from sessionId/sessionFile in the message
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| value.get("sessionFile").and_then(Value::as_str))
        .or_else(|| value.pointer("/payload/sessionId").and_then(Value::as_str))
        .or_else(|| value.pointer("/payload/sessionFile").and_then(Value::as_str));

    if let Some(sid) = session_id {
        if let Some(port) = state.inner.routes.lock().get(sid).copied() {
            return Some(port);
        }
    }

    // Fall back to active port
    let active = *state.inner.active_port.lock();
    if let Some(port) = active {
        // Verify the process is still alive
        if state.inner.pi_processes.lock().contains_key(&port) {
            return Some(port);
        }
    }
    None
}
