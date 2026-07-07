//! Unified HTTP+WebSocket server for Piter — port of Picot's embedded-server.ts
//! and BrokerWs logic into Rust.
//!
//! Provides:
//! - REST API  (health, LAN info, sessions, pi control)
//! - WebSocket for bidirectional pi events ↔ clients
//! - LAN access URLs for mobile devices
//! - Serves the Vue SPA for both desktop and mobile clients
//!
//! Architecture:
//!
//!   UI Client (WS) ──→ PiBroker (axum) ──→ pi stdin
//!   pi stdout ──→ PiBroker ──→ broadcast → all UI clients (WS)
//!   REST clients (HTTP) ──→ PiBroker ──→ JSON responses
//!
//! Pi lifecycle is owned by the broker — it starts pi on creation and
//! exposes restart/stop via REST API.

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use axum::{
    Router,
    routing::{get, post},
    response::{IntoResponse, Json},
    extract::{Query, State, WebSocketUpgrade, ws::{Message, WebSocket}},
};
use parking_lot::Mutex;
use serde::Serialize;
use tokio::sync::{broadcast, mpsc};
use tokio::sync::mpsc::error::TryRecvError;
use tower_http::cors::CorsLayer;
use tower_http::services::{ServeDir, ServeFile};

// ─── Channel Types ──────────────────────────────────────────────────────────

/// Broadcast sender for pi stdout events → all WS clients.
pub type EventTx = broadcast::Sender<String>;

/// Sender half for WS clients to push commands.
pub type CommandTx = mpsc::UnboundedSender<String>;

const EVENT_CHANNEL_CAP: usize = 4096;

// ─── Pi Process State ─────────────────────────────────────────────────────

struct PiProcess {
    child: Child,
    running: Arc<AtomicBool>,
}

// ─── Shared State ───────────────────────────────────────────────────────────

#[derive(Clone)]
struct BrokerState {
    event_tx: EventTx,
    command_tx: CommandTx,
    lan_ips: Vec<String>,
    http_port: u16,
    pi_version: String,
    pi_exe: PathBuf,
    start_time: std::time::Instant,
    pi_handle: Arc<Mutex<Option<PiProcess>>>,
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
}

// ─── PiBroker ───────────────────────────────────────────────────────────────

/// Unified HTTP+WS server that owns the pi process lifecycle.
pub struct PiBroker {
    port: u16,
    lan_ips: Vec<String>,
    start_time: std::time::Instant,
    pi_handle: Arc<Mutex<Option<PiProcess>>>,
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
        let (command_tx, command_rx) = mpsc::unbounded_channel::<String>();

        // ── Spawn pi ─────────────────────────────────────────────────────
        let pi_handle = Arc::new(Mutex::new(None));
        spawn_pi(&pi_exe, &event_tx, command_rx, &pi_handle)?;

        // ── Discover LAN IPs ─────────────────────────────────────────────
        let lan_ips = Self::discover_lan_ips();

        let state = Arc::new(BrokerState {
            event_tx: event_tx.clone(),
            command_tx: command_tx.clone(),
            lan_ips: lan_ips.clone(),
            http_port: actual_port,
            pi_version,
            pi_exe: pi_exe.clone(),
            start_time: std::time::Instant::now(),
            pi_handle: pi_handle.clone(),
        });

        // ── Build axum router ────────────────────────────────────────────
        let app = Router::new()
            // REST API
            .route("/api/health", get(health_handler))
            .route("/api/lan-info", get(lan_info_handler))
            .route("/api/sessions", get(sessions_handler))
            .route("/api/lan-qr", get(lan_qr_handler))
            .route("/api/git-branch", get(git_branch_handler))
            .route("/api/load-session", get(load_session_handler))
            .route("/api/delete-session", get(delete_session_handler))
            .route("/api/sessions/create", post(create_session_handler))
            .route("/api/sessions/rename", post(rename_session_handler))
            .route("/api/pi/status", get(pi_status_handler))
            .route("/api/pi/restart", post(pi_restart_handler))
            .route("/api/pi/stop", post(pi_stop_handler))
            .route("/api/rpc", post(rpc_handler))
            // WebSocket for bidirectional pi communication
            .route("/ws", get(ws_handler))
            .route("/ui-ws", get(ws_handler))
            // CORS so any web client can connect
            .layer(CorsLayer::permissive())
            .with_state(state)
            // Single SPA — all routes (/, /chat, /desktop) handled by vue-router
            .fallback_service(
                ServeDir::new(&dist_path)
                    .fallback(ServeFile::new(dist_path.join("index.html"))),
            );

