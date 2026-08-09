use std::io::BufRead;

use serde_json::{Value, json};
use tokio::sync::mpsc;
use pi_rpc::command::Command;

use super::super::{
    notify_undeliverable, forward_to_instance,
    helper::message::send_snapshot,
};
use crate::{
    GatewayState,
    gateway::{
        broadcast::push_sessions_list_to_clients,
        session_manager::{SessionManager, SessionResult, SessionActivity, PendingFork},
        ws::helper::extract_cwd,
        handlers::session::load_session,
        project::effective_project_extensions
    },
};

/// Send `get_state` to a specific pi instance (fire-and-forget).
/// The pi response is handled by the event loop (`responses.rs` §1c).
pub fn send_get_state(state: &GatewayState, instance_id: &str) {
    if let Some(tx) = state.instance_stdin_tx(instance_id) {
        let _ = tx.send(Command::GetState.to_json_line());
    }
}

/// Send `get_messages` to a specific pi instance (fire-and-forget).
/// fork 后用来重置消息缓存（responses.rs 处理其响应）。
pub fn send_get_messages(state: &GatewayState, instance_id: &str) {
    if let Some(tx) = state.instance_stdin_tx(instance_id) {
        let _ = tx.send(Command::GetMessages.to_json_line());
    }
}

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
        "fork" => handle_fork_command(state, value, client_tx),
        "ack_review" => handle_ack_review(state, value, client_id),
        "deactivate_session" => {
            let iid = value
                .get("instanceId")
                .and_then(Value::as_str)
                .unwrap_or("");
            if !iid.is_empty() {
                log::info!("[gateway] deactivate_session: {} (client {})", iid, client_id);
                state.session_manager.lock().deactivate(&iid, client_id);
            } else {
                log::warn!("[gateway] deactivate_session: missing instanceId");
                notify_undeliverable(client_tx, value, "missing_instanceId");
            }
        }
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
            // The response is handled by the event loop (responses.rs §1c).
            send_get_state(state, &instance_id);

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
            let session_path = db_session.as_ref().and_then(|s| s.session_path.clone());
            // Effective whitelist: global ∪ project − excluded. Resolve the
            // linked project (or by cwd) to apply per-project exclusions.
            let project_id = db_session
                .as_ref()
                .and_then(|s| s.project_id.clone())
                .or_else(|| {
                    state
                        .db
                        .list_projects(true)
                        .into_iter()
                        .find(|p| p.cwd == cwd)
                        .map(|p| p.id)
                });
            let extensions = match project_id {
                Some(pid) => effective_project_extensions(&state.db, &pid, &cwd),
                None => crate::gateway::project::effective_global_extensions(&state.db, &cwd),
            };
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
                    // 恢复 DB 已存的会话名（BUG-018）：register_instance 新建的
                    // ManagedSession 是"无名"状态（title_set=false），若不复原，
                    // 自动命名逻辑会把"内存无名"误判为"新会话"，2 轮后就用新消息
                    // 重新生成标题覆盖 DB 旧名。复用 set_session_name（置名 + title_set=true）。
                    if let Some(name) = db_session.as_ref().and_then(|s| s.name.clone()) {
                        if !name.trim().is_empty() {
                            state
                                .session_manager
                                .lock()
                                .set_session_name(&new_iid, name);
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

/// 消息撤回（fork）：把目标 user 消息的 entryId 透传给 pi 执行 fork。
/// 无感撤回链路：fork response → get_state（更新 DB session_path）→
/// get_messages（重置消息缓存）→ 删旧文件 A + 文件回滚（get_state 确认后）。
fn handle_fork_command(
    state: &GatewayState,
    value: &Value,
    client_tx: &mpsc::UnboundedSender<String>,
) {
    let iid = value
        .get("instanceId")
        .and_then(Value::as_str)
        .unwrap_or("");
    if iid.is_empty() {
        notify_undeliverable(client_tx, &value, "missing_instanceId");
        return;
    }

    // 流式中不撤回（fork 会打断 agent 的进行中状态；该场景走取消/outbox 按钮）
    let busy = state
        .session_manager
        .lock()
        .sessions
        .get(iid)
        .map(|s| s.activity == SessionActivity::Busy)
        .unwrap_or(false);
    if busy {
        notify_undeliverable(client_tx, &value, "fork_busy");
        return;
    }

    let payload = value.get("payload").cloned().unwrap_or(Value::Null);
    let content = payload
        .get("content")
        .and_then(Value::as_str)
        .map(str::to_string);
    let timestamp = payload
        .get("timestamp")
        .and_then(Value::as_i64)
        .filter(|t| *t > 0);
    let rollback = payload
        .get("rollback")
        .and_then(Value::as_bool)
        .unwrap_or(false);

    // 前端一般不持有 entryId：从会话文件按 (content, timestamp) 解析。
    let entry_id = payload
        .get("entryId")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let session_path = state.db.get_session_path(iid);
    let resolved = match entry_id {
        Some(eid) => {
            // 有 entryId 仍取文件里该 entry 的 message.timestamp 作为回滚基准。
            let ts = session_path
                .as_deref()
                .and_then(|p| entry_timestamp(p, &eid))
                .or(timestamp)
                .unwrap_or(0);
            Some((eid, ts))
        }
        None => match session_path.as_deref() {
            Some(path) => resolve_fork_target(path, content.as_deref(), timestamp),
            None => None,
        },
    };
    let Some((entry_id, target_ms)) = resolved else {
        log::warn!("[gateway] fork: no matching user message for instance {}", iid);
        notify_undeliverable(client_tx, &value, "fork_entry_not_found");
        return;
    };

    // 记录 pending：fork response 后转入 cleanup（get_state 确认新文件落盘再删 A + 回滚）
    state.session_manager.lock().set_pending_fork(
        iid,
        PendingFork {
            old_path: session_path.unwrap_or_default(),
            rollback,
            target_ms,
        },
    );

    let request_id = uuid::Uuid::new_v4().to_string();
    let fork_value = json!({
        "type": "broker_command",
        "instanceId": iid,
        "payload": { "type": "fork", "entryId": entry_id, "id": request_id },
    });
    forward_to_instance(&fork_value.to_string(), &fork_value, iid, state, client_tx);
    log::info!(
        "[gateway] fork: instance={} entry={} rollback={}",
        iid, entry_id, rollback
    );
}

/// 从会话文件解析目标 user 消息：(entryId, message.timestamp ms)。
/// 匹配优先级：timestamp 精确（±10s 容差）> content 全文相等；两者同中
/// 直接返回。返回的 timestamp 用作 checkpoint 选取基准。
fn resolve_fork_target(path: &str, content: Option<&str>, timestamp: Option<i64>) -> Option<(String, i64)> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    let mut by_content: Option<(String, i64)> = None;
    let mut best_ts: Option<(String, i64, i64)> = None; // (entry, msg_ts, |diff|)
    for line in reader.lines().flatten() {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(Value::as_str) != Some("message") {
            continue;
        }
        let Some(msg) = value.get("message") else { continue };
        if msg.get("role").and_then(Value::as_str) != Some("user") {
            continue;
        }
        let Some(entry_id) = value.get("id").and_then(Value::as_str) else {
            continue;
        };
        let entry_id = entry_id.to_string();
        let msg_ts = msg.get("timestamp").and_then(Value::as_i64).unwrap_or(0);
        let text = crate::search::extract_text(msg);

        if let Some(ts) = timestamp {
            let diff = (msg_ts - ts).abs();
            let ts_hit = diff <= 10_000;
            if ts_hit && best_ts.as_ref().map_or(true, |(_, _, d)| diff < *d) {
                best_ts = Some((entry_id.clone(), msg_ts, diff));
            }
            if let Some(c) = content {
                if !c.is_empty() && text == c {
                    if ts_hit {
                        return Some((entry_id, msg_ts)); // 双命中，最确定
                    }
                    if by_content.is_none() {
                        by_content = Some((entry_id, msg_ts));
                    }
                }
            }
        } else if let Some(c) = content {
            if !c.is_empty() && text == c && by_content.is_none() {
                by_content = Some((entry_id, msg_ts));
            }
        }
    }
    best_ts.map(|(e, t, _)| (e, t)).or(by_content)
}

/// 取指定 entryId 的 message.timestamp（ms），供已带 entryId 的 fork 用。
fn entry_timestamp(path: &str, entry_id: &str) -> Option<i64> {
    let file = std::fs::File::open(path).ok()?;
    let reader = std::io::BufReader::new(file);
    for line in reader.lines().flatten() {
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("id").and_then(Value::as_str) == Some(entry_id) {
            return value
                .get("message")
                .and_then(|m| m.get("timestamp"))
                .and_then(Value::as_i64);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_file(dir: &std::path::Path) -> std::path::PathBuf {
        let path = dir.join("sess.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"type":"session","timestamp":"2026-08-10T00:00:00Z"}}"#).unwrap();
        writeln!(
            f,
            r#"{{"type":"message","id":"m1","timestamp":"2026-08-10T00:00:01Z","message":{{"role":"user","content":"第一条消息","timestamp":1780000001000}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"message","id":"m2","timestamp":"2026-08-10T00:00:02Z","message":{{"role":"assistant","content":[{{"type":"text","text":"回复"}}],"timestamp":1780000002000}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"message","id":"m3","timestamp":"2026-08-10T00:00:03Z","message":{{"role":"user","content":"第二条消息","timestamp":1780000003000}}}}"#
        )
        .unwrap();
        drop(f);
        path
    }

    #[test]
    fn resolves_entry_by_timestamp_then_content() {
        let dir = tempfile::tempdir().unwrap();
        let path = sample_file(dir.path()).to_string_lossy().to_string();

        // 精确 timestamp → m3
        let (eid, ts) = resolve_fork_target(&path, None, Some(1780000003000)).unwrap();
        assert_eq!(eid, "m3");
        assert_eq!(ts, 1780000003000);

        // 容差内（±10s）→ m1
        let (eid, _) = resolve_fork_target(&path, None, Some(1780000001005)).unwrap();
        assert_eq!(eid, "m1");

        // 仅 content → m1
        let (eid, _) = resolve_fork_target(&path, Some("第一条消息"), None).unwrap();
        assert_eq!(eid, "m1");

        // 双命中直接返回
        let (eid, _) = resolve_fork_target(&path, Some("第二条消息"), Some(1780000003000)).unwrap();
        assert_eq!(eid, "m3");

        // 都匹配不上 → None
        assert!(resolve_fork_target(&path, Some("不存在"), Some(1)).is_none());
    }

    #[test]
    fn entry_timestamp_by_id() {
        let dir = tempfile::tempdir().unwrap();
        let path = sample_file(dir.path()).to_string_lossy().to_string();
        assert_eq!(entry_timestamp(&path, "m2"), Some(1780000002000));
        assert_eq!(entry_timestamp(&path, "nope"), None);
    }
}
