//! Gateway module — HTTP+WS server, client management, message routing.
//!
//! The gateway sits between UI clients and the broker, handling:
//! - WebSocket connections and event broadcasting
//! - REST API endpoints (health, sessions, pi control)
//! - Session lifecycle management
//! - Routing table maintenance
//!
//! The broker handles pi process communication only.

mod broadcast;
pub mod db;
pub mod ext_cache;
pub mod handlers;
mod helper;
mod messages;
pub mod project;
pub mod session_manager;
pub mod state;
pub mod ws;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::Router;
use axum::routing::{delete, get, post, put};
use tokio::sync::{broadcast as tokio_broadcast_channel, mpsc};
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP, EventTx, PROTOCOL_VERSION};
use pi_rpc::event::LIFECYCLE_EVENT_TYPES;
use broadcast::{broadcast_to_clients, broadcast_to_subscribers, push_sessions_list_to_clients};
use helper::discover_lan_ips;
use messages::command;
// ─── Gateway State ─────────────────────────────────────────────────────────

/// Default HTTP port for the gateway. Falls back to an ephemeral port when busy.
const DEFAULT_HTTP_PORT: u16 = 31421;

/// Clone-able state passed into every axum handler via `State`.
#[derive(Clone)]
pub struct GatewayState {
    pub event_tx: EventTx,
    pub inner: Arc<BrokerInner>,
    /// Cached LAN IPs, lazily refreshed with a short TTL so addresses stay
    /// accurate after network changes (e.g. switching WiFi).
    pub lan_ips: Arc<parking_lot::Mutex<(std::time::Instant, Vec<String>)>>,
    pub http_port: u16,
    pub pi_version: String,
    pub pi_exe: PathBuf,
    pub static_dir: PathBuf,
    pub start_time: std::time::Instant,
    /// SQLite database for project/session/extension management.
    pub db: Arc<db::Db>,
    /// Cached discovered extension candidates (global / per-project), filled
    /// at startup and refreshed in the background. DB state is not cached.
    pub extension_cache: Arc<parking_lot::RwLock<HashMap<String, Vec<project::ExtensionEntry>>>>,
    /// Connected UI WebSocket clients.
    pub ui_clients: Arc<parking_lot::Mutex<HashMap<u64, mpsc::UnboundedSender<String>>>>,
    /// Session manager for message tracking and idle lifecycle.
    pub session_manager: Arc<parking_lot::Mutex<session_manager::SessionManager>>,
    /// Agent 完成观察点：会话 agent_end 时回调 (instance_id, session_label)。
    /// 由桌面壳层注册（用于系统通知——托盘隐藏时前端 WS 不可达，系统通知只能走 Rust 侧）；
    /// web / headless 场景保持 None。
    pub agent_end_hook: Arc<parking_lot::Mutex<Option<Box<dyn Fn(&str, &str) + Send + Sync>>>>,
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
        data_dir: PathBuf,
    ) -> Result<(Arc<GatewayState>, u16), String> {
        let bind_port = port.unwrap_or(DEFAULT_HTTP_PORT);
        let std_listener = match std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], bind_port))) {
            Ok(l) => l,
            // 默认端口被占用时回退到随机空闲端口（仅当调用方未显式指定端口时）
            Err(_) if port.is_none() =>
                std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], 0)))
                    .map_err(|e| format!("[gateway] bind fallback failed: {}", e))?,
            Err(e) => return Err(format!("[gateway] bind failed: {}", e)),
        };
        let actual_port = std_listener
            .local_addr()
            .map_err(|e| format!("[gateway] local_addr failed: {}", e))?
            .port();
        std_listener
            .set_nonblocking(true)
            .map_err(|e| format!("[gateway] set_nonblocking failed: {}", e))?;

        let lan_ips = discover_lan_ips();

        let (event_tx, _) = tokio_broadcast_channel::channel(EVENT_CHANNEL_CAP);
        let inner = Arc::new(BrokerInner::default());
        let static_dir = pi_exe
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."))
            .to_path_buf();

        let session_manager = Arc::new(parking_lot::Mutex::new(
            session_manager::SessionManager::new(idle_timeout_secs),
        ));

        let db = db::Db::open(&data_dir).map_err(|e| format!("[gateway] db open failed: {}", e))?;

        let state = Arc::new(GatewayState {
            event_tx: event_tx.clone(),
            inner: inner.clone(),
            lan_ips: Arc::new(parking_lot::Mutex::new((
                std::time::Instant::now(),
                lan_ips.clone(),
            ))),
            http_port: actual_port,
            pi_version,
            pi_exe,
            static_dir,
            start_time: std::time::Instant::now(),
            db,
            extension_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            ui_clients: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            session_manager: session_manager.clone(),
            agent_end_hook: Arc::new(parking_lot::Mutex::new(None)),
        });

        // Warm the extension candidate cache in the background so the first
        // visit to Installed shows a snapshot without a synchronous scan.
        {
            let state = state.clone();
            std::thread::spawn(move || {
                ext_cache::refresh_all(&state);
            });
        }

        // Build axum router
        let app = Router::new()
            // System
            .route("/api/health", get(handlers::system::health_handler))
            .route("/api/lan-info", get(handlers::system::lan_info_handler))
            .route("/api/lan-qr", get(handlers::system::lan_qr_handler))
            .route("/api/git-branch", get(handlers::system::git_branch_handler))
            // Sessions
            .route("/api/sessions", get(handlers::session::sessions_handler))
            .route(
                "/api/load-session",
                get(handlers::session::load_session_handler),
            )
            .route(
                "/api/delete-session",
                get(handlers::session::delete_session_handler),
            )
            // Cross-session search
            .route("/api/search", get(handlers::search::search_handler))
            .route(
                "/api/sessions/create",
                post(handlers::session::create_session_handler),
            )
            .route(
                "/api/sessions/rename",
                post(handlers::session::rename_session_handler),
            )
            // Pi control
            .route("/api/pi/status", get(handlers::pi::pi_status_handler))
            .route("/api/pi/settings", get(handlers::pi::pi_settings_handler))
            .route(
                "/api/pi/model-catalog",
                get(handlers::pi::pi_model_catalog_handler),
            )
            .route("/api/pi/restart", post(handlers::pi::pi_restart_handler))
            .route("/api/pi/stop", post(handlers::pi::pi_stop_handler))
            .route("/api/rpc", post(handlers::pi::rpc_handler))
            .route(
                "/api/rpc/ephemeral",
                post(handlers::pi::rpc_ephemeral_handler),
            )
            // Projects
            .route("/api/projects", get(handlers::project::projects_handler))
            .route(
                "/api/projects",
                post(handlers::project::create_project_handler),
            )
            .route(
                "/api/projects/:id",
                put(handlers::project::update_project_handler),
            )
            .route(
                "/api/projects/:id",
                delete(handlers::project::delete_project_handler),
            )
            .route(
                "/api/projects/:id/pin",
                post(handlers::project::pin_project_handler),
            )
            .route(
                "/api/projects/:id/archive",
                post(handlers::project::archive_project_handler),
            )
            // Extensions & config
            .route(
                "/api/global-extensions",
                get(handlers::extensions::global_extensions_handler),
            )
            .route(
                "/api/global-extensions",
                put(handlers::extensions::update_global_extensions_handler),
            )
            .route(
                "/api/session-config",
                get(handlers::extensions::session_config_handler),
            )
            .route(
                "/api/session-config",
                put(handlers::extensions::update_session_config_handler),
            )
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

    /// Clone the stdin sender for a running instance, if it exists.
    pub fn instance_stdin_tx(&self, instance_id: &str) -> Option<mpsc::UnboundedSender<String>> {
        self.inner
            .instances
            .lock()
            .get(instance_id)
            .and_then(|inst| inst.stdin_tx.clone())
    }

    /// 注册 agent_end 观察回调（桌面壳层用，发送系统通知）。回调运行在
    /// gateway 事件循环线程，必须快速返回、不可 panic。
    pub fn set_agent_end_hook<F>(&self, hook: F)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        *self.agent_end_hook.lock() = Some(Box::new(hook));
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

    /// Current LAN IPs, rediscovered at most once per TTL so the addresses
    /// stay fresh after network changes without spawning a subprocess on
    /// every call.
    pub fn current_lan_ips(&self) -> Vec<String> {
        const LAN_IPS_TTL: std::time::Duration = std::time::Duration::from_secs(2);
        let mut cache = self.lan_ips.lock();
        if cache.0.elapsed() >= LAN_IPS_TTL {
            cache.1 = discover_lan_ips();
            cache.0 = std::time::Instant::now();
        }
        cache.1.clone()
    }

    pub fn lan_urls(&self) -> Vec<String> {
        self.current_lan_ips()
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
            inst.killed.store(true, Ordering::SeqCst);
            let _ = inst.child.kill();
        }
        drop(instances);
        log::info!("[gateway] all pi instances stopped");

        // Mark all tracked sessions unloaded (processes are gone) and push
        // the updated sessions list so clients immediately see the stopped state.
        {
            let mut mgr = self.session_manager.lock();
            let ids: Vec<String> = mgr.sessions.keys().cloned().collect();
            if !ids.is_empty() {
                mgr.mark_unloaded(&ids);
            }
        }
        push_sessions_list_to_clients(self);
    }

    pub fn has_active_processes(&self) -> bool {
        !self.inner.instances.lock().is_empty()
    }
}