        // ── Spawn server on Tauri's async runtime ────────────────────────
        tauri::async_runtime::spawn(async move {
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

        Ok(Self {
            port: actual_port,
            lan_ips,
            start_time: std::time::Instant::now(),
            pi_handle,
        })
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

    /// Whether pi is currently running.
    pub fn is_running(&self) -> bool {
        self.pi_handle
            .lock()
            .as_ref()
            .map(|h| h.running.load(Ordering::SeqCst))
            .unwrap_or(false)
    }

    /// Stop pi if running.
    pub fn stop_pi(&self) {
        if let Some(mut p) = self.pi_handle.lock().take() {
            p.running.store(false, Ordering::SeqCst);
            let _ = p.child.kill();
            let _ = p.child.wait();
            log::info!("[broker] pi stopped");
        }
    }

    /// Restart pi (stop current; full restart via REST API pending).
    pub fn restart_pi(&self) -> Result<(), String> {
        self.stop_pi();
        Err("Full restart via REST not yet implemented — pi stopped, reconnect required".into())
    }

    // ─── LAN IP Discovery ────────────────────────────────────────────────

    fn discover_lan_ips() -> Vec<String> {
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
            for (cmd, args) in &[("ifconfig", &["-a"] as &[&str]), ("ip", &["addr"])] {
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
}

/// Check if an IPv4 address is in a private (RFC 1918) range.
fn is_private_ipv4(ip: std::net::IpAddr) -> bool {
    if !ip.is_ipv4() || ip.is_loopback() {
        return false;
    }
    match ip {
        std::net::IpAddr::V4(v4) => match v4.octets() {
            [10, _, _, _] => true,
            [172, b, _, _] if (16..=31).contains(&b) => true,
            [192, 168, _, _] => true,
            _ => false,
        },
        _ => false,
    }
}

// ─── Pi Spawn ──────────────────────────────────────────────────────────────

fn stub_extension_path() -> String {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("resources")
        .join("extensions")
        .join("stub.mjs")
        .to_string_lossy()
        .to_string()
}

/// Spawn pi without --session (creates default session internally)
/// and wire up stdout → event_tx, command_rx → stdin.
fn spawn_pi(
    pi_exe: &PathBuf,
    event_tx: &EventTx,
    mut command_rx: mpsc::UnboundedReceiver<String>,
    pi_handle: &Arc<Mutex<Option<PiProcess>>>,
) -> Result<(), String> {
    let args: Vec<String> = vec![
        "--extension".to_string(),
        stub_extension_path(),
        "--mode".to_string(),
        "rpc".to_string(),
    ];

    // Use the project root (parent of src-tauri) as pi's working directory.
    // This ensures pi creates sessions with the correct cwd instead of
    // always using the src-tauri directory.
    let cwd = std::env::current_dir()
        .ok()
        .and_then(|p| {
            let parent = p.parent().map(|parent| parent.to_path_buf());
            parent
        })
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|| ".".to_string());

    let mut child = if cfg!(target_os = "windows") {
        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            Command::new(pi_exe)
                .args(&args)
                .current_dir(&cwd)
                .creation_flags(CREATE_NO_WINDOW)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .spawn()
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(std::io::Error::new(std::io::ErrorKind::Other, "unreachable"))
        }
    } else {
        Command::new(pi_exe)
            .args(&args)
            .current_dir(&cwd)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
    }
    .map_err(|e| format!("Failed to spawn pi: {}", e))?;

    let stdout = child.stdout.take().ok_or_else(|| "No stdout".to_string())?;
    let mut stdin = child.stdin.take().ok_or_else(|| "No stdin".to_string())?;

    let running = Arc::new(AtomicBool::new(true));
    let running_r = running.clone();
    let running_w = running.clone();
    let ev_tx = event_tx.clone();

    // Reader thread: pi stdout → event_tx broadcast
    std::thread::spawn(move || {
        let reader = BufReader::new(stdout);
        for line in reader.lines() {
            if !running_r.load(Ordering::SeqCst) {
                break;
            }
            if let Ok(text) = line {
                let trimmed = text.trim();
                if trimmed.is_empty() {
                    continue;
                }
                // Detect agent lifecycle events for session status push
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(trimmed) {
                    if let Some(evt_type) = val.get("type").and_then(|t| t.as_str()) {
                        match evt_type {
                            "agent_start" => {
                                let _ = ev_tx.send(
                                    r#"{"type":"session_status","status":"running"}"#.to_string(),
                                );
                            }
                            "agent_end" | "turn_end" => {
                                let _ = ev_tx.send(
                                    r#"{"type":"session_status","status":"idle"}"#.to_string(),
                                );
                            }
                            "response" => {
                                let cmd = val
                                    .get("command")
                                    .and_then(|c| c.as_str())
                                    .unwrap_or("");
                                // Pi may create session files lazily (on first
                                // prompt). Poll for the file to appear, then
                                // push sessions_list so the frontend refreshes.
                                if cmd == "new_session" {
                                    if let Some(sf) = val
                                        .get("data")
                                        .and_then(|d| d.get("sessionFile"))
                                        .and_then(|v| v.as_str())
                                    {
                                        let session_file = sf.to_string();
                                        let tx = ev_tx.clone();
                                        std::thread::spawn(move || {
                                            let path =
                                                std::path::PathBuf::from(&session_file);
                                            for _ in 0..60 {
                                                std::thread::sleep(
                                                    std::time::Duration::from_millis(500),
                                                );
                                                if path.exists() {
                                                    let sessions =
                                                        build_sessions_response();
                                                    if let Ok(json) = serde_json::to_string(
                                                        &sessions.projects,
                                                    ) {
                                                        let msg = format!(
                                                            r#"{{"type":"sessions_list","projects":{}}}"#,
                                                            json
                                                        );
                                                        let _ = tx.send(msg);
                                                    }
                                                    break;
                                                }
                                            }
                                        });
                                    }
                                }
                                if cmd == "switch_session" || cmd == "new_session" {
                                    let sessions = build_sessions_response();
                                    if let Ok(json) =
                                        serde_json::to_string(&sessions.projects)
                                    {
                                        let msg = format!(
                                            r#"{{"type":"sessions_list","projects":{}}}"#,
                                            json
                                        );
                                        let _ = ev_tx.send(msg);
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                if ev_tx.send(trimmed.to_string()).is_err() {
                    break; // no more receivers
                }
            }
        }
        running_r.store(false, Ordering::SeqCst);
    });

    // Writer thread: command_rx → pi stdin
    std::thread::spawn(move || {
        loop {
            match command_rx.try_recv() {
                Ok(mut cmd) => {
                    if !running_w.load(Ordering::SeqCst) {
                        break;
                    }
                    if !cmd.ends_with('\n') {
                        cmd.push('\n');
                    }
                    if stdin.write_all(cmd.as_bytes()).is_err() {
                        log::error!("[broker] failed to write to pi stdin");
                        break;
                    }
                    if stdin.flush().is_err() {
                        log::error!("[broker] failed to flush pi stdin");
                        break;
                    }
                }
                Err(TryRecvError::Empty) => {
                    if !running_w.load(Ordering::SeqCst) {
                        break;
                    }
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(TryRecvError::Disconnected) => {
                    log::info!("[broker] command channel closed, writer exiting");
                    break;
                }
            }
        }
        running_w.store(false, Ordering::SeqCst);
    });

    *pi_handle.lock() = Some(PiProcess { child, running });

    log::info!("[broker] pi started (pid={})", pi_handle.lock().as_ref().unwrap().child.id());
    Ok(())
}

// ─── REST API Handlers ──────────────────────────────────────────────────────

async fn health_handler(
    State(state): State<Arc<BrokerState>>,
) -> Json<HealthResponse> {
    let lan_urls: Vec<String> = state
        .lan_ips
        .iter()
        .map(|ip| format!("http://{}:{}/chat", ip, state.http_port))
        .collect();

    Json(HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        pi_version: state.pi_version.clone(),
        lan_urls,
        broker_url: format!("ws://127.0.0.1:{}/ws", state.http_port),
        uptime_secs: state.start_time.elapsed().as_secs(),
    })
}

async fn lan_info_handler(
    State(state): State<Arc<BrokerState>>,
) -> Json<LanInfoResponse> {
    let lan_urls: Vec<String> = state
        .lan_ips
        .iter()
        .map(|ip| format!("http://{}:{}/chat", ip, state.http_port))
        .collect();

    let qr_data = lan_urls
        .first()
        .cloned()
        .unwrap_or_else(|| format!("http://127.0.0.1:{}/chat", state.http_port));

    Json(LanInfoResponse {
        broker_ws_url: format!("ws://127.0.0.1:{}/ws", state.http_port),
        http_url: format!("http://127.0.0.1:{}/", state.http_port),
        lan_urls,
        qr_data,
    })
}

async fn sessions_handler() -> Json<SessionsResponse> {
    Json(build_sessions_response())
}

fn build_sessions_response() -> SessionsResponse {
    let sessions_root = get_sessions_dir();
    let mut all_sessions: Vec<(String, SessionInfo)> = Vec::new();

    // Traverse project subdirectories
    if let Ok(entries) = std::fs::read_dir(&sessions_root) {
        for entry in entries.flatten() {
            let project_dir = entry.path();
            if !project_dir.is_dir() {
                continue;
            }
            let project_encoded = project_dir
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("");
            let project_path = decode_project_name(project_encoded);

            if let Ok(session_files) = std::fs::read_dir(&project_dir) {
                for sf in session_files.flatten() {
                    let path = sf.path();
                    if path.extension().and_then(|e| e.to_str()) != Some("jsonl") {
                        continue;
                    }

                    let metadata = sf.metadata().ok();
                    let ctime = metadata
                        .as_ref()
                        .and_then(|m| m.created().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);
                    let mtime = metadata
                        .and_then(|m| m.modified().ok())
                        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                        .map(|d| d.as_secs())
                        .unwrap_or(0);

                    let file_content =
                        std::fs::read_to_string(&path).ok().unwrap_or_default();
                    let first_line = file_content
                        .lines()
                        .next()
                        .map(|l| l.to_string());

                    // Parse cwd from the session's first line — the ground truth for project path
                    let session_cwd = first_line
                        .as_ref()
                        .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                        .and_then(|v| v.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string()))
                        .unwrap_or_else(|| project_path.clone());

                    // Use real cwd for grouping, fall back to decoded dir name
                    let group_key = session_cwd.clone();

                    // Extract first user message as both preview and label (conversation topic)
                    let first_user_msg = file_content
                        .lines()
                        .filter_map(|line| {
                            serde_json::from_str::<serde_json::Value>(line)
                                .ok()
                                .and_then(|v| {
                                    let role = v
                                        .get("message")
                                        .and_then(|m| m.get("role"))
                                        .and_then(|r| r.as_str());
                                    if role == Some("user") {
                                        v.get("message")
                                            .and_then(|m| m.get("content"))
                                            .and_then(|c| {
                                                if let Some(s) = c.as_str() {
                                                    Some(s.to_string())
                                                } else if let Some(arr) = c.as_array() {
                                                    arr.iter()
                                                        .filter(|b| {
                                                            b.get("type")
                                                                .and_then(|t| t.as_str())
                                                                == Some("text")
                                                        })
                                                        .filter_map(|b| {
                                                            b.get("text").and_then(|t| t.as_str())
                                                        })
                                                        .next()
                                                        .map(|s| s.to_string())
                                                } else {
                                                    None
                                                }
                                            })
                                    } else {
                                        None
                                    }
                                })
                        })
                        .next()
                        .unwrap_or_default();

                    // Title: first user message (first line, ~50 chars). Fallback: first 8 chars of UUID.
                    let session_label = if !first_user_msg.is_empty() {
                        first_user_msg
                            .lines()
                            .next()
                            .unwrap_or("")
                            .chars()
                            .take(50)
                            .collect::<String>()
                    } else {
                        first_line
                            .as_ref()
                            .and_then(|l| serde_json::from_str::<serde_json::Value>(l).ok())
                            .and_then(|v| {
                                v.get("id")
                                    .and_then(|i| i.as_str())
                                    .map(|s| s.chars().take(8).collect::<String>())
                            })
                            .unwrap_or_default()
                    };

                    let preview: String = first_user_msg.chars().take(120).collect();

                    all_sessions.push((
                        group_key,
                        SessionInfo {
                            id: path
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("")
                                .to_string(),
                            label: session_label,
                            created_at: format_timestamp(ctime),
                            file_path: path.to_string_lossy().to_string(),
                            updated_at: mtime,
                            preview: preview.chars().take(120).collect(),
                            cwd: session_cwd,
                        },
                    ));
                }
            }
        }
    }

    // Stable sort: by updated_at desc, then created_at desc for deterministic ordering
    all_sessions.sort_by(|a, b| {
        b.1.updated_at
            .cmp(&a.1.updated_at)
            .then_with(|| b.1.created_at.cmp(&a.1.created_at))
    });

    let mut project_map: HashMap<String, Vec<SessionInfo>> = HashMap::new();
    for (proj_path, session) in all_sessions {
        project_map.entry(proj_path).or_default().push(session);
    }

    let mut projects: Vec<ProjectGroup> = project_map
        .into_iter()
        .map(|(path, sessions)| {
            let dir_name = std::path::Path::new(&path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&path)
                .to_string();
            ProjectGroup {
                path,
                dir_name,
                sessions,
            }
        })
        .collect();

    projects.sort_by(|a, b| a.dir_name.cmp(&b.dir_name));

    SessionsResponse { projects }
}

async fn load_session_handler(
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<serde_json::Value>> {
    let file_path = match params.get("path") {
        Some(p) => p,
        None => return Json(vec![]),
    };
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return Json(vec![]),
    };
    let mut messages = Vec::new();
    for line in content.lines() {
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("message") {
                if let Some(msg) = val.get("message") {
                    messages.push(msg.clone());
                }
            }
        }
    }
    Json(messages)
}

async fn delete_session_handler(
    Query(params): Query<HashMap<String, String>>,
) -> Json<serde_json::Value> {
    let file_path = match params.get("path") {
        Some(p) => p,
        None => {
            return Json(serde_json::json!({"success": false, "error": "missing path"}));
        }
    };
    match std::fs::remove_file(file_path) {
        Ok(_) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
    }
}

/// `POST /api/sessions/create` — create a new empty session file.
async fn create_session_handler(
    State(state): State<Arc<BrokerState>>,
    axum::Json(body): axum::Json<HashMap<String, serde_json::Value>>,
) -> Json<serde_json::Value> {
    let cwd = body
        .get("cwd")
        .and_then(|v| v.as_str())
        .unwrap_or(".")
        .to_string();
    let name = body
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("New Session");

    let sessions_dir = get_sessions_dir().join(encode_project_name(&cwd));
    std::fs::create_dir_all(&sessions_dir).ok();

    let id = uuid::Uuid::new_v4().to_string();
    let file_path = sessions_dir.join(format!("{}.jsonl", id));

    let meta = serde_json::json!({
        "type": "session",
        "version": 3,
        "id": id,
        "timestamp": chrono::Utc::now().to_rfc3339(),
        "cwd": cwd,
        "name": name,
    });

    match std::fs::write(&file_path, format!("{}\n", meta)) {
        Ok(_) => {
            // Push updated sessions list immediately to all WS clients
            let sessions = build_sessions_response();
            if let Ok(json) = serde_json::to_string(&sessions.projects) {
                let msg = format!(
                    r#"{{"type":"sessions_list","projects":{}}}"#,
                    json
                );
                let _ = state.event_tx.send(msg);
            }
            // Tell pi to switch to this new session
            let switch_cmd = serde_json::json!({
                "type": "switch_session",
                "sessionFile": file_path.to_string_lossy(),
            });
            let _ = state.command_tx.send(switch_cmd.to_string());
            Json(serde_json::json!({
                "success": true,
                "id": id,
                "file_path": file_path.to_string_lossy(),
            }))
        }
        Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
    }
}

/// `POST /api/sessions/rename` — rename a session.
async fn rename_session_handler(
    axum::Json(body): axum::Json<HashMap<String, serde_json::Value>>,
) -> Json<serde_json::Value> {
    let file_path = match body.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Json(serde_json::json!({"success": false, "error": "missing path"})),
    };
    let new_name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Json(serde_json::json!({"success": false, "error": "missing name"})),
    };

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"success": false, "error": e.to_string()})),
    };

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    if let Some(first) = lines.first_mut() {
        if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(first) {
            val["name"] = serde_json::Value::String(new_name.to_string());
            *first = val.to_string();
        }
    }

