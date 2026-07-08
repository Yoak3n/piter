//! REST API handlers for the broker's HTTP server.

use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::Query;
use axum::http::HeaderMap;
use axum::response::Json;
use serde_json::Value;
use uuid::Uuid;

use super::types::{
    BrokerState, GitBranchResponse, HealthResponse, LanInfoResponse, PiStatusResponse,
    ProjectGroup, SessionInfo, SessionsResponse,
};
use super::util::{
    decode_project_name, encode_project_name, format_timestamp, generate_qr_svg,
    get_sessions_dir,
};

// ─── Health & LAN ──────────────────────────────────────────────────────────

pub async fn health_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
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

pub async fn lan_info_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
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

/// `GET /api/lan-qr` — returns an SVG QR code for the LAN access URL.
pub async fn lan_qr_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
) -> (HeaderMap, String) {
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
    let mut headers = HeaderMap::new();
    headers.insert(
        axum::http::header::CONTENT_TYPE,
        "image/svg+xml".parse().unwrap(),
    );
    (headers, svg)
}

// ─── Git ───────────────────────────────────────────────────────────────────

/// `GET /api/git-branch` — returns the current git branch.
pub async fn git_branch_handler() -> Json<GitBranchResponse> {
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

// ─── Sessions ──────────────────────────────────────────────────────────────

pub async fn sessions_handler() -> Json<SessionsResponse> {
    Json(build_sessions_response())
}

pub fn build_sessions_response() -> SessionsResponse {
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
                        .and_then(|l| serde_json::from_str::<Value>(l).ok())
                        .and_then(|v| v.get("cwd").and_then(|c| c.as_str()).map(|s| s.to_string()))
                        .unwrap_or_else(|| project_path.clone());

                    // Use real cwd for grouping, fall back to decoded dir name
                    let group_key = session_cwd.clone();

                    // Extract first user message as both preview and label (conversation topic)
                    let first_user_msg = file_content
                        .lines()
                        .filter_map(|line| {
                            serde_json::from_str::<Value>(line)
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
                            .and_then(|l| serde_json::from_str::<Value>(l).ok())
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
                            preview,
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
            let dir_name = Path::new(&path)
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

pub async fn load_session_handler(
    Query(params): Query<HashMap<String, String>>,
) -> Json<Vec<Value>> {
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
        if let Ok(val) = serde_json::from_str::<Value>(line) {
            if val.get("type").and_then(|t| t.as_str()) == Some("message") {
                if let Some(msg) = val.get("message") {
                    messages.push(msg.clone());
                }
            }
        }
    }
    Json(messages)
}

pub async fn delete_session_handler(
    Query(params): Query<HashMap<String, String>>,
) -> Json<Value> {
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
pub async fn create_session_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
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

    let id = Uuid::new_v4().to_string();
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
pub async fn rename_session_handler(
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let file_path = match body.get("path").and_then(|v| v.as_str()) {
        Some(p) => p,
        None => return Json(serde_json::json!({"success": false, "error": "missing path"}))
    };
    let new_name = match body.get("name").and_then(|v| v.as_str()) {
        Some(n) => n,
        None => return Json(serde_json::json!({"success": false, "error": "missing name"}))
    };

    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return Json(serde_json::json!({"success": false, "error": e.to_string()})),
    };

    let mut lines: Vec<String> = content.lines().map(|l| l.to_string()).collect();
    if let Some(first) = lines.first_mut() {
        if let Ok(mut val) = serde_json::from_str::<Value>(first) {
            val["name"] = Value::String(new_name.to_string());
            *first = val.to_string();
        }
    }

    let new_content = lines.join("\n") + "\n";
    match std::fs::write(file_path, new_content) {
        Ok(_) => Json(serde_json::json!({"success": true})),
        Err(e) => Json(serde_json::json!({"success": false, "error": e.to_string()})),
    }
}

// ─── Pi Control ────────────────────────────────────────────────────────────

/// `POST /api/rpc` — forwards command to pi via stdin and waits for correlated response.
pub async fn rpc_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
    axum::Json(body): axum::Json<Value>,
) -> Json<Value> {
    let command = body.get("type").and_then(|v| v.as_str()).unwrap_or("").to_string();

    let port = body.get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .or_else(|| *state.inner.active_port.lock());

    let Some(port) = port else {
        return Json(serde_json::json!({"success": false, "error": "no active port"}));
    };

    // Create oneshot channel for the response, keyed by command type
    let (tx, rx) = tokio::sync::oneshot::channel();
    state.inner.pending_rpc.lock().insert(command.clone(), tx);

    // Send to the target port's pi process
    {
        let processes = state.inner.pi_processes.lock();
        let Some(process) = processes.get(&port) else {
            state.inner.pending_rpc.lock().remove(&command);
            return Json(serde_json::json!({"success": false, "error": "pi not running on port"}));
        };
        if let Some(tx) = &process.stdin_tx {
            let cmd_str = body.to_string();
            if tx.send(cmd_str).is_err() {
                state.inner.pending_rpc.lock().remove(&command);
                return Json(serde_json::json!({"success": false, "error": "failed to send command"}));
            }
        }
    }

    // Wait for pi response with timeout
    match tokio::time::timeout(std::time::Duration::from_secs(30), rx).await {
        Ok(Ok(response)) => Json(response),
        Ok(Err(_)) => {
            Json(serde_json::json!({"success": false, "error": "response channel closed"}))
        }
        Err(_) => {
            state.inner.pending_rpc.lock().remove(&command);
            Json(serde_json::json!({"success": false, "error": "timeout waiting for pi response"}))
        }
    }
}

/// `GET /api/pi/status` — whether pi is running.
pub async fn pi_status_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
) -> Json<PiStatusResponse> {
    let active_port = *state.inner.active_port.lock();
    let running = if let Some(port) = active_port {
        state.inner.pi_processes.lock().contains_key(&port)
    } else {
        false
    };
    Json(PiStatusResponse { running, port: active_port })
}

/// `POST /api/pi/restart` — restart pi.
pub async fn pi_restart_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let port = body.get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .or_else(|| *state.inner.active_port.lock());

    if let Some(port) = port {
        // Kill the existing process
        let mut processes = state.inner.pi_processes.lock();
        if let Some(mut process) = processes.remove(&port) {
            process.running.store(false, Ordering::SeqCst);
            let _ = process.child.kill();
            let _ = process.child.wait();
        }
        drop(processes);

        Json(serde_json::json!({"success": true, "note": "pi stopped; restart logic pending"}))
    } else {
        Json(serde_json::json!({"success": false, "error": "no port specified"}))
    }
}

/// `POST /api/pi/stop` — stop pi.
pub async fn pi_stop_handler(
    axum::extract::State(state): axum::extract::State<Arc<BrokerState>>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Json<Value> {
    let port = body.get("port")
        .and_then(|v| v.as_u64())
        .and_then(|p| u16::try_from(p).ok())
        .or_else(|| *state.inner.active_port.lock());

    if let Some(port) = port {
        let mut processes = state.inner.pi_processes.lock();
        if let Some(mut process) = processes.remove(&port) {
            process.running.store(false, Ordering::SeqCst);
            let _ = process.child.kill();
            let _ = process.child.wait();
            log::info!("[broker] pi stopped on port {} via API", port);
            Json(serde_json::json!({"success": true}))
        } else {
            Json(serde_json::json!({"success": false, "error": "pi not running on port"}))
        }
    } else {
        Json(serde_json::json!({"success": false, "error": "no port specified"}))
    }
}
