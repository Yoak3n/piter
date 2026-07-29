//! Gateway module — HTTP+WS server, client management, message routing.
//!
//! The gateway sits between UI clients and the broker, handling:
//! - WebSocket connections and event broadcasting
//! - REST API endpoints (health, sessions, pi control)
//! - Session lifecycle management
//! - Routing table maintenance
//!
//! The broker handles pi process communication only.

pub mod db;
pub mod handlers;
pub mod project;
pub mod session_manager;
pub mod ws;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post, put};
use tokio::sync::{broadcast, mpsc};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP, EventTx, PROTOCOL_VERSION};

// ─── Gateway State ─────────────────────────────────────────────────────────

/// Clone-able state passed into every axum handler via `State`.
#[derive(Clone)]
pub struct GatewayState {
    pub event_tx: EventTx,
    pub inner: Arc<BrokerInner>,
    pub lan_ips: Vec<String>,
    pub http_port: u16,
    pub pi_version: String,
    pub pi_exe: PathBuf,
    pub static_dir: PathBuf,
    pub start_time: std::time::Instant,
    /// SQLite database for project/session/extension management.
    pub db: Arc<db::Db>,
    /// Connected UI WebSocket clients.
    pub ui_clients: Arc<parking_lot::Mutex<HashMap<u64, mpsc::UnboundedSender<String>>>>,
    /// Session manager for message tracking and idle lifecycle.
    pub session_manager: Arc<parking_lot::Mutex<session_manager::SessionManager>>,
}

// ─── GatewayState lifecycle methods ────────────────────────────────────────