    let new_content = lines.join("\n") + "\n";
    match std::fs::write(file_path, new_content) {
        Ok(_) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
    }
}

/// `POST /api/rpc` — forwards command to pi via stdin and waits for correlated response.
/// Pi's RPC mode responds with `{"type":"response","command":"<type>","success":true,"data":{...}}`.
async fn rpc_handler(
    State(state): State<Arc<BrokerState>>,
    axum::Json(body): axum::Json<serde_json::Value>,
) -> Json<serde_json::Value> {
    let req_type = body.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();
    let request_id = uuid::Uuid::new_v4().to_string();
    let mut rx = state.event_tx.subscribe();

    // Embed requestId so we can correlate the response
    let mut cmd = body.clone();
    if let Some(obj) = cmd.as_object_mut() {
        obj.insert("requestId".to_string(), serde_json::Value::String(request_id.clone()));
    }
    let cmd_str = cmd.to_string();

    log::debug!("[broker] rpc send: {}", &cmd_str[..cmd_str.len().min(120)]);
    if state.command_tx.send(cmd_str).is_err() {
        return Json(serde_json::json!({"success": false, "error": "pi not running"}));
    }

    // Wait for a response with matching requestId or command type (timeout 8s)
    let timeout = tokio::time::sleep(std::time::Duration::from_secs(8));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Ok(text) => {
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                            // Pi RPC mode response: {"type":"response","command":"get_available_models",...}
                            if val.get("type").and_then(|v| v.as_str()) == Some("response")
                                && val.get("command").and_then(|v| v.as_str()) == Some(&req_type)
                            {
                                return Json(val);
                            }
                            // Also match by requestId
                            if val.get("requestId").and_then(|v| v.as_str()) == Some(&request_id) {
                                return Json(val);
                            }
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("[broker] rpc lagged {} events", n);
                        continue;
                    }
                    Err(_) => break,
                }
            }
            _ = &mut timeout => {
                log::warn!("[broker] rpc timeout for request {}", request_id);
                break;
            }
        }
    }

    Json(serde_json::json!({"success": false, "error": "timeout"}))
}

