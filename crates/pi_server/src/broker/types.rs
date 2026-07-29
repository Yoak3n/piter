//! Type definitions for the broker module.
//!
//! Each pi process is wrapped as a `PiInstance`, keyed by UUID `instance_id`.
//! Instances can be *persistent* (normal session, `--session <path>`) or
//! *ephemeral* (`--no-session`, one-shot for backend queries).

use std::collections::HashMap;
use std::process::Child;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use parking_lot::Mutex as PLMutex;
use tokio::sync::{broadcast, mpsc};

use pi_rpc::event::Response;

// ─── Channel Types ──────────────────────────────────────────────────────────

/// Broadcast sender for pi stdout events → subscribers (gateway, etc.).
pub type EventTx = broadcast::Sender<String>;

pub const EVENT_CHANNEL_CAP: usize = 4096;
pub const PROTOCOL_VERSION: u8 = 1;

// ─── Pi Instance ──────────────────────────────────────────────────────────

/// A single pi process instance.
pub struct PiInstance {
    pub id: String,
    pub child: Child,
    pub running: Arc<std::sync::atomic::AtomicBool>,
    pub stdin_tx: Option<mpsc::UnboundedSender<String>>,
    /// Session file path (known when resuming, None for new sessions).
    pub session_path: Option<String>,
    pub persistent: bool,
    pub cwd: String,
    pub created_at: std::time::Instant,
}

// ─── Pending RPC ────────────────────────────────────────────────────────────

/// A pending RPC request awaiting a pi response.
pub struct PendingRpc {
    pub sender: tokio::sync::oneshot::Sender<Response>,
}

// ─── Broker Inner State ────────────────────────────────────────────────────

#[derive(Default)]
pub struct BrokerInner {
    pub instances: PLMutex<HashMap<String, PiInstance>>,
    pub routes: PLMutex<HashMap<String, String>>,
    pub next_client_id: AtomicU64,
    pub pending_rpc: PLMutex<HashMap<String, PendingRpc>>,
}

// ─── Pi Agent Settings ─────────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PiAgentSettings {
    #[serde(default)]
    pub default_provider: String,
    #[serde(default)]
    pub default_model: String,
    #[serde(default)]
    pub default_thinking_level: String,
    #[serde(default)]
    pub packages: Vec<String>,
}