impl GatewayState {
    /// Start the gateway HTTP+WS server.
    ///
    /// Returns `(GatewayState, port)`.
    pub fn start_gateway(
        pi_exe: PathBuf,
        pi_version: String,
        dist_path: PathBuf,
        port: Option<u16>,
        idle_timeout_secs: Option<u64>,
    ) -> Result<(Arc<GatewayState>, u16), String> {
        let bind_port = port.unwrap_or(0);
        let std_listener = std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], bind_port)))
            .map_err(|e| format!("[gateway] bind failed: {}", e))?;
        let actual_port = std_listener
            .local_addr()
            .map_err(|e| format!("[gateway] local_addr failed: {}", e))?
            .port();
        std_listener
            .set_nonblocking(true)
            .map_err(|e| format!("[gateway] set_nonblocking failed: {}", e))?;

        let lan_ips = discover_lan_ips();

        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAP);
        let inner = Arc::new(BrokerInner::default());
        let static_dir = pi_exe
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        let session_manager = Arc::new(parking_lot::Mutex::new(
            session_manager::SessionManager::new(idle_timeout_secs),
        ));

        let db = db::Db::open()
            .map_err(|e| format!("[gateway] db open failed: {}", e))?;

        let state = Arc::new(GatewayState {
            event_tx: event_tx.clone(),
            inner: inner.clone(),
            lan_ips: lan_ips.clone(),
            http_port: actual_port,
            pi_version,
            pi_exe,
            static_dir,
            start_time: std::time::Instant::now(),
            db,
            ui_clients: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            session_manager: session_manager.clone(),
        });

        // Build axum router
        let app = Router::new()
            // System
            .route("/api/health", get(handlers::system::health_handler))
            .route("/api/lan-info", get(handlers::system::lan_info_handler))
            .route("/api/lan-qr", get(handlers::system::lan_qr_handler))
            .route("/api/git-branch", get(handlers::system::git_branch_handler))
            // Sessions
            .route("/api/sessions", get(handlers::session::sessions_handler))
            .route("/api/load-session", get(handlers::session::load_session_handler))
            .route("/api/delete-session", get(handlers::session::delete_session_handler))
            .route("/api/sessions/create", post(handlers::session::create_session_handler))
            .route("/api/sessions/rename", post(handlers::session::rename_session_handler))
            // Pi control
            .route("/api/pi/status", get(handlers::pi::pi_status_handler))
            .route("/api/pi/settings", get(handlers::pi::pi_settings_handler))
            .route("/api/pi/restart", post(handlers::pi::pi_restart_handler))
            .route("/api/pi/stop", post(handlers::pi::pi_stop_handler))
            .route("/api/rpc", post(handlers::pi::rpc_handler))
            .route("/api/rpc/ephemeral", post(handlers::pi::rpc_ephemeral_handler))
            // Projects
            .route("/api/projects", get(handlers::project::projects_handler))
            .route("/api/projects", post(handlers::project::create_project_handler))
            .route("/api/projects/:id", put(handlers::project::update_project_handler))
            .route("/api/projects/:id", delete(handlers::project::delete_project_handler))
            .route("/api/projects/:id/pin", post(handlers::project::pin_project_handler))
            .route("/api/projects/:id/archive", post(handlers::project::archive_project_handler))
            // Extensions & config
            .route("/api/global-extensions", get(handlers::extensions::global_extensions_handler))
            .route("/api/global-extensions", put(handlers::extensions::update_global_extensions_handler))
            .route("/api/session-config", get(handlers::extensions::session_config_handler))
            .route("/api/session-config", put(handlers::extensions::update_session_config_handler))
            // WebSocket
            .route("/ws", get(ws::ws_handler))
            .route("/ui-ws", get(ws::ws_handler))
            // CORS
            .layer(CorsLayer::permissive())
            .with_state(state.clone())
            // SPA fallback
            .fallback_service(
                ServeDir::new(&dist_path).fallback(ServeFile::new(dist_path.join("index.html"))),
            );

        // Spawn HTTP+WS server and event loop
        let event_tx_clone = event_tx.clone();
        let state_for_thread = state.clone();
        let sm_for_cleanup = session_manager.clone();
        let inner_for_cleanup = inner;

        // Start session cleanup task
        session_manager::spawn_cleanup_task(
            sm_for_cleanup,
            inner_for_cleanup,
            event_tx_clone.clone(),
            std::time::Duration::from_secs(60),
        );

        std::thread::spawn(move || {
            let runtime =
                tokio::runtime::Runtime::new().expect("[gateway] failed to create tokio runtime");

            runtime.spawn({
                async move {
                    let listener = match tokio::net::TcpListener::from_std(std_listener) {
                        Ok(l) => l,
                        Err(e) => {
                            log::error!("[gateway] tokio listener failed: {}", e);
                            return;
                        }
                    };
                    if let Err(e) = axum::serve(listener, app).await {
                        log::error!("[gateway] server error: {}", e);
                    }
                }
            });

            // Event loop: subscribe to broker events, maintain routing, forward to clients
            runtime.block_on(async move {
                let mut event_rx = event_tx_clone.subscribe();
                run_event_loop(&state_for_thread, &mut event_rx).await;
            });
        });

        log::info!(
            "[gateway] HTTP+WS server started on http://127.0.0.1:{}",
            actual_port
        );
        if !lan_ips.is_empty() {
            log::info!("[gateway] LAN access: {}", state.lan_urls().join(", "));
        }

        Ok((state, actual_port))
    }

    /// Start building a persistent pi process spawn.
    ///
    /// ```ignore
    /// let id = gw.spawn().cwd("/project").extensions(&exts).run()?;
    /// ```
    pub fn spawn(&self) -> crate::broker::process::SpawnBuilder {
        crate::broker::process::SpawnBuilder::new(
            self.inner.clone(),
            self.event_tx.clone(),
            self.pi_exe.clone(),
            self.static_dir.clone(),
            self.pi_version.clone(),
            true, // persistent
        )
    }

    /// Start building an ephemeral pi process spawn (no session).
    pub fn spawn_ephemeral(&self) -> crate::broker::process::SpawnBuilder {
        crate::broker::process::SpawnBuilder::new(
            self.inner.clone(),
            self.event_tx.clone(),
            self.pi_exe.clone(),
            self.static_dir.clone(),
            self.pi_version.clone(),
            false, // ephemeral
        )
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/ws", self.http_port)
    }

    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.http_port)
    }

    pub fn port(&self) -> u16 {
        self.http_port
    }

    pub fn lan_urls(&self) -> Vec<String> {
        self.lan_ips
            .iter()
            .map(|ip| {
                format!(
                    "http://{}:{}/chat?brokerWs=ws://{}:{}/ws",
                    ip, self.http_port, ip, self.http_port
                )
            })
            .collect()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Kill all pi instances.
    pub fn kill_all(&self) {
        use std::sync::atomic::Ordering;
        let mut instances = self.inner.instances.lock();
        for (_, mut inst) in instances.drain() {
            inst.running.store(false, Ordering::SeqCst);
            let _ = inst.child.kill();
        }
        log::info!("[gateway] all pi instances stopped");
    }

    pub fn has_active_processes(&self) -> bool {
        !self.inner.instances.lock().is_empty()
    }
}