/// `GET /api/pi/status` — whether pi is running.
async fn pi_status_handler(
    State(state): State<Arc<BrokerState>>,
) -> Json<PiStatusResponse> {
    let running = state
        .pi_handle
        .lock()
        .as_ref()
        .map(|h| h.running.load(Ordering::SeqCst))
        .unwrap_or(false);
    Json(PiStatusResponse { running })
}

/// `POST /api/pi/restart` — restart pi.
async fn pi_restart_handler(
    State(state): State<Arc<BrokerState>>,
) -> Json<serde_json::Value> {
    if let Some(mut p) = state.pi_handle.lock().take() {
        p.running.store(false, Ordering::SeqCst);
        let _ = p.child.kill();
        let _ = p.child.wait();
    }
    Json(serde_json::json!({"success": true, "note": "pi stopped; full restart pending"}))
}

/// `POST /api/pi/stop` — stop pi.
async fn pi_stop_handler(
    State(state): State<Arc<BrokerState>>,
) -> Json<serde_json::Value> {
    if let Some(mut p) = state.pi_handle.lock().take() {
        p.running.store(false, Ordering::SeqCst);
        let _ = p.child.kill();
        let _ = p.child.wait();
        log::info!("[broker] pi stopped via API");
        Json(serde_json::json!({"success": true}))
    } else {
        Json(serde_json::json!({"success": false, "error": "pi not running"}))
    }
}

