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

use axum::extract::State;
use axum::http::{StatusCode, Uri};
use axum::middleware;
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::{delete, get, post, put};
use axum::Router;
use tokio::sync::broadcast as tokio_broadcast_channel;
use tower_http::cors::CorsLayer;

use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP};

use super::{
    db, event_loop::run_event_loop, ext_cache, handlers, helper::discover_lan_ips, lan_auth, mdns,
    migrate, session_manager, workspace, ws, GatewayState,
};

/// Default HTTP port for the gateway. Falls back to an ephemeral port when busy.
/// 高位端口定案：固定默认 31421（与 dev 一致，App/work 需要稳定端口；被占用时回退随机）。
const DEFAULT_HTTP_PORT: u16 = 31421;

impl GatewayState {
    /// Start the gateway HTTP+WS server.
    ///
    /// Returns `(GatewayState, port)`.
    pub fn start_gateway(
        pi_exe: PathBuf,
        pi_version: String,
        dist_path: PathBuf,
        work_dist_path: Option<PathBuf>,
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

        // mDNS 广播注册（_piter._tcp）；失败不致命——扫码/手动输入是保底通路。
        let mdns_reg = match mdns::MdnsRegistration::start(actual_port, &mdns::default_instance_name())
        {
            Ok(reg) => {
                log::info!(
                    "[mdns] 已注册 {}（端口 {}）",
                    reg.fullname(),
                    actual_port
                );
                Some(reg)
            }
            Err(e) => {
                log::warn!("[mdns] 注册失败（不影响 gateway）: {e}");
                None
            }
        };

        // 工作空间基目录与迁移队列在 state 构造前解析（db 随后被 move 进 state）。
        let ws_base_dir = resolve_workspace_base(&db, &static_dir, &data_dir);
        let migration_pending = migrate::load_queue(&data_dir);

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
            chat_dist: dist_path.clone(),
            work_dist: work_dist_path,
            connections: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            extension_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            ui_clients: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            session_manager: session_manager.clone(),
            agent_end_hook: Arc::new(parking_lot::Mutex::new(None)),
            mdns: Arc::new(parking_lot::Mutex::new(mdns_reg)),
            workspace_base_dir: Arc::new(parking_lot::Mutex::new(ws_base_dir)),
            migrations: Arc::new(parking_lot::Mutex::new(migrate::MigrationState {
                pending: migration_pending,
                ..Default::default()
            })),
        });

        // 启动工作空间基目录迁移调度（恢复上次未完成队列 + 后续变更触发）。
        migrate::spawn_migration_task(state.clone());

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
            // mDNS 广播状态
            .route(
                "/api/mdns/status",
                get(handlers::mdns::mdns_status_handler),
            )
            // Workspaces (0.3.0)
            .route("/api/workspaces/base-dir", get(handlers::workspace::get_base_dir_handler))
            .route("/api/workspaces/base-dir", put(handlers::workspace::set_base_dir_handler))
            .route("/api/workspaces", get(handlers::workspace::list_workspaces_handler))
            .route("/api/workspaces", post(handlers::workspace::create_workspace_handler))
            .route("/api/workspaces/:id", get(handlers::workspace::get_workspace_handler))
            .route("/api/workspaces/:id", delete(handlers::workspace::delete_workspace_handler))
            .route("/api/workspaces/:id/mode", put(handlers::workspace::set_workspace_mode_handler))
            .route("/api/workspaces/:id/files", get(handlers::workspace::files_handler))
            .route("/api/workspaces/:id/upload", post(handlers::workspace::upload_handler))
            .route(
                "/api/workspaces/:id/mark-deliverable",
                post(handlers::workspace::mark_deliverable_handler),
            )
            .route("/api/workspaces/:id/artifacts", get(handlers::workspace::artifacts_handler))
            .route(
                "/api/workspaces/:id/deliverables",
                get(handlers::workspace::deliverables_handler),
            )
            .route("/api/workspaces/:id/download", get(handlers::workspace::download_handler))
            .route("/api/workspaces/:id/zip", post(handlers::workspace::zip_handler))
            // WebSocket
            .route("/ws", get(ws::ws_handler))
            .route("/ui-ws", get(ws::ws_handler))
            .route("/chat-ws", get(ws::ws_handler))
            .route("/work-ws", get(ws::ws_handler))
            // 连接客户端列表（分享页数据源）
            .route("/api/connections", get(handlers::system::connections_handler))
            // LAN auth: outermost layer so it gates every route AND the SPA
            // fallback (loopback exempt / auth disabled → transparent).
            .layer(middleware::from_fn_with_state(
                state.clone(),
                lan_auth::lan_auth_middleware,
            ))
            // CORS
            .layer(CorsLayer::permissive())
            // SPA fallback：多前端分发（0.3.0，计划「工作视图与下载」）。
            // 注意：fallback handler 带 `State`，会迫使 Router 保持 state 类型；
            // `with_state` 必须放在最后，使最终 Router<()> 可直接
            // `into_make_service_with_connect_info`（该方法仅对 Router<()> 存在）。
            .fallback(spa_fallback)
            .with_state(state.clone());

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

