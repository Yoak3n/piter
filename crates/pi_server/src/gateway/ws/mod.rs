//! WebSocket handler and message routing.
//!
//! Frontend uses `instance_id` for all session operations.
//! Real session file paths are internal to the backend.

mod helper;
mod broker;

pub use broker::{dispatch_control, send_get_messages, send_get_state};

use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::ws;
use axum::extract::{ConnectInfo, OriginalUri};
use axum::http::HeaderMap;
use axum::response::IntoResponse;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::sync::mpsc;

use crate::broker::types::PROTOCOL_VERSION;
use super::{
    state::ClientConnection, GatewayState,
    helper::{notify_undeliverable, forward_to_instance},
    broadcast::broadcast_connections_list,
};
use helper::resolve_command_instance;


pub async fn ws_handler(
    ws: axum::extract::WebSocketUpgrade,
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    OriginalUri(uri): OriginalUri,
) -> impl IntoResponse {
    // 端点 → 客户端类型（path 定前端）：
    // - /work-ws → work（工作空间视图/App，初始握手不含 chat 会话列表）
    // - /chat-ws → chat（Vue chat / App chat WebView）
    // - /ws、/ui-ws → ui（历史/管理兼容，不冒充业务客户端）
    let kind = match uri.path() {
        "/work-ws" => "work",
        "/chat-ws" => "chat",
        _ => "ui",
    }
    .to_string();
    ws.on_upgrade(move |socket| handle_ws(socket, state, addr, headers, kind))
}

/// 客户端形态判定（仅展示辅助，UA 可缺失/伪装）：Flutter App / 移动端 → app。
fn detect_client_form(ua: &str) -> String {
    let u = ua.to_lowercase();
    if u.contains("dart/") || u.contains("android") || u.contains("iphone") || u.contains("mobile")
    {
        "app".to_string()
    } else {
        "web".to_string()
    }
}

