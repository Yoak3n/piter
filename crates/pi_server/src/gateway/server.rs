//! Gateway 服务器启动：端口绑定、Router 构建、服务线程 spawn。
//!
//! 做什么：`GatewayState::start_gateway` —— 绑定 HTTP 端口（默认端口被占时回退随机端口）、
//! 构建 axum Router（REST + WS + LAN 鉴权中间件 + SPA fallback）、spawn 会话清理任务与
//! HTTP+WS 服务线程 + 事件循环线程。
//! 不做什么：不定义 GatewayState 结构体（state.rs）；不处理 broker 事件（event_loop.rs）。
//! 依赖：state（GatewayState）、db、session_manager、ext_cache、handlers、ws、lan_auth、event_loop。

use std::collections::HashMap;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use axum::middleware;
use axum::routing::{delete, get, post, put};
use axum::Router;
use tokio::sync::broadcast as tokio_broadcast_channel;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP};

use super::{
    db, event_loop::run_event_loop, ext_cache, handlers, helper::discover_lan_ips, lan_auth,
    session_manager, ws, GatewayState,
};

/// Default HTTP port for the gateway. Falls back to an ephemeral port when busy.
const DEFAULT_HTTP_PORT: u16 = 31421;

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
            data_dir,
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
            .route(
                "/api/sessions/:id/pin",
                post(handlers::session::pin_session_handler),
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
            // Budget (monthly usage cap)
            .route("/api/budget", get(handlers::budget::get_budget_handler))
            .route("/api/budget", put(handlers::budget::put_budget_handler))
            .route(
                "/api/budget/status",
                get(handlers::budget::budget_status_handler),
            )
            // LAN auth (PIN + per-device tokens)
            .route(
                "/api/lan/auth",
                post(handlers::lan_auth::lan_auth_handler),
            )
            .route(
                "/api/lan/auth/config",
                get(handlers::lan_auth::get_lan_auth_config_handler),
            )
            .route(
                "/api/lan/auth/config",
                put(handlers::lan_auth::put_lan_auth_config_handler),
            )
            .route(
                "/api/lan/auth/devices",
                get(handlers::lan_auth::lan_auth_devices_handler),
            )
            .route(
                "/api/lan/auth/devices/:id",
                delete(handlers::lan_auth::delete_lan_auth_device_handler),
            )
            .route(
                "/api/lan/auth/revoke",
                post(handlers::lan_auth::lan_auth_revoke_handler),
            )
            // WebSocket
            .route("/ws", get(ws::ws_handler))
            .route("/ui-ws", get(ws::ws_handler))
            // LAN auth: outermost layer so it gates every route AND the SPA
            // fallback (loopback exempt / auth disabled → transparent).
            .layer(middleware::from_fn_with_state(
                state.clone(),
                lan_auth::lan_auth_middleware,
            ))
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
                    if let Err(e) = axum::serve(
                        listener,
                        // ConnectInfo<SocketAddr> → LAN auth can see the peer
                        // address and exempt loopback (desktop) traffic.
                        app.into_make_service_with_connect_info::<SocketAddr>(),
                    )
                    .await
                    {
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
}