/// 解析工作空间基目录（0.3.0 文档定案：real_dir = `<基目录>/workspaces/<id>`）：
/// 1. Admin 配置（DB workspace_config）优先；
/// 2. 默认安装目录（static_dir = pi 所在目录）——可能被写入保护（Program Files），
///    首启可写校验，失败回退 app 数据目录（兼容 0.3.0 前的 AppData 位置）。
fn resolve_workspace_base(
    db: &crate::gateway::db::Db,
    static_dir: &std::path::Path,
    data_dir: &std::path::Path,
) -> PathBuf {
    let configured = db.get_workspace_base_dir();
    if !configured.trim().is_empty() && workspace::dir_writable(std::path::Path::new(&configured)) {
        return PathBuf::from(configured);
    }
    if workspace::dir_writable(static_dir) {
        static_dir.to_path_buf()
    } else {
        data_dir.to_path_buf()
    }
}

/// 多 SPA fallback（0.3.0，计划「工作视图与下载」）：非 API/WS 路径按前缀分发。
/// - `/work`、`/work/*`、`/workspaces/:id` → work SPA（history 模式 fallback 到 index.html）
/// - `/` → 重定向 `/chat`
/// - 其余 → chat SPA
/// `/api/*` 与 WS 由上面的显式路由接管，不会走到这里。
///
/// 实现为手写静态文件服务（相对路径清洗防穿越 + 扩展名推断 Content-Type +
/// SPA history fallback），避免 tower-http ServeDir 的响应体类型转换问题。
async fn spa_fallback(
    uri: Uri,
    State(state): State<Arc<GatewayState>>,
) -> Response {
    let path = uri.path();
    if path == "/" {
        return Redirect::temporary("/chat").into_response();
    }
    // SPA 目录页统一无尾斜杠（go_router 不允许尾斜杠路由；相对资源 base 已由
    // index.html 的 <base href> 指定，无尾斜杠不影响加载）。
    if path == "/work/" {
        return Redirect::temporary("/work").into_response();
    }
    if path == "/chat/" {
        return Redirect::temporary("/chat").into_response();
    }
    let (dir, strip_prefix) = if path.starts_with("/work") || path.starts_with("/workspaces") {
        match &state.work_dist {
            Some(d) => (d.clone(), true),
            None => {
                return (StatusCode::NOT_FOUND, "work view not deployed").into_response();
            }
        }
    } else {
        // /chat 前缀同样剥离：页面 URL 带前缀时，相对资源（/chat/xxx.js）按
        // 前缀下分发到 chat 产物。
        (state.chat_dist.clone(), path.starts_with("/chat"))
    };

    // 相对路径清洗：剥离前缀段（/work、/workspaces、/chat），跳过空段，
    // 拒绝 `..` / `\` 段（防目录穿越）。
    let mut ok = true;
    let mut rel = String::new();
    let mut first = true;
    for seg in path.trim_start_matches('/').split('/') {
        if seg.is_empty() {
            continue;
        }
        if strip_prefix && first {
            first = false;
            continue;
        }
        first = false;
        if seg == ".." || seg.contains('\\') {
            ok = false;
            break;
        }
        if !rel.is_empty() {
            rel.push('/');
        }
        rel.push_str(seg);
    }
    if !ok {
        return (StatusCode::BAD_REQUEST, "invalid path").into_response();
    }

    let candidate = dir.join(&rel);
    let file = if candidate.is_file() {
        candidate
    } else {
        // SPA history fallback：未知路径 → index.html
        dir.join("index.html")
    };
    match tokio::fs::read(&file).await {
        Ok(bytes) => {
            let mime = guess_mime(&file);
            ([(axum::http::header::CONTENT_TYPE, mime)], bytes).into_response()
        }
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// 按扩展名推断 Content-Type（覆盖 chat/work 前端静态资源常用类型）。
fn guess_mime(path: &std::path::Path) -> &'static str {
    match path
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("")
    {
        "html" => "text/html; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "map" => "application/json",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "otf" => "font/otf",
        "md" => "text/markdown; charset=utf-8",
        "txt" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}
