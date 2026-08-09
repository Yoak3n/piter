//! Response 事件处理：pi 对 gateway 命令的响应。
//!
//! 做什么：`handle_response_event` 分拣 response 给三个子 handler ——
//! `handle_session_response`（会话确认 → 路由表 + get_state）、
//! `handle_get_state_response`（完整状态落 DB + session manager + 模型持久化）、
//! `handle_model_response`（set_model/cycle_model 成功 → 刷新运行时模型 + default 回滚）。
//! 不做什么：不广播事件（event_loop.rs）；不解析非 response 事件。
//! 依赖：ws::send_get_state、session_manager、db、broadcast、broker::util。

use std::sync::Arc;

use super::{
    broadcast::push_sessions_list_to_clients, session_manager, ws::send_get_state, GatewayState,
};

/// Handle response-type events: session tracking, get_state triggers, and state completion.
pub(super) fn handle_response_event(state: &Arc<GatewayState>, raw: &str, instance_id: &str) {
    let Ok(resp) = pi_rpc::event::Response::from_json_line(raw) else {
        return;
    };

    handle_session_response(state, &resp, instance_id);
    handle_get_state_response(state, &resp, instance_id);
    handle_model_response(state, &resp, instance_id);
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
