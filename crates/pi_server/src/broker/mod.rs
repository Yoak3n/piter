//! Unified HTTP+WebSocket server for Piter — port of Picot's embedded-server.ts
//! and BrokerWs logic into Rust.
//!
//! Provides:
//! - REST API (health, LAN info, sessions, pi control)
//! - WebSocket for bidirectional pi events ↔ clients
//! - LAN access URLs for mobile devices
//! - Serves the Vue SPA for both desktop and mobile clients
//! - Multi-process support for concurrent sessions
//! - Enhanced PATH management for pi's child processes
//!
//! Architecture:
//!
//!   UI Client (WS) ──→ PiBroker (axum) ──→ pi stdin (port N)
//!   pi stdout ──→ PiBroker ──→ broadcast → all UI clients (WS)
//!   REST clients (HTTP) ──→ PiBroker ──→ JSON responses

mod handlers;
mod process;
pub mod types;
mod util;
mod ws;

use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::routing::{get, post};
use axum::Router;
use tokio::sync::broadcast;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

use types::{BrokerInner, BrokerState, EVENT_CHANNEL_CAP};

// ─── Public re-exports ─────────────────────────────────────────────────────

pub use types::SessionsResponse;

// ─── PiBroker ───────────────────────────────────────────────────────────────

/// Unified HTTP+WS server that owns the pi process lifecycle.
pub struct PiBroker {
    port: u16,
    lan_ips: Vec<String>,
    start_time: std::time::Instant,
    pub(crate) inner: Arc<BrokerInner>,
    _runtime: tokio::runtime::Runtime,
}

impl PiBroker {
    /// Start the broker and spawn pi.
    ///
    /// `pi_exe` — path to the pi binary.
    /// `pi_version` — pi version string for display.
    /// `dist_path` — path to the built Vue SPA dist directory.
    /// `port` — optional port override. `None` = bind to random available port.
    pub fn start(
        pi_exe: PathBuf,
        pi_version: String,
        dist_path: PathBuf,
        port: Option<u16>,
    ) -> Result<Self, String> {
        // ── Bind to port ────────────────────────────────────────────────
        let bind_port = port.unwrap_or(0);
        let std_listener = std::net::TcpListener::bind(SocketAddr::from(([0, 0, 0, 0], bind_port)))
            .map_err(|e| format!("[broker] bind failed: {}", e))?;
        let actual_port = std_listener
            .local_addr()
            .map_err(|e| format!("[broker] local_addr failed: {}", e))?
            .port();
        std_listener
            .set_nonblocking(true)
            .map_err(|e| format!("[broker] set_nonblocking failed: {}", e))?;

        // ── Create channels ──────────────────────────────────────────────
        let (event_tx, _) = broadcast::channel(EVENT_CHANNEL_CAP);
        let inner = Arc::new(BrokerInner::default());

        // ── Discover LAN IPs ─────────────────────────────────────────────
        let lan_ips = Self::discover_lan_ips();

        // Static directory is parent of pi_exe (pi's installation dir)
        let static_dir = pi_exe.parent().unwrap_or_else(|| Path::new(".")).to_path_buf();

        let state = Arc::new(BrokerState {
            event_tx: event_tx.clone(),
            inner: inner.clone(),
            lan_ips: lan_ips.clone(),
            http_port: actual_port,
            pi_version: pi_version.clone(),
            pi_exe: pi_exe.clone(),
            static_dir: static_dir.clone(),
            start_time: std::time::Instant::now(),
        });

        // ── Build axum router ────────────────────────────────────────────
        let app = Router::new()
            // REST API
            .route("/api/health", get(handlers::health_handler))
            .route("/api/lan-info", get(handlers::lan_info_handler))
            .route("/api/sessions", get(handlers::sessions_handler))
            .route("/api/lan-qr", get(handlers::lan_qr_handler))
            .route("/api/git-branch", get(handlers::git_branch_handler))
            .route("/api/load-session", get(handlers::load_session_handler))
            .route("/api/delete-session", get(handlers::delete_session_handler))
            .route("/api/sessions/create", post(handlers::create_session_handler))
            .route("/api/sessions/rename", post(handlers::rename_session_handler))
            .route("/api/pi/status", get(handlers::pi_status_handler))
            .route("/api/pi/restart", post(handlers::pi_restart_handler))
            .route("/api/pi/stop", post(handlers::pi_stop_handler))
            .route("/api/rpc", post(handlers::rpc_handler))
            // WebSocket for bidirectional pi communication
            .route("/ws", get(ws::ws_handler))
            .route("/ui-ws", get(ws::ws_handler))
            // CORS so any web client can connect
            .layer(CorsLayer::permissive())
            .with_state(state)
            // Single SPA — all routes (/, /chat, /desktop) handled by vue-router
            .fallback_service(
                ServeDir::new(&dist_path)
                    .fallback(ServeFile::new(dist_path.join("index.html"))),
            );

        // ── Create dedicated runtime and spawn server ─────────────────────
        let runtime = tokio::runtime::Runtime::new()
            .map_err(|e| format!("[broker] failed to create tokio runtime: {}", e))?;

        runtime.spawn(async move {
            let listener = match tokio::net::TcpListener::from_std(std_listener) {
                Ok(l) => l,
                Err(e) => {
                    log::error!("[broker] tokio listener failed: {}", e);
                    return;
                }
            };
            if let Err(e) = axum::serve(listener, app).await {
                log::error!("[broker] server error: {}", e);
            }
        });

        log::info!("[broker] HTTP+WS server started on http://127.0.0.1:{}", actual_port);
        if !lan_ips.is_empty() {
            let lan_urls: Vec<String> = lan_ips
                .iter()
                .map(|ip| format!("http://{}:{}/chat", ip, actual_port))
                .collect();
            log::info!("[broker] LAN access: {}", lan_urls.join(", "));
        }

        let broker = Self {
            port: actual_port,
            lan_ips,
            start_time: std::time::Instant::now(),
            inner,
            _runtime: runtime,
        };

        Ok(broker)
    }

    /// WebSocket URL for the broker.
    pub fn url(&self) -> String {
        format!("ws://127.0.0.1:{}/ws", self.port)
    }

    /// HTTP URL for REST API access.
    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.port)
    }

    pub fn port(&self) -> u16 {
        self.port
    }

    pub fn lan_ips(&self) -> &[String] {
        &self.lan_ips
    }

    pub fn lan_urls(&self) -> Vec<String> {
        self.lan_ips
            .iter()
            .map(|ip| {
                format!(
                    "http://{}:{}/chat?brokerWs=ws://{}:{}/ws",
                    ip, self.port, ip, self.port
                )
            })
            .collect()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }
}