fn get_sessions_dir() -> std::path::PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join(".pi")
        .join("agent")
        .join("sessions")
}

/// Decode a pi-encoded project directory name back to a path.
/// pi encodes `E:\Project\RustProject\piter` as `--E--Project-RustProject-piter--`.
/// Rule: `--` prefix/suffix, `:\\` → `--`, `\\` → `-`.
fn decode_project_name(encoded: &str) -> String {
    let trimmed = encoded
        .strip_prefix("--")
        .and_then(|s| s.strip_suffix("--"))
        .unwrap_or(encoded);
    // First "--" separates drive letter from path
    if let Some(pos) = trimmed.find("--") {
        let drive = &trimmed[..pos];
        let rest = &trimmed[pos + 2..];
        format!("{}:\\{}", drive, rest.replace('-', "\\"))
    } else {
        trimmed.replace('-', "\\")
    }
}

/// Encode a filesystem path into pi's project directory name format.
/// `E:\Project\RustProject\piter` → `--E--Project-RustProject-piter--`.
fn encode_project_name(path: &str) -> String {
    let step1 = path.replace(":\\", "--");
    let step2 = step1.replace('\\', "-");
    format!("--{}--", step2)
}

fn format_timestamp(secs: u64) -> String {
    use chrono::DateTime;
    let dt = DateTime::from_timestamp(secs as i64, 0).unwrap_or_default();
    dt.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

/// `GET /api/lan-qr` — returns an SVG QR code for the LAN access URL.
async fn lan_qr_handler(
    State(state): State<Arc<BrokerState>>,
) -> (axum::http::HeaderMap, String) {
    let data = state
        .lan_ips
        .first()
        .map(|ip| {
            format!(
                "http://{}:{}/chat?brokerWs=ws://{}:{}/ws",
                ip, state.http_port, ip, state.http_port
            )
        })
        .unwrap_or_else(|| format!("http://127.0.0.1:{}/chat", state.http_port));

    let svg = generate_qr_svg(&data);
    let mut headers = axum::http::HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "image/svg+xml".parse().unwrap(),
    );
    (headers, svg)
}