async fn handle_ws(
    socket: ws::WebSocket,
    state: Arc<GatewayState>,
    addr: SocketAddr,
    headers: HeaderMap,
    kind: String,
) {
    let client_id = state
        .inner
        .next_client_id
        .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let (mut ws_tx, mut ws_rx) = socket.split();
    let (client_tx, mut client_rx) = mpsc::unbounded_channel();

    state.ui_clients.lock().insert(client_id, client_tx.clone());

    // ── 连接注册（/api/connections + join 广播；kind 由端点 path 决定）──
    let ua = headers
        .get(axum::http::header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();
    let conn = ClientConnection {
        id: client_id,
        kind: kind.clone(),
        form: detect_client_form(&ua),
        ip: addr.ip().to_string(),
        user_agent: ua,
        connected_at_ms: crate::gateway::now_epoch_ms(),
    };
    state.connections.lock().insert(client_id, conn);
    broadcast_connections_list(&state);

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
    // work 客户端不需要 chat 的会话列表（handler 区分：work 专注工作空间事件）。
    if kind != "work" {
        let sessions = super::state::build_project_session_tree(&state);
        if let Ok(json) = serde_json::to_string(&sessions) {
            let _ = client_tx.send(format!(r#"{{"type":"sessions_list","projects":{}}}"#, json));
        }
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
    state.connections.lock().remove(&client_id);
    broadcast_connections_list(&state);
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
        let result = dispatch_gateway_command(command, &data, state, client_id, &client_tx);
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
    client_id: u64,
    client_tx: &mpsc::UnboundedSender<String>,
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
        // ── Workspace 命令（0.3.0，契约 mock-contract §3.2）────────────
        // 在指定工作空间内建/复用会话（cwd = workspace real_dir，自动携带 constraint 扩展）。
        "create_workspace_session" => {
            let ws_id = data.get("workspaceId").and_then(Value::as_str).ok_or("missing workspaceId")?;
            // 防饿死：该工作空间正等待基目录迁移（活跃会话等待/迁移中）→ 禁止新建会话。
            if crate::gateway::migrate::is_pending(state, ws_id) {
                return Err("workspace_migration_pending".to_string());
            }
            let cwd = crate::gateway::workspace::workspace_dir_from_id(&state.db, ws_id)?;
            let name = state
                .db
                .get_project(ws_id)
                .map(|p| p.name)
                .unwrap_or_else(|| "New Project".into());
            // 复用该工作空间已有的最新会话（保留历史对话），无则新建。
            // created_at 为 RFC3339（定宽、UTC），字典序即时间序。
            let latest = state
                .db
                .all_sessions()
                .into_iter()
                .filter(|s| s.project_id.as_deref() == Some(ws_id))
                .max_by(|a, b| a.created_at.cmp(&b.created_at));
            let iid = match latest {
                Some(session) => {
                    restore_workspace_session(state, client_tx, client_id, &session)?;
                    session.instance_id.clone()
                }
                None => super::session_manager::SessionManager::create_session(
                    &state.session_manager,
                    state,
                    &cwd.to_string_lossy(),
                    &name,
                    client_id,
                    None,
                )?,
            };
            // 建/复用后确保快照基线：存量文件计入基线，首轮 diff 只报真实改动，
            // 读文件/历史文件不再被误报为"新增"。
            crate::gateway::workspace::ensure_session_baseline(&state.db, ws_id, &iid)?;
            super::broadcast::push_sessions_list_to_clients(state);
            crate::gateway::ws::broker::command::send_get_state(state, &iid);
            Ok(json!({ "instanceId": iid, "cwd": cwd }))
        }
        // 批准越界写入：把绝对路径写入工作空间 .pi/approvals.json（constraint
        // 扩展下一轮 tool_call 命中白名单即放行）。allow=false 仅应答，不改状态。
        "approve_write" => {
            let ws_id = data.get("workspaceId").and_then(Value::as_str).ok_or("missing workspaceId")?;
            let path = data.get("path").and_then(Value::as_str).ok_or("missing path")?;
            let allow = data.get("allow").and_then(Value::as_bool).unwrap_or(true);
            if allow {
                let dir = crate::gateway::workspace::workspace_dir_from_id(&state.db, ws_id)?;
                crate::gateway::workspace::add_approval(&dir, path)?;
            }
            Ok(json!({ "success": true, "approved": allow }))
        }
        _ => Err(format!("unknown gateway command: {}", command)),
    }
}

/// 恢复一个已存在的 work 会话（复用 chat 端 switch_session 恢复链）：
/// - 运行中（Switched）：挂上订阅者，回推 session_snapshot（内存消息）。
/// - 已卸载（NeedSpawn）：resume_session 复活，从会话文件载入历史并回推
///   session_snapshot；同时恢复 DB 会话名（防 BUG-018 自动命名覆盖）。
fn restore_workspace_session(
    state: &Arc<GatewayState>,
    client_tx: &mpsc::UnboundedSender<String>,
    client_id: u64,
    session: &crate::gateway::db::SessionRow,
) -> Result<(), String> {
    use crate::gateway::{
        handlers::pi::resume_session,
        handlers::session::load_session,
        project::{effective_global_extensions, effective_project_extensions},
        session_manager::{SessionManager, SessionResult},
        ws::helper::message::send_snapshot,
    };
    let iid = session.instance_id.clone();
    match SessionManager::switch_session(&state.session_manager, &iid, client_id) {
        SessionResult::Switched {
            messages, message_seq, ..
        } => {
            send_snapshot(client_tx, &iid, &messages, message_seq);
            Ok(())
        }
        SessionResult::NeedSpawn { .. } => {
            // 迁移对齐：workspace 基目录变化后，会话文件首行 stored cwd 可能指向失效路径
            // （pi 校验不存在则拒绝加载直接退出）。恢复前用当前 real_dir 校正：
            // DB 与会话文件可能不同步（DB 已更新但文件仍旧路径），所以两者都要对齐。
            let mut effective_cwd = session.cwd.clone();
            if let Some(pid) = session.project_id.as_deref() {
                if let Ok(real) = crate::gateway::workspace::workspace_dir_from_id(&state.db, pid) {
                    let real_str = real.to_string_lossy().to_string();
                    if real_str != effective_cwd {
                        log::warn!(
                            "[gateway] workspace session {} cwd mismatch: {} → {}",
                            iid, effective_cwd, real_str
                        );
                        if let Err(e) = state.db.update_session_cwd(&iid, &real_str) {
                            log::warn!("[gateway] update session cwd failed: {}", e);
                        }
                        effective_cwd = real_str.clone();
                    }
                    // 会话文件 stored cwd 始终与 real_dir 对齐（幂等，不一致才写）。
                    if let Some(sp) = session.session_path.as_deref() {
                        if let Err(e) = crate::gateway::workspace::rewrite_session_file_cwd(
                            sp, &real_str,
                        ) {
                            log::warn!("[gateway] rewrite session file cwd failed: {}", e);
                        }
                    }
                }
            }
            let extensions = match session.project_id.as_deref() {
                Some(pid) => effective_project_extensions(&state.db, pid, &effective_cwd),
                None => effective_global_extensions(&state.db, &effective_cwd),
            };
            let session_path = session.session_path.clone();
            let existing_messages: Vec<serde_json::Value> = session_path
                .as_ref()
                .map(|sp| load_session(sp))
                .unwrap_or_default();
            let msg_seq = existing_messages.len() as u64;
            let new_iid = resume_session(
                state,
                &iid,
                &effective_cwd,
                session_path.as_deref(),
                None,
                &extensions,
            )?;
            state
                .inner
                .routes
                .lock()
                .insert(new_iid.clone(), new_iid.clone());
            SessionManager::register_instance(
                &state.session_manager,
                &new_iid,
                &effective_cwd,
                client_id,
            );
            {
                let mut mgr = state.session_manager.lock();
                if let Some(s) = mgr.sessions.get_mut(&new_iid) {
                    s.messages = existing_messages.clone();
                    s.message_seq = msg_seq;
                }
            }
            if let Some(name) = session.name.clone() {
                if !name.trim().is_empty() {
                    state
                        .session_manager
                        .lock()
                        .set_session_name(&new_iid, name);
                }
            }
            send_snapshot(client_tx, &new_iid, &existing_messages, msg_seq);
            Ok(())
        }
    }
}