// ─── Gateway Server ────────────────────────────────────────────────────────

// ─── Event Loop ────────────────────────────────────────────────────────────

/// pi event types we recognise for envelope wrapping.
const PI_LIFECYCLE_TYPES: &[&str] = &[
    "session_start",
    "session_shutdown",
    "session_name",
    "agent_start",
    "agent_end",
    "turn_start",
    "turn_end",
    "message_start",
    "message_update",
    "message_end",
    "tool_execution_start",
    "tool_execution_update",
    "tool_execution_end",
    "auto_compaction_start",
    "auto_compaction_end",
    "auto_retry_start",
    "auto_retry_end",
    "model_select",
];

/// Main event loop: subscribe to broker events, maintain routing table,
/// wrap and forward events to WS clients.
async fn run_event_loop(state: &Arc<GatewayState>, event_rx: &mut broadcast::Receiver<String>) {
    loop {
        match event_rx.recv().await {
            Ok(raw) => {
                process_broker_event(state, &raw);
            }
            Err(broadcast::error::RecvError::Closed) => {
                log::info!("[gateway] event channel closed, event loop exiting");
                break;
            }
            Err(broadcast::error::RecvError::Lagged(n)) => {
                log::warn!("[gateway] event loop lagged {} events, skipping", n);
                continue;
            }
        }
    }
}

