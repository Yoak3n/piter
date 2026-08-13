//! Gateway 事件循环：订阅 broker 事件并分发。
//!
//! 做什么：`run_event_loop` 订阅 broker 事件通道；`process_broker_event` 做事件分拣
//! （response 交给 responses.rs、会话消息跟踪 + 广播、sessions_list 推送、agent_end
//! 收尾）；`track_and_broadcast` 包装事件信封转发给 WS 客户端。
//! 不做什么：不解析 response 细节（responses.rs）；不建路由（server.rs）。
//! 依赖：broadcast、ws::send_get_state、responses、session_manager、db、GatewayState。

use std::path::PathBuf;
use std::sync::Arc;

use pi_rpc::event::LIFECYCLE_EVENT_TYPES;
use serde_json::{json, Value};
use tokio::sync::broadcast as tokio_broadcast_channel;

use crate::broker::types::PROTOCOL_VERSION;

use super::{
    broadcast::{broadcast_to_clients, broadcast_to_subscribers, push_sessions_list_to_clients},
    responses::handle_response_event,
    workspace,
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

    // ── 4. Workspace 集成（0.3.0）───────────────────────────────────
    if event_type == "turn_end" {
        maybe_push_turn_artifacts(state, &instance_id);
    } else if event_type == "tool_execution_start" {
        maybe_push_write_block(state, &val, &instance_id);
    }

    // ── 5b. Refresh pi state after agent finishes handling a message ──
    if event_type == "agent_end" {
        // 撤回文件回滚的 checkpoint：agent_end 时若 git 仓库有改动即落一个快照。
        super::checkpoint::create_checkpoint(state, &instance_id);
        send_get_state(state, &instance_id);
        // 会话完成通知观察点：向桌面壳层暴露 agent_end（托盘隐藏时前端 WS 不可达，
        // 系统通知只能由 Rust 侧基于此回调发送；label 为空时由壳层回退 instance_id）。
        // 附带最后一条 assistant 消息摘要，供通知正文展示完成内容。
        if let Some(hook) = &*state.agent_end_hook.lock() {
            let mgr = state.session_manager.lock();
            let label = mgr
                .sessions
                .get(&instance_id)
                .and_then(|s| s.session_name.clone())
                .unwrap_or_default();
            let last_text = mgr
                .sessions
                .get(&instance_id)
                .and_then(|s| {
                    s.messages
                        .iter()
                        .rev()
                        .find(|m| {
                            m.get("role").and_then(serde_json::Value::as_str)
                                == Some("assistant")
                        })
                })
                .map(crate::search::extract_text)
                .unwrap_or_default();
            drop(mgr);
            hook(&instance_id, &label, &last_text);
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

// ─── Workspace 集成（0.3.0）：turn_end 快照 diff → turn_artifacts ───────────

/// turn_end 时对工作空间会话做快照 diff，并把本轮产物推给客户端。
/// 非工作空间会话 / 无变化 / 快照失败 → 静默跳过。
fn maybe_push_turn_artifacts(state: &GatewayState, instance_id: &str) {
    let Some(ws_id) = workspace::workspace_id_for_session(&state.db, instance_id) else {
        return;
    };
    let turn_id = state
        .session_manager
        .lock()
        .sessions
        .get(instance_id)
        .map(|s| s.message_seq as i64)
        .unwrap_or(0);
    let rows = match workspace::capture_turn_artifacts(
        &state.db,
        &state.data_dir,
        &ws_id,
        instance_id,
        turn_id,
        "snapshot",
    ) {
        Ok(r) => r,
        Err(e) => {
            log::warn!("[workspace] capture_turn_artifacts failed: {}", e);
            return;
        }
    };
    if rows.is_empty() {
        return;
    }
    let items: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "path": r.rel_path,
                "op": r.op,
                "size": r.size,
                "linesAdded": r.lines_added,
                "linesDeleted": r.lines_deleted,
                "deliverable": r.deliverable,
            })
        })
        .collect();
    let msg = json!({
        "type": "turn_artifacts",
        "instanceId": instance_id,
        "workspaceId": ws_id,
        "turnId": turn_id,
        "items": items,
    })
    .to_string();
    if state.session_manager.lock().has_subscribers(instance_id) {
        broadcast_to_subscribers(state, instance_id, &msg);
    } else {
        broadcast_to_clients(state, &msg);
    }
}

// ─── 写阻断推断（软约束通道）：write/edit 目标在工作空间外 → write_block ─────

/// 从 tool_execution_start 推断越界写入并推送 write_block（ask/deny 模式）。
/// 已审批（approvals.json 白名单）的目标不再重复打扰；allow 模式不推断
/// （constraint 扩展放行一切）。
fn maybe_push_write_block(state: &GatewayState, val: &Value, instance_id: &str) {
    let Some(ws_id) = workspace::workspace_id_for_session(&state.db, instance_id) else {
        return;
    };
    // allow 模式由扩展放行，无需打扰。
    if state.db.get_project_mode(&ws_id) == "allow" {
        return;
    }
    let tool = val.get("toolName").and_then(Value::as_str).unwrap_or("");
    if tool != "write" && tool != "edit" {
        return;
    }
    let Some(raw) = val
        .get("args")
        .and_then(|a| a.get("path"))
        .and_then(Value::as_str)
    else {
        return;
    };
    let dir = match workspace::workspace_dir_from_id(&state.db, &ws_id) {
        Ok(d) => d,
        Err(_) => return,
    };
    let abs_str = if PathBuf::from(raw).is_absolute() {
        crate::broker::util::strip_verbatim_prefix(raw).replace('\\', "/")
    } else {
        dir.join(raw).to_string_lossy().replace('\\', "/")
    };
    let abs = PathBuf::from(&abs_str);
    // 规范化比较：目录 canonicalize；目标存在时也 canonicalize，否则按字面比。
    let Ok(base) = dir.canonicalize() else {
        return;
    };
    let target = abs.canonicalize().unwrap_or_else(|_| abs.clone());
    if workspace::is_inside(&base, &target) {
        return;
    }
    // 已在白名单（上轮已批准）→ 不再推送。
    if workspace::approvals_set(&dir).contains(&abs_str) {
        return;
    }
    let request_id = format!(
        "wb_{}",
        &uuid::Uuid::new_v4().to_string().replace('-', "")[..8]
    );
    let msg = json!({
        "type": "write_block",
        "instanceId": instance_id,
        "workspaceId": ws_id,
        "path": abs_str,
        "reason": format!(
            "写入位置应在工作空间内（cwd={}）；如确实需要请批准",
            dir.to_string_lossy()
        ),
        "requestId": request_id,
    })
    .to_string();
    if state.session_manager.lock().has_subscribers(instance_id) {
        broadcast_to_subscribers(state, instance_id, &msg);
    } else {
        broadcast_to_clients(state, &msg);
    }
}
