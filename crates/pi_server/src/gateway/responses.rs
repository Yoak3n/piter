//! Response 事件处理：pi 对 gateway 命令的响应。
//!
//! 做什么：`handle_response_event` 分拣 response 给四个子 handler ——
//! `handle_session_response`（会话确认 → 路由表 + get_state）、
//! `handle_get_state_response`（完整状态落 DB + session manager + 模型持久化
//!   + 撤回后的删旧文件/文件回滚收尾）、
//! `handle_model_response`（set_model/cycle_model 成功 → 刷新运行时模型 + default 回滚）、
//! `handle_fork_response`（撤回成功 → get_state/get_messages 补发）与
//! `handle_get_messages_response`（fork 后重置消息缓存并推送快照）。
//! 不做什么：不广播事件（event_loop.rs）；不解析非 response 事件。
//! 依赖：ws::send_get_state / send_get_messages、session_manager、db、broadcast、checkpoint。

use std::sync::Arc;

use super::{
    broadcast::{broadcast_to_clients, broadcast_to_subscribers, push_sessions_list_to_clients},
    session_manager, ws::{send_get_messages, send_get_state}, GatewayState,
};

/// Handle response-type events: session tracking, get_state triggers, and state completion.
pub(super) fn handle_response_event(state: &Arc<GatewayState>, raw: &str, instance_id: &str) {
    let Ok(resp) = pi_rpc::event::Response::from_json_line(raw) else {
        return;
    };

    handle_session_response(state, &resp, instance_id);
    handle_get_state_response(state, &resp, instance_id);
    handle_model_response(state, &resp, instance_id);
    handle_fork_response(state, &resp, instance_id);
    handle_get_messages_response(state, &resp, instance_id);
}

/// 1a/1b. On session-related responses: track assignment + trigger get_state.
fn handle_session_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if !resp.is_session_response() {
        return;
    }

    if let Some(sf) = resp.session_file() {
        log::info!(
            "[gateway] pi confirmed session: instance={} session={}",
            instance_id,
            sf
        );
        state
            .inner
            .routes
            .lock()
            .insert(sf.to_string(), instance_id.to_string());
    }
    push_sessions_list_to_clients(state);
    send_get_state(state, instance_id);
}