/// Process a single event from the broker.
fn process_broker_event(state: &Arc<GatewayState>, raw: &str) {
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };

    let event_type = val
        .get("type")
        .and_then(serde_json::Value::as_str)
        .unwrap_or("");

    let instance_id = pi_rpc::event::extract_instance_id(&val)
        .unwrap_or("")
        .to_string();
    // ── 1. Response events: track session assignment ───────────────────
    if event_type == "response" {
        if let Ok(resp) = pi_rpc::event::Response::from_json_line(raw) {
            if resp.is_session_response() {
                if let Some(sf) = resp.session_file() {
                    let instance_id = pi_rpc::event::extract_instance_id(&val).unwrap_or("");
                    if !instance_id.is_empty() {
                        log::info!(
                            "[gateway] pi confirmed session: instance={} session={}",
                            instance_id,
                            sf
                        );
                        state
                            .inner
                            .routes
                            .lock()
                            .insert(sf.to_string(), instance_id.to_string());
                    }
                }
                push_sessions_list_to_clients(state);
            }
        }
    }

    // ── 1b. On new_session/switch_session success → query get_state ───
    if event_type == "response" && !instance_id.is_empty() {
        if let Ok(resp) = pi_rpc::event::Response::from_json_line(raw) {
            if resp.success && matches!(resp.command.as_str(), "new_session" | "switch_session") {
                // Send get_state to learn sessionId and sessionFile
                if let Some(inst) = state.inner.instances.lock().get(&instance_id) {
                    if let Some(tx) = &inst.stdin_tx {
                        let get_state = serde_json::json!({"type": "get_state"}).to_string();
                        let _ = tx.send(get_state);
                    }
                }
            }
        }
    }

    // ── 1c. On get_state response → complete pending link + store full state ──
    if event_type == "response" && !instance_id.is_empty() {
        if let Ok(resp) = pi_rpc::event::Response::from_json_line(raw) {
            if resp.success && resp.command == "get_state" {
                let data = resp.data.as_ref();

                // Extract sessionFile for DB link and routing
                let session_file = data
                    .and_then(|d| d.get("sessionFile").and_then(serde_json::Value::as_str))
                    .filter(|s| !s.is_empty());

                if let Some(sf) = session_file {
                    // Complete session record with actual file path from pi
                    let _ = state.db.complete_session(&instance_id, sf);
                    state.inner.routes.lock().insert(sf.to_string(), instance_id.clone());
                    // Check if there was a pending project link
                    let pending = state.session_manager.lock().pending_links.remove(&instance_id);
                    if pending.is_some() {
                        push_sessions_list_to_clients(state);
                    }
                }

                // Extract pi's native sessionId and register as route
                let pi_session_id = data
                    .and_then(|d| d.get("sessionId").and_then(serde_json::Value::as_str))
                    .filter(|s| !s.is_empty());

                if let Some(sid) = pi_session_id {
                    state.inner.routes.lock().insert(sid.to_string(), instance_id.clone());
                }

                // Parse full pi session state and store in session manager
                let pi_state = session_manager::PiSessionState {
                    session_file: session_file.map(|s| s.to_string()),
                    session_id: pi_session_id.map(|s| s.to_string()),
                    session_name: data.and_then(|d| d.get("sessionName").and_then(serde_json::Value::as_str)).map(|s| s.to_string()),
                    model_id: data.and_then(|d| d.get("model").and_then(|m| m.get("id")).and_then(serde_json::Value::as_str)).map(|s| s.to_string()),
                    model_name: data.and_then(|d| d.get("model").and_then(|m| m.get("name")).and_then(serde_json::Value::as_str)).map(|s| s.to_string()),
                    model_provider: data.and_then(|d| d.get("model").and_then(|m| m.get("provider")).and_then(serde_json::Value::as_str)).map(|s| s.to_string()),
                    thinking_level: data.and_then(|d| d.get("thinkingLevel").and_then(serde_json::Value::as_str)).map(|s| s.to_string()),
                    is_streaming: data.and_then(|d| d.get("isStreaming").and_then(serde_json::Value::as_bool)).unwrap_or(false),
                    is_compacting: data.and_then(|d| d.get("isCompacting").and_then(serde_json::Value::as_bool)).unwrap_or(false),
                    message_count: data.and_then(|d| d.get("messageCount").and_then(serde_json::Value::as_u64)).unwrap_or(0) as u32,
                    pending_message_count: data.and_then(|d| d.get("pendingMessageCount").and_then(serde_json::Value::as_u64)).unwrap_or(0) as u32,
                    context_window: data.and_then(|d| d.get("model").and_then(|m| m.get("contextWindow")).and_then(serde_json::Value::as_u64)).map(|v| v as u32),
                };
                state.session_manager.lock().update_pi_state(&instance_id, pi_state);
            }
        }
    }

    // ── 4. Track message in session manager and broadcast ───────────
    let message_seq = if !instance_id.is_empty() {
        state.session_manager.lock().on_event(&val, &instance_id).unwrap_or(0)
    } else {
        0
    };

    let is_pi_event = PI_LIFECYCLE_TYPES.contains(&event_type);

    let envelope = if is_pi_event {
        serde_json::json!({
            "type": "event",
            "event": val,
            "instanceId": instance_id,
            "messageSeq": message_seq,
            "protocolVersion": PROTOCOL_VERSION,
        })
    } else {
        serde_json::json!({
            "type": event_type,
            "payload": val,
            "instanceId": instance_id,
            "messageSeq": message_seq,
            "protocolVersion": PROTOCOL_VERSION,
        })
    };

    // If instance has subscribers, send only to them; otherwise broadcast to all
    let envelope_str = envelope.to_string();
    if !instance_id.is_empty() && state.session_manager.lock().has_subscribers(&instance_id) {
        broadcast_to_subscribers(state, &instance_id, &envelope_str);
    } else {
        broadcast_to_clients(state, &envelope_str);
    }

    // ── 5. Push updated sessions list for session-changing events ─────
    if matches!(
        event_type,
        "session_start" | "session_shutdown" | "agent_end" | "turn_end"
    ) {
        push_sessions_list_to_clients(state);
    }

    // ── 6. Push if session state changed (active↔idle transitions) ───
    if event_type == "session_cleanup" {
        push_sessions_list_to_clients(state);
        return;
    }

    // Persist any auto-generated session names to DB
    let pending_names = state.session_manager.lock().take_pending_names();
    for (iid, name) in &pending_names {
        let _ = state.db.set_session_name(iid, name);
    }

    if state.session_manager.lock().take_dirty() || !pending_names.is_empty() {
        push_sessions_list_to_clients(state);
    }
}

