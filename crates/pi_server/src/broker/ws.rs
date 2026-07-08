//! WebSocket handler and message routing.
//!
//! Architecture: one pi process per session. Each session file gets its own
//! dedicated pi process. Switching sessions means switching which pi process
//! is "active" — it never reuses a process that's busy with another session.
//!
//! Protocol follows picot's broker_ws conventions:
//! - UI → broker: `broker_command` (wrapped) or bare commands
//! - broker → pi:  unwrapped payload forwarded to stdin
//! - pi → broker:  stdout events tagged with sessionPath, wrapped as `broker_event`
//! - broker → UI:  raw events broadcast to all connected clients

use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::ws;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
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

    // Capability handshake: tell the client whether native ops are available.
    let native = state.inner.control_handler.lock().unwrap().is_some();
    let _ = client_tx.send(
        json!({
            "type": "capabilities",
            "protocolVersion": PROTOCOL_VERSION,
            "native": native,
        })
        .to_string(),
    );

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
                        route_ui_message(&text, &state, &client_tx);
                    }
                    Some(Ok(ws::Message::Close(_))) | None => {
                        log::debug!("[broker] ws client {} closed connection", client_id);
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::debug!("[broker] ws client {} recv error: {}", client_id, e);
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

/// Route an incoming UI message. Handles `broker_control` (host ops),
/// `broker_command` (wrapped pi commands), and bare pi commands.
///
/// Session lifecycle commands (`new_session`, `switch_session`) will
/// automatically spawn a pi process if none is running.
fn route_ui_message(text: &str, state: &Arc<BrokerState>, client_tx: &mpsc::UnboundedSender<String>) {
    let Ok(value) = serde_json::from_str::<Value>(text) else {
        log::warn!("[broker] invalid UI message");
        return;
    };

    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");

    // `broker_control` requests are NOT forwarded to a pi upstream — they are
    // process/window lifecycle or native ops handled by the host (Rust).
    if msg_type == "broker_control" {
        let state = state.clone();
        let value = value.clone();
        tokio::spawn(async move {
            dispatch_control(value, &state).await;
        });
        return;
    }

    // ── Resolve the effective command type ──
    // For `broker_command`, the real command is inside payload.type.
    let effective_type = if msg_type == "broker_command" {
        value.pointer("/payload/type").and_then(Value::as_str).unwrap_or("")
    } else {
        msg_type
    };

    // ── Session commands: ensure pi is running ────────────────────────
    // `new_session` and `switch_session` are the entry points for creating
    // or selecting a session. If no pi process exists yet, spawn one first
    // and then forward the command.
    let is_session_cmd = matches!(effective_type, "new_session" | "switch_session");

    if is_session_cmd && state.inner.pi_processes.lock().is_empty() {
        log::info!(
            "[broker] session command '{}' but no pi running; spawning pi first",
            effective_type
        );

        // Extract cwd from the command payload (new_session carries it)
        let cwd = value
            .pointer("/payload/cwd")
            .and_then(Value::as_str)
            .map(|s| s.to_string())
            .or_else(|| {
                value.get("cwd").and_then(Value::as_str).map(|s| s.to_string())
            })
            .unwrap_or_else(|| {
                std::env::current_dir()
                    .map(|p| p.to_string_lossy().to_string())
                    .unwrap_or_else(|_| ".".to_string())
            });

        // Allocate a port for the new pi process
        let port = next_port(state);
        if let Err(e) = super::process::spawn_pi_process(
            &state.pi_exe,
            &state.static_dir,
            &state.pi_version,
            &cwd,
            port,
            None,
            &state.event_tx,
            &state.inner,
        ) {
            log::error!("[broker] spawn_pi failed: {}", e);
            notify_undeliverable(client_tx, &value, "spawn_failed");
            return;
        }

        // Forward the command after a short delay to let pi initialise.
        let text_owned = text.to_string();
        let effective_type_owned = effective_type.to_string();
        let state = state.clone();
        let client_tx = client_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(800)).await;
            log::info!("[broker] forwarding deferred '{}' to newly spawned pi", effective_type_owned);
            forward_to_active(&text_owned, &state, &client_tx);
        });
        return;
    }

    // ── Normal routing ────────────────────────────────────────────────
    let Some(port) = resolve_command_port(&value, state) else {
        log::warn!("[broker] no route for UI command: {}", msg_type);
        notify_undeliverable(client_tx, &value, "no_route");
        return;
    };

    log::info!(
        "[broker] route command={} request_id={:?} session_id={:?} source_port={:?} -> port={}",
        effective_type,
        value.get("requestId").and_then(Value::as_str),
        value.get("sessionId").and_then(Value::as_str),
        value.get("sourcePort").and_then(Value::as_u64),
        port,
    );

    forward_to_pi(text, &value, port, state, client_tx);
}

/// Forward a bare or `broker_command` message to the active pi process
/// (auto-selects port). Used by the deferred session-command path.
fn forward_to_active(text: &str, state: &Arc<BrokerState>, client_tx: &mpsc::UnboundedSender<String>) {
    let Ok(value) = serde_json::from_str::<Value>(text) else { return };
    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let effective_type = if msg_type == "broker_command" {
        value.pointer("/payload/type").and_then(Value::as_str).unwrap_or("")
    } else {
        msg_type
    };

    let Some(port) = resolve_command_port(&value, state) else {
        log::warn!("[broker] deferred command '{}' still has no route after spawn", effective_type);
        notify_undeliverable(client_tx, &value, "no_route");
        return;
    };
    forward_to_pi(text, &value, port, state, client_tx);
}

