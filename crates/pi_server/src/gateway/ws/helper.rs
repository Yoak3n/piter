use std::sync::Arc;

use serde_json::Value;

use crate::GatewayState;

/// Extract cwd from a UI command payload.
/// Returns `None` if cwd is missing or relative (frontend must send absolute path).
pub fn extract_cwd(value: &Value) -> Option<String> {
    let raw = value
        .pointer("/payload/cwd")
        .and_then(Value::as_str)
        .or_else(|| value.get("cwd").and_then(Value::as_str))?;

    if std::path::Path::new(raw).is_absolute() {
        Some(raw.to_string())
    } else {
        log::warn!("[gateway] rejecting relative cwd: '{}'", raw);
        None
    }
}



/// Resolve instance by instanceId (from message payload). Returns None if
/// not provided or not found in the running instances table.
pub fn resolve_command_instance(value: &Value, state: &Arc<GatewayState>) -> Option<String> {
    let direct_id = value
        .get("instanceId")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/payload/instanceId").and_then(Value::as_str));

    match direct_id {
        Some(iid) if state.inner.instances.lock().contains_key(iid) => Some(iid.to_string()),
        Some(iid) => {
            log::warn!("[gateway] instanceId '{}' not found in running instances", iid);
            None
        }
        None => {
            log::warn!("[gateway] no instanceId in message");
            None
        }
    }
}