// ─── Gateway Server ────────────────────────────────────────────────────────

// ─── Event Loop ────────────────────────────────────────────────────────────

/// Main event loop: subscribe to broker events, maintain routing table,
/// wrap and forward events to WS clients.
async fn run_event_loop(
    state: &Arc<GatewayState>,
    event_rx: &mut tokio_broadcast_channel::Receiver<String>,
) {
    loop {
        match event_rx.recv().await {
            Ok(raw) => {
                process_broker_event(state, &raw);
            }
            Err(tokio_broadcast_channel::error::RecvError::Closed) => {
                log::info!("[gateway] event channel closed, event loop exiting");
                break;
            }
            Err(tokio_broadcast_channel::error::RecvError::Lagged(n)) => {
                log::warn!("[gateway] event loop lagged {} events, skipping", n);
                continue;
            }
        }
    }
}

/// Process a single event from the broker.
fn process_broker_event(state: &Arc<GatewayState>, raw: &str) {
    // 3个卫语句：
    // 1. 解析JSON字符串为serde_json::Value
    // 2. 检查type字段是否存在且为字符串类型
    // 3. 检查instance_id字段是否存在且为字符串类型
    let Ok(val) = serde_json::from_str::<serde_json::Value>(raw) else {
        return;
    };
    let Some(event_type) = val.get("type").and_then(serde_json::Value::as_str) else {
        return;
    };

    // Gateway internal event (no instanceId) — handle before instanceId guard.
    if event_type == "session_cleanup" {
        push_sessions_list_to_clients(state);
        return;
    }

    let Some(iid) = pi_rpc::event::extract_instance_id(&val) else {
        return;
    };


    let instance_id = iid.to_string();

    // ── 1. Response events ─────────────────────────────────────────────
    if event_type == "response" {
        handle_response_event(state, raw, &instance_id);
    }

    // ── 2. Track message in session manager and broadcast ───────────
    track_and_broadcast(state, &val, event_type, &instance_id);

    // ── 3. Push updated sessions list for session-changing events ─────
    if matches!(event_type, "agent_end" | "turn_end") {
        push_sessions_list_to_clients(state);
    }

    // ── 5b. Refresh pi state after agent finishes handling a message ──
    if event_type == "agent_end" {
        command::send_get_state(state, &instance_id);
        // 会话完成通知观察点：向桌面壳层暴露 agent_end（托盘隐藏时前端 WS 不可达，
        // 系统通知只能由 Rust 侧基于此回调发送；label 为空时由壳层回退 instance_id）。
        if let Some(hook) = &*state.agent_end_hook.lock() {
            let label = state
                .session_manager
                .lock()
                .sessions
                .get(&instance_id)
                .and_then(|s| s.session_name.clone())
                .unwrap_or_default();
            hook(&instance_id, &label);
        }
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

/// Handle response-type events: session tracking, get_state triggers, and state completion.
fn handle_response_event(state: &Arc<GatewayState>, raw: &str, instance_id: &str) {
    let Ok(resp) = pi_rpc::event::Response::from_json_line(raw) else {
        return;
    };

    handle_session_response(state, &resp, instance_id);
    handle_get_state_response(state, &resp, instance_id);
    handle_model_response(state, &resp, instance_id);
}

/// 1a/1b. On session-related responses: track assignment + trigger get_state.
fn handle_session_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if !resp.is_session_response() {
        return;
    }

    if let Some(sf) = resp.session_file() {
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
    push_sessions_list_to_clients(state);
    command::send_get_state(state, instance_id);
}

/// 1c. On get_state response → complete pending link + store full state.
fn handle_get_state_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if !resp.success || resp.command != "get_state" {
        return;
    }

    let data = match resp.data.as_ref() {
        Some(d) => d,
        None => return,
    };

    // Extract sessionFile for DB link and routing
    let session_file = data
        .get("sessionFile")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());

    if let Some(sf) = session_file {
        let _ = state.db.complete_session(instance_id, sf);
        state
            .inner
            .routes
            .lock()
            .insert(sf.to_string(), instance_id.to_string());
        let pending = state
            .session_manager
            .lock()
            .pending_links
            .remove(instance_id);
        if pending.is_some() {
            push_sessions_list_to_clients(state);
        }
    }

    // Extract pi's native sessionId and register as route
    let pi_session_id = data
        .get("sessionId")
        .and_then(serde_json::Value::as_str)
        .filter(|s| !s.is_empty());

    if let Some(sid) = pi_session_id {
        state
            .inner
            .routes
            .lock()
            .insert(sid.to_string(), instance_id.to_string());
    }

    // Parse full pi session state and store in session manager
    let model = data.get("model");
    let pi_state = session_manager::PiSessionState {
        session_file: session_file.map(|s| s.to_string()),
        session_id: pi_session_id.map(|s| s.to_string()),
        session_name: data
            .get("sessionName")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        model_id: model
            .and_then(|m| m.get("id"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        model_name: model
            .and_then(|m| m.get("name"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        model_provider: model
            .and_then(|m| m.get("provider"))
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        thinking_level: data
            .get("thinkingLevel")
            .and_then(serde_json::Value::as_str)
            .map(|s| s.to_string()),
        is_streaming: data
            .get("isStreaming")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        is_compacting: data
            .get("isCompacting")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        message_count: data
            .get("messageCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        pending_message_count: data
            .get("pendingMessageCount")
            .and_then(serde_json::Value::as_u64)
            .unwrap_or(0) as u32,
        context_window: model
            .and_then(|m| m.get("contextWindow"))
            .and_then(serde_json::Value::as_u64)
            .map(|v| v as u32),
    };
    state
        .session_manager
        .lock()
        .update_pi_state(instance_id, pi_state);

    // Persist the instance's current model so it can be restored after restart
    if let Some(session) = state.session_manager.lock().sessions.get(instance_id) {
        if let Some(ps) = &session.pi_state {
            let _ = state.db.set_session_model(
                instance_id,
                ps.model_id.as_deref(),
                ps.model_provider.as_deref(),
            );
        }
    }
}

/// 1d. On set_model / cycle_model success → refresh runtime model state and
/// persist it to the DB immediately (no need to wait for the next get_state).
fn handle_model_response(
    state: &Arc<GatewayState>,
    resp: &pi_rpc::event::Response,
    instance_id: &str,
) {
    if !matches!(resp.command.as_str(), "set_model" | "cycle_model") {
        return;
    }
    if !resp.success {
        // set_model 失败：pi 未改模型，default 无需恢复，清掉备份避免残留
        state
            .session_manager
            .lock()
            .sessions
            .get_mut(instance_id)
            .map(|s| s.pending_default_restore = None);
        return;
    }
    let Some(data) = resp.data.as_ref() else {
        return;
    };
    // set_model returns the Model object directly; cycle_model wraps it in {model, ...}
    let model = data.get("model").unwrap_or(data);
    let Some(id) = model.get("id").and_then(serde_json::Value::as_str) else {
        return;
    };
    let provider = model.get("provider").and_then(serde_json::Value::as_str);

    {
        let mut mgr = state.session_manager.lock();
        if let Some(session) = mgr.sessions.get_mut(instance_id) {
            let ps = session.pi_state.get_or_insert_with(Default::default);
            ps.model_id = Some(id.to_string());
            ps.model_provider = provider.map(|s| s.to_string());
            if let Some(name) = model.get("name").and_then(serde_json::Value::as_str) {
                ps.model_name = Some(name.to_string());
            }
        }
        // pi 的 set_model 把 default 写进了 settings.json —— 恢复为切换前的值。
        // 仅当 settings.json 当前 default 仍等于本次 set_model 写入的模型时才回滚：
        // 若窗口内有其它写入（admin 修改 / 其它实例切换），尊重最新值，避免覆盖。
        if let Some((prev_provider, prev_model)) =
            mgr.sessions.get_mut(instance_id).and_then(|s| s.pending_default_restore.take())
        {
            drop(mgr);
            let expected_provider = provider.map(str::to_string).unwrap_or_default();
            if let Ok(cur) = crate::broker::util::read_pi_settings() {
                if cur.default_provider == expected_provider && cur.default_model == id {
                    let _ = crate::broker::util::set_default_model(&prev_provider, &prev_model);
                }
            }
        }
    }
    let _ = state.db.set_session_model(instance_id, Some(id), provider);
    push_sessions_list_to_clients(state);
}

/// Track message in session manager and broadcast to clients.
fn track_and_broadcast(
    state: &Arc<GatewayState>,
    val: &serde_json::Value,
    event_type: &str,
    instance_id: &str,
) {
    let message_seq = if !instance_id.is_empty() {
        state
            .session_manager
            .lock()
            .on_event(val, instance_id)
            .unwrap_or(0)
    } else {
        0
    };

    let is_pi_event = LIFECYCLE_EVENT_TYPES.contains(&event_type);

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
    if !instance_id.is_empty() && state.session_manager.lock().has_subscribers(instance_id) {
        broadcast_to_subscribers(state, instance_id, &envelope_str);
    } else {
        broadcast_to_clients(state, &envelope_str);
    }
}

/// Lightweight runtime info from session manager for enriching project tree.
#[derive(Clone)]
struct RuntimeSessionInfo {
    state: String,
    model: Option<String>,
    model_provider: Option<String>,
    thinking_level: Option<String>,
    message_count: u32,
    message_seq: u64,
    session_name: Option<String>,
    last_active_epoch: u64,
}