/// Send a message to all connected WS clients.
fn broadcast_to_clients(state: &GatewayState, msg: &str) {
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
fn broadcast_to_subscribers(state: &GatewayState, instance_id: &str, msg: &str) {
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

/// Build project-session tree from database + session file metadata + runtime state.
pub fn build_project_session_tree(state: &GatewayState) -> Vec<handlers::ProjectGroup> {
    use handlers::{ProjectGroup, SessionInfo};
    use std::collections::HashMap;

    // Build lookup: session_file_path → (instance_id, state_info) from session manager
    let mgr = state.session_manager.lock();
    let mut runtime_by_iid: HashMap<String, RuntimeSessionInfo> = HashMap::new();
    for session in mgr.sessions.values() {
        let info = RuntimeSessionInfo {
            state: match &session.state {
                session_manager::SessionState::Active => "active".to_string(),
                session_manager::SessionState::Idle { .. } => "idle".to_string(),
                session_manager::SessionState::Unloaded => "unloaded".to_string(),
            },
            model: session.pi_state.as_ref().and_then(|p| p.model_id.clone()),
            thinking_level: session.pi_state.as_ref().and_then(|p| p.thinking_level.clone()),
            message_count: session.messages.len() as u32,
            message_seq: session.message_seq,
            session_name: session.session_name.clone(),
            last_active_epoch: session.last_active_epoch,
        };
        runtime_by_iid.insert(session.instance_id.clone(), info);
    }
    drop(mgr);

    // Single DB query for all sessions (avoid O(n²))
    let all_db_sessions = state.db.all_sessions();
    let db_by_iid: HashMap<String, db::SessionRow> = all_db_sessions
        .into_iter()
        .map(|s| (s.instance_id.clone(), s))
        .collect();

    let db_projects = super::gateway::project::list_projects(&state.db, false);

    let mut result: Vec<ProjectGroup> = Vec::new();

    for proj in &db_projects {
        let instance_ids = state.db.get_project_sessions(&proj.id);
        let mut sessions: Vec<SessionInfo> = Vec::new();

        for iid in &instance_ids {
            let rt = runtime_by_iid.get(iid);
            let db_row = db_by_iid.get(iid);

            // Label: runtime auto-title > DB name > instance id fallback
            let label = rt
                .and_then(|r| r.session_name.clone())
                .or_else(|| db_row.and_then(|r| r.name.clone()))
                .unwrap_or_else(|| iid.chars().take(8).collect());

            let state_str = rt
                .map(|r| r.state.clone())
                .unwrap_or_else(|| "unloaded".to_string());

            sessions.push(SessionInfo {
                id: iid.clone(),
                label,
                created_at: String::new(),
                file_path: db_row
                    .and_then(|r| r.session_path.clone())
                    .unwrap_or_default(),
                updated_at: rt.map(|r| r.last_active_epoch).unwrap_or_else(|| {
                    // Parse DB created_at RFC3339 string to epoch
                    db_row.and_then(|r| chrono::DateTime::parse_from_rfc3339(&r.created_at).ok())
                        .map(|dt| dt.timestamp() as u64)
                        .unwrap_or(0)
                }),
                preview: String::new(),
                cwd: proj.cwd.clone(),
                instance_id: Some(iid.clone()),
                state: Some(state_str),
                model: rt.and_then(|r| r.model.clone()),
                thinking_level: rt.and_then(|r| r.thinking_level.clone()),
                message_count: rt.map(|r| r.message_count).unwrap_or(0),
                message_seq: rt.map(|r| r.message_seq).unwrap_or(0),
            });
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        result.push(ProjectGroup {
            path: proj.cwd.clone(),
            dir_name: proj.name.clone(),
            sessions,
        });
    }

    // Orphaned sessions (in DB but no project)
    let all_linked: std::collections::HashSet<String> = result
        .iter()
        .flat_map(|p| p.sessions.iter().filter_map(|s| s.instance_id.clone()))
        .collect();

    let orphans: Vec<SessionInfo> = db_by_iid
        .values()
        .filter(|s| s.project_id.is_none() && !all_linked.contains(&s.instance_id))
        .map(|s| {
            let rt = runtime_by_iid.get(&s.instance_id);
            let label = s.name.clone()
                .or_else(|| rt.and_then(|r| r.session_name.clone()))
                .unwrap_or_else(|| s.instance_id.chars().take(8).collect());
            SessionInfo {
                id: s.instance_id.clone(),
                label,
                created_at: String::new(),
                file_path: s.session_path.clone().unwrap_or_default(),
                updated_at: rt.map(|r| r.message_count as u64).unwrap_or(0),
                preview: String::new(),
                cwd: s.cwd.clone(),
                instance_id: Some(s.instance_id.clone()),
                state: Some(rt.map(|r| r.state.clone()).unwrap_or_else(|| "unloaded".to_string())),
                model: rt.and_then(|r| r.model.clone()),
                thinking_level: None,
                message_count: rt.map(|r| r.message_count).unwrap_or(0),
                message_seq: rt.map(|r| r.message_seq).unwrap_or(0),
            }
        })
        .collect();

    if !orphans.is_empty() {
        result.push(ProjectGroup {
            path: String::new(),
            dir_name: "Other".to_string(),
            sessions: orphans,
        });
    }

    result
}

/// Lightweight runtime info from session manager for enriching project tree.
#[derive(Clone)]
struct RuntimeSessionInfo {
    state: String,
    model: Option<String>,
    thinking_level: Option<String>,
    message_count: u32,
    message_seq: u64,
    session_name: Option<String>,
    last_active_epoch: u64,
}


// ─── LAN IP Discovery ──────────────────────────────────────────────────────

pub fn discover_lan_ips() -> Vec<String> {
    let mut ips: Vec<String> = Vec::new();

    // Primary: UDP socket trick — connect to public DNS to find route IP
    if let Ok(socket) = std::net::UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:53").is_ok() {
            if let Ok(addr) = socket.local_addr() {
                let ip = addr.ip();
                if is_private_ipv4(ip) {
                    ips.push(ip.to_string());
                }
            }
        }
    }

    #[cfg(target_os = "windows")]
    {
        if let Ok(output) = std::process::Command::new("ipconfig").output() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                let line = line.trim();
                if line.contains("IPv4") && line.contains(':') {
                    if let Some(ip_str) = line.split(':').next_back() {
                        let ip_str = ip_str.trim();
                        if let Ok(addr) = ip_str.parse::<std::net::IpAddr>() {
                            if is_private_ipv4(addr) && !ips.contains(&ip_str.to_string()) {
                                ips.push(ip_str.to_string());
                            }
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        for (cmd, args) in [("ifconfig", &["-a"] as &[&str]), ("ip", &["addr"])] {
            if let Ok(output) = std::process::Command::new(cmd).args(args).output() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    let trimmed = line.trim();
                    if let Some(inet) = trimmed.strip_prefix("inet ") {
                        if let Some(ip_part) = inet.split_whitespace().next() {
                            if let Ok(addr) = ip_part.parse::<std::net::IpAddr>() {
                                if is_private_ipv4(addr) && !ips.contains(&ip_part.to_string()) {
                                    ips.push(ip_part.to_string());
                                }
                            }
                        }
                    }
                }
                if !ips.is_empty() {
                    break;
                }
            }
        }
    }

    ips
}

fn is_private_ipv4(ip: std::net::IpAddr) -> bool {
    if !ip.is_ipv4() || ip.is_loopback() {
        return false;
    }
    match ip {
        std::net::IpAddr::V4(v4) => {
            matches!(v4.octets(), [10, ..] | [172, 16..=31, ..] | [192, 168, ..])
        }
        _ => false,
    }
}
