use serde_json::{Value, json};
use tokio::sync::mpsc;



pub fn send_snapshot(
    client_tx: &mpsc::UnboundedSender<String>,
    instance_id: &str,
    messages: &[Value],
    message_seq: u64,
) {
    let msg = json!({
        "type": "session_snapshot",
        "instanceId": instance_id,
        "messages": messages,
        "messageSeq": message_seq,
    });
    log::info!("[gateway] send_snapshot: iid={}, msgs={}, seq={}", instance_id, messages.len(), message_seq);
    if client_tx.send(msg.to_string()).is_err() {
        log::warn!("[gateway] send_snapshot: client_tx send FAILED — channel closed");
    }
}