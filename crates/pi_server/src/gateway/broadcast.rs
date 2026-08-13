use crate::GatewayState;
use super::state::build_project_session_tree;

/// Send a message to all connected WS clients.
pub fn broadcast_to_clients(state: &GatewayState, msg: &str) {
    let mut clients = state.ui_clients.lock();
    let mut dead = Vec::new();
    for (id, tx) in clients.iter() {
        if tx.send(msg.to_string()).is_err() {
            dead.push(*id);
        }
    }
    for id in dead {
        clients.remove(&id);
    }
}

/// Send a message only to clients subscribed to a specific session.
pub fn broadcast_to_subscribers(state: &GatewayState, instance_id: &str, msg: &str) {
    let subscriber_ids: Vec<u64> = state
        .session_manager
        .lock()
        .sessions
        .get(instance_id)
        .map(|s| s.subscribers.iter().copied().collect())
        .unwrap_or_default();

    if subscriber_ids.is_empty() {
        return;
    }

    let clients = state.ui_clients.lock();
    let mut dead = Vec::new();
    for id in &subscriber_ids {
        if let Some(tx) = clients.get(id) {
            if tx.send(msg.to_string()).is_err() {
                dead.push(*id);
            }
        }
    }
    drop(clients);
    if !dead.is_empty() {
        let mut clients = state.ui_clients.lock();
        for id in dead {
            clients.remove(&id);
        }
    }
}



/// Push the current sessions list directly to all connected WS clients.
/// Builds from database: projects → linked sessions → file metadata.
pub fn push_sessions_list_to_clients(state: &GatewayState) {
    let projects = build_project_session_tree(state);
    if let Ok(json) = serde_json::to_string(&projects) {
        let msg = format!(r#"{{"type":"sessions_list","projects":{}}}"#, json);
        broadcast_to_clients(state, &msg);
    }
}

/// Push the current WS client connection list to all clients
/// (join/leave 广播，分享页「连接客户端」实时刷新)。
pub fn broadcast_connections_list(state: &GatewayState) {
    let conns: Vec<_> = state.connections.lock().values().cloned().collect();
    if let Ok(json) = serde_json::to_string(&conns) {
        let msg = format!(r#"{{"type":"connections_list","connections":{}}}"#, json);
        broadcast_to_clients(state, &msg);
    }
}