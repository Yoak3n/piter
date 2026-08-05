pub mod utils;

use serde_json::{Value, json};
use tokio::sync::mpsc;
pub use utils::*;

use crate::GatewayState;
// Forward a message to a specific instance
pub fn forward_to_instance(
    text: &str,
    value: &Value,
    instance_id: &str,
    state: &GatewayState,
    client_tx: &mpsc::UnboundedSender<String>,
) {
    let msg_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let (forward_text, command_type) = if msg_type == "broker_command" {
        let payload = value.get("payload");
        let cmd = payload.and_then(|p| p.get("type")).and_then(Value::as_str).unwrap_or("");
        let text = payload
            .and_then(|p| serde_json::to_string(p).ok())
            .unwrap_or_else(|| text.to_string());
        (text, cmd)
    } else {
        let cmd = value.get("type").and_then(Value::as_str).unwrap_or("");
        (text.to_string(), cmd)
    };

    let tx = state.instance_stdin_tx(instance_id);
    match tx {
        Some(tx) => {
            // For prompt/steer/follow_up commands, check if model needs switching
            if matches!(command_type, "prompt" | "steer" | "follow_up") {
                if let Some(desired) = extract_desired_model(value) {
                    sync_model_if_needed(state, instance_id, &desired, &tx);
                }
            }
            let _ = tx.send(forward_text);
        }
        None => {
            notify_undeliverable(client_tx, value, "upstream_unavailable");
        }
    }
}

/// Extract desiredModel {id, provider} from the message.
/// Checks both top-level and /payload/desiredModel (for broker_command wrapper).
fn extract_desired_model(value: &Value) -> Option<DesiredModel> {
    let m = value.pointer("/payload/desiredModel")
        .or_else(|| value.pointer("/desiredModel"))
        .or_else(|| value.get("desiredModel"))?;
    let id = m.get("id").and_then(Value::as_str)?;
    let provider = m.get("provider").and_then(Value::as_str).unwrap_or("");
    Some(DesiredModel { id: id.to_string(), provider: provider.to_string() })
}

struct DesiredModel {
    id: String,
    provider: String,
}

/// Compare desired model with session's current model; send set_model RPC if they differ.
fn sync_model_if_needed(
    state: &GatewayState,
    instance_id: &str,
    desired: &DesiredModel,
    tx: &mpsc::UnboundedSender<String>,
) {
    let mgr = state.session_manager.lock();
    let current = mgr.sessions.get(instance_id).and_then(|s| s.pi_state.as_ref());

    let needs_switch = match current {
        Some(ps) => {
            ps.model_id.as_deref() != Some(&desired.id)
                || ps.model_provider.as_deref() != Some(&desired.provider)
        }
        None => true, // no state yet, switch to be safe
    };
    drop(mgr);

    if needs_switch {
        // pi 的 set_model 会把所选模型写进 settings.json 的 default；
        // 先备份当前 default，收到 set_model 成功响应后恢复——default 只允许 admin 修改。
        if let Ok(current) = crate::broker::util::read_pi_settings() {
            let backup = (current.default_provider, current.default_model);
            state
                .session_manager
                .lock()
                .sessions
                .get_mut(instance_id)
                .map(|s| s.pending_default_restore = Some(backup));
        }
        let set_model_cmd = serde_json::json!({
            "type": "set_model",
            "provider": desired.provider,
            "modelId": desired.id
        });
        let _ = tx.send(set_model_cmd.to_string());
        log::info!(
            "[gateway] model sync: switching instance {} to {}/{}",
            instance_id, desired.provider, desired.id
        );
    }
}

pub fn notify_undeliverable(client_tx: &mpsc::UnboundedSender<String>, value: &Value, reason: &str) {
    let request_id = value.get("id").and_then(Value::as_str).unwrap_or("");
    let command = value
        .pointer("/payload/type")
        .and_then(Value::as_str)
        .or_else(|| value.get("type").and_then(Value::as_str))
        .unwrap_or("");
    let _ = client_tx.send(json!({
        "type": "command_undeliverable",
        "protocolVersion": super::PROTOCOL_VERSION,
        "requestId": request_id,
        "command": command,
        "reason": reason,
    }).to_string());
}
