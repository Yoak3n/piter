//! Centralized commands sent from gateway to pi instances.

use pi_rpc::command::Command;

use crate::gateway::GatewayState;

/// Send `get_state` to a specific pi instance (fire-and-forget).
/// The pi response is handled by the event loop (`mod.rs` §1c).
pub fn send_get_state(state: &GatewayState, instance_id: &str) {
    if let Some(tx) = state.instance_stdin_tx(instance_id) {
        let _ = tx.send(Command::GetState.to_json_line());
    }
}
