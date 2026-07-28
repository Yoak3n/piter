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

pub fn extract_project_id(value: &Value) -> Option<String> {
    value
        .pointer("/payload/projectId")
        .and_then(Value::as_str)
        .or_else(|| value.get("projectId").and_then(Value::as_str))
        .map(|s| s.to_string())
}



/// Resolve instance by instanceId (primary) or active_instance fallback.
pub fn resolve_command_instance(value: &Value, state: &Arc<GatewayState>) -> Option<String> {
    // Direct instanceId
    let direct_id = value
        .get("instanceId")
        .and_then(Value::as_str)
        .or_else(|| value.pointer("/payload/instanceId").and_then(Value::as_str));

    if let Some(iid) = direct_id {
        if state.inner.instances.lock().contains_key(iid) {
            return Some(iid.to_string());
        }
    }

    // Active instance fallback (only when unambiguous)
    let instance_count = state.inner.instances.lock().len();
    let active = state.inner.active_instance.lock().clone();

    if instance_count > 1 {
        log::warn!("[gateway] ambiguous: {} instances, no instanceId", instance_count);
        return None;
    }

    active
}