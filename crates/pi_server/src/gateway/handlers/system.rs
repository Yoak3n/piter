//! System handlers: health, LAN info, QR code, git branch.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::Json;

use super::{GitBranchResponse, HealthResponse, LanInfoResponse};
use crate::gateway::GatewayState;

// ─── Shared logic (callable from WS) ───────────────────────────────────────

pub fn get_health(state: &GatewayState) -> HealthResponse {
    let lan_urls: Vec<String> = state
        .current_lan_ips()
        .iter()
        .map(|ip| format!("http://{}:{}/chat", ip, state.http_port))
        .collect();

    HealthResponse {
        status: "ok".into(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        pi_version: state.pi_version.clone(),
        lan_urls,
        broker_url: format!("ws://127.0.0.1:{}/ws", state.http_port),
        uptime_secs: state.start_time.elapsed().as_secs(),
    }
}

pub fn get_lan_info(state: &GatewayState) -> LanInfoResponse {
    let lan_urls: Vec<String> = state
        .current_lan_ips()
        .iter()
        .map(|ip| format!("http://{}:{}/chat", ip, state.http_port))
        .collect();

    let qr_data = lan_urls
        .first()
        .cloned()
        .unwrap_or_else(|| format!("http://127.0.0.1:{}/chat", state.http_port));

    LanInfoResponse {
        broker_ws_url: format!("ws://127.0.0.1:{}/ws", state.http_port),
        http_url: format!("http://127.0.0.1:{}/", state.http_port),
        lan_urls,
        qr_data,
    }
}

pub fn get_git_branch() -> GitBranchResponse {
    // git is a console app; without CREATE_NO_WINDOW a console window
    // flashes when spawned from the GUI-subsystem piter.exe.
    #[cfg(target_os = "windows")]
    let mut cmd = {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        let mut c = std::process::Command::new("git");
        c.creation_flags(CREATE_NO_WINDOW);
        c
    };
    #[cfg(not(target_os = "windows"))]
    let mut cmd = std::process::Command::new("git");

    let branch = cmd
        .args(["rev-parse", "--abbrev-ref", "HEAD"])
        .output()
        .ok()
        .and_then(|out| {
            if out.status.success() {
                let b = String::from_utf8_lossy(&out.stdout).trim().to_string();
                if b.is_empty() { None } else { Some(b) }
            } else {
                None
            }
        });
    GitBranchResponse { branch }
}

// ─── REST handlers ──────────────────────────────────────────────────────────

pub async fn health_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<HealthResponse> {
    Json(get_health(&state))
}

pub async fn lan_info_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<LanInfoResponse> {
    Json(get_lan_info(&state))
}

pub async fn lan_qr_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    Query(params): Query<HashMap<String, String>>,
) -> (HeaderMap, String) {
    let ips = state.current_lan_ips();
    // 分享页 work 卡片需要 work URL 的二维码；path 白名单（仅 /work），防任意路径。
    let path = match params.get("path").map(String::as_str) {
        Some("/work") => "/work",
        _ => "/chat",
    };
    let data = ips
        .first()
        .map(|ip| {
            format!(
                "http://{}:{}{}?brokerWs=ws://{}:{}/chat-ws&mobile=1",
                ip, state.http_port, path, ip, state.http_port
            )
        })
        .unwrap_or_else(|| format!("http://127.0.0.1:{}{}", state.http_port, path));

    let svg = super::session::generate_qr_svg(&data);
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "image/svg+xml".parse().unwrap(),
    );
    (headers, svg)
}

pub async fn git_branch_handler() -> Json<GitBranchResponse> {
    Json(get_git_branch())
}

/// `GET /api/connections` → 当前 WS 客户端连接列表（分享页「连接客户端」数据源）。
pub async fn connections_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<serde_json::Value> {
    let conns: Vec<serde_json::Value> = state
        .connections
        .lock()
        .values()
        .map(|c| serde_json::to_value(c).unwrap_or_default())
        .collect();
    Json(serde_json::json!({ "connections": conns }))
}