/// Forward a command to a specific pi process by port.
fn forward_to_pi(
    text: &str,
    value: &Value,
    port: u16,
    state: &Arc<BrokerState>,
    client_tx: &mpsc::UnboundedSender<String>,
) {
    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    // If this is a `broker_command`, unwrap the payload before forwarding.
    // The pi process expects bare commands, not wrapped envelopes.
    let forward_text = if msg_type == "broker_command" {
        match value.get("payload") {
            Some(payload) => serde_json::to_string(payload).unwrap_or_else(|_| text.to_string()),
            None => text.to_string(),
        }
    } else {
        text.to_string()
    };

    let processes = state.inner.pi_processes.lock();
    if let Some(process) = processes.get(&port) {
        if let Some(tx) = &process.stdin_tx {
            if tx.send(forward_text).is_err() {
                log::warn!("[broker] upstream {} channel closed; command dropped", port);
                drop(processes);
                notify_undeliverable(client_tx, value, "upstream_unavailable");
            }
        }
    } else {
        log::warn!("[broker] no pi process on port {}; command dropped", port);
        drop(processes);
        notify_undeliverable(client_tx, value, "upstream_unavailable");
    }
}

/// Allocate the next port number for a new pi process.
fn next_port(state: &Arc<BrokerState>) -> u16 {
    let mut active = state.inner.active_port.lock();
    let port = active.map(|p| p + 1).unwrap_or(9001);
    *active = Some(port);
    port
}

/// Reply to the originating UI client that a `broker_command` could not be
/// delivered. Tagged with the original `requestId` so the frontend can
/// correlate it to the in-flight prompt and surface a visible error.
fn notify_undeliverable(client_tx: &mpsc::UnboundedSender<String>, value: &Value, reason: &str) {
    let request_id = value.get("requestId").and_then(Value::as_str).unwrap_or("");
    let command = value
        .pointer("/payload/type")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or("");
    let _ = client_tx.send(
        json!({
            "type": "command_undeliverable",
            "protocolVersion": PROTOCOL_VERSION,
            "requestId": request_id,
            "command": command,
            "reason": reason,
            "sessionId": value.get("sessionId").cloned().unwrap_or(Value::Null),
        })
        .to_string(),
    );
}

async fn dispatch_control(value: Value, state: &Arc<BrokerState>) {
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
        "ping" => json!({
            "type": "control_response",
            "requestId": request_id,
            "ok": true,
            "result": { "pong": true },
        }),
        "info" => json!({
            "type": "control_response",
            "requestId": request_id,
            "ok": true,
            "result": {
                "version": env!("CARGO_PKG_VERSION"),
                "features": ["rpc", "ws", "lan", "health", "multi_process"],
                "runtimes": ["rust", "pi-rpc"],
            },
        }),
        _ => json!({
            "type": "control_response",
            "requestId": request_id,
            "ok": false,
            "error": format!("Unknown command: {}", command),
        }),
    };

    let _ = state.event_tx.send(response.to_string());
}

/// Resolve which pi port should receive a UI command.
///
/// Resolution order (following picot's broker_ws):
/// 1. `sessionId` / `sessionFile` / `sessionPath` from the message or payload
/// 2. `sourcePort` hint from the client
/// 3. Global `active_port` (only when unambiguous — single upstream)
fn resolve_command_port(value: &Value, state: &Arc<BrokerState>) -> Option<u16> {
    // ── Level 1: session-id route ──
    let session_id = value
        .get("sessionId")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/payload/sessionId").and_then(Value::as_str))
        .or_else(|| value.get("sessionFile").and_then(Value::as_str))
        .or_else(|| value.pointer("/payload/sessionFile").and_then(Value::as_str))
        .or_else(|| value.get("sessionPath").and_then(Value::as_str))
        .or_else(|| value.pointer("/payload/sessionPath").and_then(Value::as_str));

    if let Some(sid) = session_id {
        if let Some(port) = state.inner.routes.lock().get(sid).copied() {
            // The session route is authoritative. Warn if sourcePort disagrees.
            let source_port = value
                .get("sourcePort")
                .and_then(Value::as_u64)
                .and_then(|p| u16::try_from(p).ok());
            if let Some(sp) = source_port {
                if sp != port {
                    log::warn!(
                        "[broker] route/source_port disagree: session_id={} -> port={} but source_port={}; trusting session route",
                        sid, port, sp
                    );
                }
            }
            return Some(port);
        }
    }

    // ── Level 2: sourcePort hint ──
    if let Some(source_port) = value
        .get("sourcePort")
        .and_then(Value::as_u64)
        .and_then(|p| u16::try_from(p).ok())
    {
        if state.inner.pi_processes.lock().contains_key(&source_port) {
            return Some(source_port);
        }
    }

    // ── Level 3: active_port (only when unambiguous) ──
    let active = *state.inner.active_port.lock();
    let process_count = state.inner.pi_processes.lock().len();
    if process_count > 1 {
        log::warn!(
            "[broker] refusing ambiguous active_port fallback ({:?}) among {} live upstreams",
            active, process_count
        );
        return None;
    }
    active
}
