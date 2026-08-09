pub mod command;

pub use command::{handler_broker_command, send_get_state};
use serde_json::{Value, json};
use tokio::sync::mpsc;

pub async fn dispatch_control(value: Value, client_tx: &mpsc::UnboundedSender<String>) {
    let request_id = value.get("requestId").and_then(Value::as_str).unwrap_or("").to_string();
    let command = value.get("command").and_then(Value::as_str).unwrap_or("").to_string();

    let response = match command.as_str() {
        "ping" => json!({"type": "control_response", "requestId": request_id, "ok": true, "result": {"pong": true}}),
        "info" => json!({"type": "control_response", "requestId": request_id, "ok": true, "result": {
            "version": env!("CARGO_PKG_VERSION"),
            "features": ["rpc", "ws", "lan", "health", "multi_instance"],
        }}),
        _ => json!({"type": "control_response", "requestId": request_id, "ok": false, "error": format!("Unknown command: {}", command)}),
    };

    let _ = client_tx.send(response.to_string());
}