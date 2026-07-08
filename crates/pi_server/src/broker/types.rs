//! Type definitions for the broker module.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::process::Child;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::sync::Mutex;

use parking_lot::Mutex as PLMutex;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::{broadcast, mpsc};

// ─── Channel Types ──────────────────────────────────────────────────────────

/// Broadcast sender for pi stdout events → all WS clients.
pub type EventTx = broadcast::Sender<String>;

pub const EVENT_CHANNEL_CAP: usize = 4096;
pub const PROTOCOL_VERSION: u8 = 1;

// ─── Type Aliases ───────────────────────────────────────────────────────────

pub type ControlHandler = Arc<
    dyn Fn(String, Value, ProgressSink) -> BoxFuture<'static, Result<Value, String>> + Send + Sync,
>;

pub type ProgressSink = Arc<dyn Fn(Value) + Send + Sync>;

pub type BoxFuture<'a, T> = std::pin::Pin<Box<dyn std::future::Future<Output = T> + Send + 'a>>;

// ─── Pi Process State ─────────────────────────────────────────────────────

pub struct PiProcess {
    pub child: Child,
    pub running: Arc<AtomicBool>,
    pub stdin_tx: Option<mpsc::UnboundedSender<String>>,
}

// ─── Pending RPC ────────────────────────────────────────────────────────────

/// One-shot sender for a pending RPC request. When pi responds with a matching
/// requestId, the reader thread routes the response here instead of broadcasting.
pub type PendingRpcSender = tokio::sync::oneshot::Sender<Value>;

// ─── Broker Inner State ────────────────────────────────────────────────────

#[derive(Default)]
pub struct BrokerInner {
    pub ui_clients: Mutex<HashMap<u64, mpsc::UnboundedSender<String>>>,
    pub pi_processes: PLMutex<HashMap<u16, PiProcess>>,
    pub session_ports: PLMutex<HashMap<String, u16>>,
    pub workspace_dedicated: PLMutex<HashMap<u16, Vec<u16>>>,
    pub routes: PLMutex<HashMap<String, u16>>,
    pub disabled_ports: PLMutex<HashSet<u16>>,
    pub active_port: PLMutex<Option<u16>>,
    pub next_client_id: AtomicU64,
    pub control_handler: Mutex<Option<ControlHandler>>,
    /// Pending RPC requests waiting for pi response, keyed by command type.
    pub pending_rpc: PLMutex<HashMap<String, PendingRpcSender>>,
    /// Tracks which session each port is currently serving (port → session_path).
    /// Used to tag outgoing events with sessionPath so the frontend can filter.
    pub port_sessions: PLMutex<HashMap<u16, String>>,
}

// ─── Shared State ─────────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct BrokerState {
    pub event_tx: EventTx,
    pub inner: Arc<BrokerInner>,
    pub lan_ips: Vec<String>,
    pub http_port: u16,
    pub pi_version: String,
    pub pi_exe: PathBuf,
    pub static_dir: PathBuf,
    pub start_time: std::time::Instant,
}

// ─── REST Response Types ────────────────────────────────────────────────────

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub version: String,
    pub pi_version: String,
    pub lan_urls: Vec<String>,
    pub broker_url: String,
    pub uptime_secs: u64,
}

#[derive(Serialize)]
pub struct LanInfoResponse {
    pub broker_ws_url: String,
    pub http_url: String,
    pub lan_urls: Vec<String>,
    pub qr_data: String,
}

#[derive(Serialize)]
pub struct SessionInfo {
    pub id: String,
    pub label: String,
    pub created_at: String,
    pub file_path: String,
    pub updated_at: u64,
    pub preview: String,
    pub cwd: String,
}

#[derive(Serialize)]
pub struct ProjectGroup {
    pub path: String,
    pub dir_name: String,
    pub sessions: Vec<SessionInfo>,
}

#[derive(Serialize)]
pub struct SessionsResponse {
    pub projects: Vec<ProjectGroup>,
}

#[derive(Serialize)]
pub struct QrResponse {
    pub svg: String,
    pub data: String,
}

#[derive(Serialize)]
pub struct GitBranchResponse {
    pub branch: Option<String>,
}

#[derive(Serialize)]
pub struct PiStatusResponse {
    pub running: bool,
    pub port: Option<u16>,
}
