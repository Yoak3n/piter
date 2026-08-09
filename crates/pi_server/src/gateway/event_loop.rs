//! Gateway 事件循环：订阅 broker 事件并分发。
//!
//! 做什么：`run_event_loop` 订阅 broker 事件通道；`process_broker_event` 做事件分拣
//! （response 交给 responses.rs、会话消息跟踪 + 广播、sessions_list 推送、agent_end
//! 收尾）；`track_and_broadcast` 包装事件信封转发给 WS 客户端。
//! 不做什么：不解析 response 细节（responses.rs）；不建路由（server.rs）。
//! 依赖：broadcast、ws::send_get_state、responses、session_manager、db、GatewayState。

use std::sync::Arc;

use pi_rpc::event::LIFECYCLE_EVENT_TYPES;
use tokio::sync::broadcast as tokio_broadcast_channel;

use crate::broker::types::PROTOCOL_VERSION;

use super::{
    broadcast::{broadcast_to_clients, broadcast_to_subscribers, push_sessions_list_to_clients},
    responses::handle_response_event,
    ws::send_get_state,
    GatewayState,
};

/// Main event loop: subscribe to broker events, maintain routing table,
/// wrap and forward events to WS clients.
pub(super) async fn run_event_loop(
    state: &Arc<GatewayState>,
    event_rx: &mut tokio_broadcast_channel::Receiver<String>,
) {
    loop {
        match event_rx.recv().await {
            Ok(raw) => {
                process_broker_event(state, &raw);
            }
            Err(tokio_broadcast_channel::error::RecvError::Closed) => {
                log::info!("[gateway] event channel closed, event loop exiting");
                break;
            }
            Err(tokio_broadcast_channel::error::RecvError::Lagged(n)) => {
                log::warn!("[gateway] event loop lagged {} events, skipping", n);
                continue;
            }
        }
    }
}

/// Process a single event from the broker.
fn process_broker_event(state: &Arc<GatewayState>, raw: &str) {
    // 3个卫语句：
    // 1. 解析JSON字符串为serde_json::Value
    // 2. 检查type字段是否存在且为字符串类型
    // 3. 检查instance_id字段是否存在且为字符串类型
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    let Some(event_type) = val.get("type").and_then(serde_json::Value::as_str) else {
        return;
    };

    // Gateway internal event (no instanceId) — handle before instanceId guard.
    if event_type == "session_cleanup" {
        push_sessions_list_to_clients(state);
        return;
    }

    let Some(iid) = pi_rpc::event::extract_instance_id(&val) else {
        return;
    };


    let instance_id = iid.to_string();

    // ── 1. Response events ─────────────────────────────────────────────
    if event_type == "response" {
        handle_response_event(state, raw, &instance_id);
    }

    // ── 2. Track message in session manager and broadcast ───────────
    track_and_broadcast(state, &val, event_type, &instance_id);

    // ── 3. Push updated sessions list for session-changing events ─────
    if matches!(event_type, "agent_end" | "turn_end") {
        push_sessions_list_to_clients(state);
    }

    // ── 5b. Refresh pi state after agent finishes handling a message ──
    if event_type == "agent_end" {
        send_get_state(state, &instance_id);
        // 会话完成通知观察点：向桌面壳层暴露 agent_end（托盘隐藏时前端 WS 不可达，
        // 系统通知只能由 Rust 侧基于此回调发送；label 为空时由壳层回退 instance_id）。
        if let Some(hook) = &*state.agent_end_hook.lock() {
            let label = state
                .session_manager
                .lock()
                .sessions
                .get(&instance_id)
                .and_then(|s| s.session_name.clone())
                .unwrap_or_default();
            hook(&instance_id, &label);
        }
    }

    // Persist any auto-generated session names to DB
    let pending_names = state.session_manager.lock().take_pending_names();
    for (iid, name) in &pending_names {
        let _ = state.db.set_session_name(iid, name);
    }

    if state.session_manager.lock().take_dirty() || !pending_names.is_empty() {
        push_sessions_list_to_clients(state);
    }
}

/// Track message in session manager and broadcast to clients.
fn track_and_broadcast(
    state: &Arc<GatewayState>,
    val: &serde_json::Value,
    event_type: &str,
    instance_id: &str,
) {
    let message_seq = if !instance_id.is_empty() {
        state
            .session_manager
            .lock()
            .on_event(val, instance_id)
            .unwrap_or(0)
    } else {
        0
    };

    let is_pi_event = LIFECYCLE_EVENT_TYPES.contains(&event_type);

    let envelope = if is_pi_event {
        serde_json::json!({
            "type": "event",
            "event": val,
            "instanceId": instance_id,
            "messageSeq": message_seq,
            "protocolVersion": PROTOCOL_VERSION,
        })
    } else {
        serde_json::json!({
            "type": event_type,
            "payload": val,
            "instanceId": instance_id,
            "messageSeq": message_seq,
            "protocolVersion": PROTOCOL_VERSION,
        })
    };

    // If instance has subscribers, send only to them; otherwise broadcast to all
    let envelope_str = envelope.to_string();
    if !instance_id.is_empty() && state.session_manager.lock().has_subscribers(instance_id) {
        broadcast_to_subscribers(state, instance_id, &envelope_str);
    } else {
        broadcast_to_clients(state, &envelope_str);
    }
}