/// `GET /api/git-branch` — returns the current git branch.
async fn git_branch_handler() -> Json<GitBranchResponse> {
    let branch = std::process::Command::new("git")
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if b.is_empty() {
                    None
                } else {
                    Some(b)
                }
            } else {
                None
            }
        });
    Json(GitBranchResponse { branch })
}

/// Generate an SVG QR code from a string.
pub fn generate_qr_svg(data: &str) -> String {
    use qrcode::QrCode;
    use qrcode::render::svg as qr_svg;
    match QrCode::new(data) {
        Ok(code) => {
            code.render()
                .min_dimensions(200, 200)
                .dark_color(qr_svg::Color("#000000"))
                .light_color(qr_svg::Color("#ffffff"))
                .build()
        }
        Err(_) => String::new(),
    }
}

// ─── WebSocket Handler ──────────────────────────────────────────────────────

async fn ws_handler(
    ws: WebSocketUpgrade,
    State(state): State<Arc<BrokerState>>,
) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_ws(socket, state))
}

async fn handle_ws(mut socket: WebSocket, state: Arc<BrokerState>) {
    let mut rx = state.event_tx.subscribe();
    log::debug!("[broker] ws client connected");

    // Push current sessions list on connect
    let sessions = build_sessions_response();
    if let Ok(json) = serde_json::to_string(&sessions.projects) {
        let msg = format!(r#"{{"type":"sessions_list","projects":{}}}"#, json);
        if socket.send(Message::Text(msg)).await.is_err() {
            return;
        }
    }

    loop {
        tokio::select! {
            // ── Read from WS client ─────────────────────────────────────
            msg = socket.recv() => {
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(response) = handle_broker_command(&text) {
                            log::debug!("[broker] ws broker_control -> {}", &response[..response.len().min(60)]);
                            if socket.send(Message::Text(response)).await.is_err() {
                                break;
                            }
                            continue;
                        }

                        log::debug!("[broker] ws <cmd> {}", &text[..text.len().min(80)]);
                        // Push updated sessions list immediately for session mutations
                        if let Ok(val) = serde_json::from_str::<serde_json::Value>(&text) {
                            if let Some(t) = val.get("type").and_then(|v| v.as_str()) {
                                if t == "new_session" || t == "switch_session" {
                                    let sessions = build_sessions_response();
                                    if let Ok(json) = serde_json::to_string(&sessions.projects) {
                                        let msg = format!(
                                            r#"{{"type":"sessions_list","projects":{}}}"#,
                                            json
                                        );
                                        let _ = socket.send(Message::Text(msg)).await;
                                    }
                                }
                            }
                        }
                        if state.command_tx.send(text).is_err() {
                            log::warn!("[broker] command channel closed");
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => {
                        log::debug!("[broker] ws client closed connection");
                        break;
                    }
                    Some(Ok(_)) => {}
                    Some(Err(e)) => {
                        log::debug!("[broker] ws recv error: {}", e);
                        break;
                    }
                }
            }
            // ── Read from pi stdout → broadcast to WS client ──────────
            result = rx.recv() => {
                match result {
                    Ok(event) => {
                        if socket.send(Message::Text(event)).await.is_err() {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                    Err(broadcast::error::RecvError::Lagged(n)) => {
                        log::warn!("[broker] ws lagged {} events, skipping", n);
                        continue;
                    }
                }
            }
        }
    }

    log::debug!("[broker] ws client disconnected");
}

/// Intercept broker_control commands and handle them in Rust.
fn handle_broker_command(text: &str) -> Option<String> {
    let value: serde_json::Value = serde_json::from_str(text).ok()?;
    let cmd_type = value.get("type")?.as_str()?;

    if cmd_type != "broker_control" && cmd_type != "broker_info" {
        return None;
    }

    let action = value
        .get("action")
        .or_else(|| value.get("command"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let response = match action {
        "ping" => serde_json::json!({
            "type": "broker_control",
            "action": "pong",
            "success": true,
            "version": env!("CARGO_PKG_VERSION"),
        }),

        "info" => serde_json::json!({
            "type": "broker_control",
            "action": "info",
            "success": true,
            "version": env!("CARGO_PKG_VERSION"),
            "features": ["rpc", "ws", "lan", "health"],
            "runtimes": ["rust", "pi-rpc"],
        }),

        _ => serde_json::json!({
            "type": "broker_control",
            "action": action,
            "success": false,
            "error": format!("Unknown broker_control action: {}", action),
        }),
    };

    Some(response.to_string())
}