/// 1c. On get_state response → complete pending link + store full state.
fn handle_get_state_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if !resp.success || resp.command != "get_state" {
        return;
    }

    let data = match resp.data.as_ref() {
        Some(d) => d,
        None => return,
    };

    // Extract sessionFile for DB link and routing
    let session_file = data
        .get("sessionFile")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());

    if let Some(sf) = session_file {
        let _ = state.db.complete_session(instance_id, sf);
        state
            .inner
            .routes
            .lock()
            .insert(sf.to_string(), instance_id.to_string());
        let pending = state
            .session_manager
            .lock()
            .pending_links
            .remove(instance_id);
        if pending.is_some() {
            push_sessions_list_to_clients(state);
        }
        // 无感撤回收尾：get_state 确认新文件 B 已落盘 → 删旧文件 A + 文件回滚。
        // 任一失败只推送提示、不阻断（消息已撤回、B 已生效，A 留待下次清理）。
        let cleanup = state
            .session_manager
            .lock()
            .take_pending_fork_cleanup(instance_id);
        if let Some(pf) = cleanup {
            if pf.old_path != sf {
                if pf.rollback {
                    if let Err(e) = super::checkpoint::restore_checkpoint(state, instance_id, pf.target_ms) {
                        log::warn!("[gateway] fork rollback failed for {}: {}", instance_id, e);
                        broadcast_fork_notice(state, instance_id, "fork_warn", &format!(
                            "文件恢复失败：{}", e
                        ));
                    }
                }
                // 旧文件可能已在上一轮收尾中删掉（清理失败留待下次），再遇到时不再重复告警。
                if std::path::Path::new(&pf.old_path).exists() {
                    if let Err(e) = std::fs::remove_file(&pf.old_path) {
                        log::warn!("[gateway] fork cleanup of old session failed: {}", e);
                        broadcast_fork_notice(state, instance_id, "fork_warn", "旧记录未清理，下次启动可清理");
                    } else {
                        log::info!("[gateway] fork: removed old session file {}", pf.old_path);
                    }
                }
                push_sessions_list_to_clients(state);
            }
        }
    }

    // Extract pi's native sessionId and register as route
    let pi_session_id = data
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());

    if let Some(sid) = pi_session_id {
        state
            .inner
            .routes
            .lock()
            .insert(sid.to_string(), instance_id.to_string());
    }

    // Parse full pi session state and store in session manager
    let model = data.get("model");
    let pi_state = session_manager::PiSessionState {
        session_file: session_file.map(|s| s.to_string()),
        session_id: pi_session_id.map(|s| s.to_string()),
        session_name: data
            .get("sessionName")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        model_id: model
            .and_then(|m| m.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        model_name: model
            .and_then(|m| m.get("name"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        model_provider: model
            .and_then(|m| m.get("provider"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        thinking_level: data
            .get("thinkingLevel")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        is_streaming: data
            .get("isStreaming")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        is_compacting: data
            .get("isCompacting")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        message_count: data
            .get("messageCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        pending_message_count: data
            .get("pendingMessageCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        context_window: model
            .and_then(|m| m.get("contextWindow"))
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32),
    };
    state
        .session_manager
        .lock()
        .update_pi_state(instance_id, pi_state);

    // Persist the instance's current model so it can be restored after restart
    if let Some(session) = state.session_manager.lock().sessions.get(instance_id) {
        if let Some(ps) = &session.pi_state {
            let _ = state.db.set_session_model(
                instance_id,
                ps.model_id.as_deref(),
                ps.model_provider.as_deref(),
            );
        }
    }
}

/// 1d. On set_model / cycle_model success → refresh runtime model state and
/// persist it to the DB immediately (no need to wait for the next get_state).
fn handle_model_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if !matches!(resp.command.as_str(), "set_model" | "cycle_model") {
        return;
    }
    if !resp.success {
        // set_model 失败：pi 未改模型，default 无需恢复，清掉备份避免残留
        state
            .session_manager
            .lock()
            .sessions
            .get_mut(instance_id)
            .map(|s| s.pending_default_restore = None);
        return;
    }
    let Some(data) = resp.data.as_ref() else {
        return;
    };
    // set_model returns the Model object directly; cycle_model wraps it in {model, ...}
    let model = data.get("model").unwrap_or(data);
    let Some(id) = model.get("id").and_then(serde_json::Value::as_str) else {
        return;
    };
    let provider = model.get("provider").and_then(serde_json::Value::as_str);

    {
        let mut mgr = state.session_manager.lock();
        if let Some(session) = mgr.sessions.get_mut(instance_id) {
            let ps = session.pi_state.get_or_insert_with(Default::default);
            ps.model_id = Some(id.to_string());
            ps.model_provider = provider.map(|s| s.to_string());
            if let Some(name) = model.get("name").and_then(serde_json::Value::as_str) {
                ps.model_name = Some(name.to_string());
            }
        }
        // pi 的 set_model 把 default 写进了 settings.json —— 恢复为切换前的值。
        // 仅当 settings.json 当前 default 仍等于本次 set_model 写入的模型时才回滚：
        // 若窗口内有其它写入（admin 修改 / 其它实例切换），尊重最新值，避免覆盖。
        if let Some((prev_provider, prev_model)) =
            mgr.sessions.get_mut(instance_id).and_then(|s| s.pending_default_restore.take())
        {
            drop(mgr);
            let expected_provider = provider.map(str::to_string).unwrap_or_default();
            if let Ok(cur) = crate::broker::util::read_pi_settings() {
                if cur.default_provider == expected_provider && cur.default_model == id {
                    let _ = crate::broker::util::set_default_model(&prev_provider, &prev_model);
                }
            }
        }
    }
    let _ = state.db.set_session_model(instance_id, Some(id), provider);
    push_sessions_list_to_clients(state);
}

/// 消息撤回（fork）响应：成功（且未被扩展取消）→ 转入 cleanup 槽 + 补发
/// get_state（更新 DB 指向新文件 B）/ get_messages（重置消息缓存）。
/// 失败 / 取消 → 推送前端提示（不改变会话）。
fn handle_fork_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if resp.command != "fork" {
        return;
    }

    // 取出 pending（broker/command.rs 已记录），失败时也清掉，避免残留。
    let pf = state.session_manager.lock().take_pending_fork(instance_id);

    if !resp.success {
        let err = resp.error.clone().unwrap_or_else(|| "unknown".to_string());
        broadcast_fork_notice(state, instance_id, "fork_error", &format!("撤回失败：{}", err));
        return;
    }
    let cancelled = resp
        .data
        .as_ref()
        .and_then(|d| d.get("cancelled"))
        .and_then(serde_json::Value::as_bool)
        .unwrap_or(false);
    if cancelled {
        broadcast_fork_notice(state, instance_id, "fork_error", "撤回被取消");
        return;
    }

    if let Some(pf) = pf {
        state
            .session_manager
            .lock()
            .set_pending_fork_cleanup(instance_id, pf);
    }

    push_sessions_list_to_clients(state);
    send_get_state(state, instance_id);
    send_get_messages(state, instance_id);
}

/// get_messages 响应（fork 后重置消息缓存）：整表替换内存消息 + 清 partial +
/// 重置 message_seq，然后把截断后的消息快照推给订阅者/所有客户端。
fn handle_get_messages_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if resp.command != "get_messages" || !resp.success {
        return;
    }
    let Some(msgs) = resp
        .data
        .as_ref()
        .and_then(|d| d.get("messages"))
        .and_then(serde_json::Value::as_array)
    else {
        return;
    };

    {
        let mut mgr = state.session_manager.lock();
        if let Some(s) = mgr.sessions.get_mut(instance_id) {
            s.messages = msgs.clone();
            s.partial_message = None;
            s.message_seq = s.messages.len() as u64;
            mgr.mark_dirty();
        }
    }

    let snapshot = serde_json::json!({
        "type": "session_snapshot",
        "instanceId": instance_id,
        "messages": msgs,
        "messageSeq": msgs.len(),
    })
    .to_string();
    if state.session_manager.lock().has_subscribers(instance_id) {
        broadcast_to_subscribers(state, instance_id, &snapshot);
    } else {
        broadcast_to_clients(state, &snapshot);
    }
}

/// 向该会话的订阅者（无订阅者则广播全部客户端）推送撤回提示事件。
fn broadcast_fork_notice(
    state: &Arc<GatewayState>,
    instance_id: &str,
    event_type: &str,
    message: &str,
) {
    let msg = serde_json::json!({
        "type": event_type,
        "message": message,
        "instanceId": instance_id,
    })
    .to_string();
    if state.session_manager.lock().has_subscribers(instance_id) {
        broadcast_to_subscribers(state, instance_id, &msg);
    } else {
        broadcast_to_clients(state, &msg);
    }
}
