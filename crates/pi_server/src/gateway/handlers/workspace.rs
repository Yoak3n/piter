//! Workspace REST handlers（0.3.0）：CRUD + files + upload(multipart) +
//! mark-deliverable + artifacts + deliverables + download + zip + mode。
//!
//! 契约对齐 work/docs/mock-contract.md §2：成功响应直接返回领域结构（不套
//! `success` 包装）；错误返回 `{"error": "<code>", "message": "<human>"}`
//! 并带非 2xx 状态码（Flutter 端按 `data['error']` 解析）。

use std::sync::Arc;

use axum::extract::{Multipart, Path as AxumPath, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};

use crate::gateway::db::ArtifactRow;
use crate::gateway::workspace;
use crate::gateway::GatewayState;

// ─── Shared view shapes ─────────────────────────────────────────────────────

/// Artifact JSON view（ArtifactRow → 契约 camelCase 形态，createdAt 为 epoch ms）。
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ArtifactView {
    id: String,
    workspace_id: String,
    session_id: String,
    turn_id: i64,
    path: String,
    op: String,
    size: i64,
    lines_added: i64,
    lines_deleted: i64,
    source: String,
    deliverable: bool,
    created_at: i64,
}

fn artifact_view(r: &ArtifactRow) -> ArtifactView {
    ArtifactView {
        id: r.id.clone(),
        workspace_id: r.workspace_id.clone(),
        session_id: r.session_id.clone(),
        turn_id: r.turn_id,
        path: r.rel_path.clone(),
        op: r.op.clone(),
        size: r.size,
        lines_added: r.lines_added,
        lines_deleted: r.lines_deleted,
        source: r.source.clone(),
        deliverable: r.deliverable,
        created_at: rfc3339_to_ms(&r.created_at),
    }
}

fn rfc3339_to_ms(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

/// 错误响应：`{"error": code, "message": human}` + 状态码。
fn api_err(status: StatusCode, code: &str, message: impl Into<String>) -> Response {
    (status, Json(json!({ "error": code, "message": message.into() }))).into_response()
}

fn get_workspace_dir(state: &GatewayState, id: &str) -> Result<std::path::PathBuf, Response> {
    workspace::workspace_dir_from_id(&state.db, id).map_err(|e| {
        let (code, status) = if e.starts_with("workspace not found") {
            ("workspace_not_found", StatusCode::NOT_FOUND)
        } else {
            ("not_a_workspace", StatusCode::BAD_REQUEST)
        };
        api_err(status, code, e)
    })
}

// ─── CRUD ──────────────────────────────────────────────────────────────────

pub async fn list_workspaces_handler(State(state): State<Arc<GatewayState>>) -> Json<Value> {
    let base = state.workspace_base_dir();
    let ws = workspace::list_workspaces(&state.db, &base);
    Json(json!({ "workspaces": ws }))
}

pub async fn create_workspace_handler(
    State(state): State<Arc<GatewayState>>,
    Json(body): Json<Value>,
) -> Response {
    let name = body
        .get("name")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let Some(name) = name else {
        return api_err(StatusCode::BAD_REQUEST, "missing_name", "缺少 name");
    };
    let base = state.workspace_base_dir();
    match workspace::create_workspace(&state.db, &base, name) {
        Ok(ws) => Json(json!({ "workspace": ws })).into_response(),
        Err(e) => api_err(StatusCode::INTERNAL_SERVER_ERROR, "workspace_create_failed", e),
    }
}

pub async fn get_workspace_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let base = state.workspace_base_dir();
    match workspace::get_workspace(&state.db, &base, &id) {
        Some(ws) => Json(json!({ "workspace": ws })).into_response(),
        None => api_err(StatusCode::NOT_FOUND, "workspace_not_found", format!("工作空间 {} 不存在", id)),
    }
}

pub async fn delete_workspace_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let base = state.workspace_base_dir();
    // 连带删除工作空间拥有的会话：kill 运行中 pi 进程 + 移除内存会话 +
    // 删 DB 行（含搜索索引/checkpoints）+ 删会话 jsonl 文件。
    // delete_session 内部会广播 sessions_list；多次调用仅多次刷新，幂等。
    for iid in state.db.get_project_sessions(&id) {
        let _ = super::session::delete_session(&iid, &state);
    }
    match workspace::delete_workspace(&state.db, &base, &id) {
        Ok(()) => Json(json!({ "success": true })).into_response(),
        Err(e) => api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "workspace_delete_failed",
            e,
        ),
    }
}

pub async fn set_workspace_mode_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let mode = body
        .get("mode")
        .and_then(Value::as_str)
        .filter(|m| matches!(*m, "ask" | "allow" | "deny"));
    let Some(mode) = mode else {
        return api_err(StatusCode::BAD_REQUEST, "invalid_mode", "mode 必须是 ask | allow | deny");
    };
    let base = state.workspace_base_dir();
    match workspace::set_workspace_mode(&state.db, &base, &id, mode) {
        Ok(ws) => Json(json!({ "workspace": ws })).into_response(),
        Err(e) => api_err(StatusCode::INTERNAL_SERVER_ERROR, "mode_update_failed", e),
    }
}

// ─── 基目录配置与迁移（0.3.0 文档定案：默认安装目录 + Admin 可配置）──────────

/// `GET /api/workspaces/base-dir` → 生效基目录/配置值/可写性/迁移队列/各工作空间状态。
pub async fn get_base_dir_handler(State(state): State<Arc<GatewayState>>) -> Json<Value> {
    let base = state.workspace_base_dir();
    let configured = state.db.get_workspace_base_dir();
    let writable = workspace::dir_writable(&base);
    let mig = state.migrations.lock();
    let pending: Vec<Value> = mig
        .pending
        .iter()
        .map(|p| {
            json!({
                "id": p.id,
                "oldPath": p.old_path,
                "newPath": p.new_path,
                "waiting": p.waiting,
            })
        })
        .collect();
    let errors: Vec<Value> = mig
        .errors
        .iter()
        .map(|(id, e)| json!({ "id": id, "error": e }))
        .collect();
    let migrating = mig.migrating;
    drop(mig);

    let ws_list: Vec<Value> = workspace::list_workspaces(&state.db, &base)
        .into_iter()
        .map(|w| {
            let active = {
                let mgr = state.session_manager.lock();
                mgr.sessions.values().any(|s| {
                    s.cwd == w.cwd
                        && s.activity
                            != crate::gateway::session_manager::SessionActivity::Unloaded
                })
            };
            json!({ "id": w.id, "name": w.name, "cwd": w.cwd, "active": active })
        })
        .collect();

    Json(json!({
        "baseDir": base,
        "configured": configured,
        "defaultBaseDir": state.static_dir,
        "writable": writable,
        "migration": { "migrating": migrating, "pending": pending, "errors": errors },
        "workspaces": ws_list,
    }))
}

/// `PUT /api/workspaces/base-dir` body `{"baseDir": str}`（空串 = 清除配置回默认）。
/// 校验可写 → 持久化配置 → 更新生效基目录 → 构建迁移队列（现有工作空间迁到新目录）。
pub async fn set_base_dir_handler(
    State(state): State<Arc<GatewayState>>,
    Json(body): Json<Value>,
) -> Response {
    let configured = body
        .get("baseDir")
        .and_then(Value::as_str)
        .map(|s| s.trim().to_string())
        .unwrap_or_default();

    // 目标可写校验（空 = 默认安装目录，同样校验）。
    let new_base = if configured.is_empty() {
        state.static_dir.clone()
    } else {
        std::path::PathBuf::from(&configured)
    };
    if !workspace::dir_writable(&new_base) {
        return api_err(
            StatusCode::BAD_REQUEST,
            "not_writable",
            format!("基目录不可写：{}", new_base.display()),
        );
    }

    // 迁移进行中 / 队列未清空 → 拒绝（互斥，防并行改基目录）。
    {
        let mig = state.migrations.lock();
        if !mig.pending.is_empty() || mig.migrating {
            return api_err(
                StatusCode::CONFLICT,
                "migration_in_progress",
                "有迁移进行中，请等待完成后再修改基目录",
            );
        }
    }

    if let Err(e) = state.db.set_workspace_base_dir(&configured) {
        return api_err(StatusCode::INTERNAL_SERVER_ERROR, "save_failed", e);
    }
    *state.workspace_base_dir.lock() = new_base.clone();

    // 构建迁移队列：现有工作空间 cwd 不在新基目录下的都迁到新位置。
    let mut pending = Vec::new();
    for proj in state.db.list_projects(true) {
        if proj.project_type != "workspace" {
            continue;
        }
        let new_path = workspace::workspace_dir(&new_base, &proj.id);
        if std::path::PathBuf::from(&proj.cwd) != new_path {
            pending.push(crate::gateway::migrate::PendingMigration {
                id: proj.id,
                old_path: proj.cwd,
                new_path: new_path.to_string_lossy().to_string(),
                waiting: false,
            });
        }
    }
    {
        let mut mig = state.migrations.lock();
        mig.pending = pending.clone();
        mig.errors.clear();
    }
    crate::gateway::migrate::save_queue(&state);
    // 立即尝试推进一次（不活跃的工作空间直接迁移；活跃的进入等待）。
    crate::gateway::migrate::try_run_migrations(&state);

    Json(json!({
        "baseDir": new_base,
        "configured": configured,
        "migrationPending": pending.len(),
    }))
    .into_response()
}

// ─── Files ─────────────────────────────────────────────────────────────────

pub async fn files_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    let dir = match get_workspace_dir(&state, &id) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let ws_id = id.clone();
    let files = workspace::scan_files(&dir, |path| {
        state.db.is_deliverable_marked(&ws_id, path)
    });
    let base_path = dir.to_string_lossy().replace('\\', "/");
    let base_path = if base_path.ends_with('/') { base_path } else { format!("{}/", base_path) };
    Json(json!({ "files": files, "basePath": base_path })).into_response()
}

/// 上传校验与保存：multipart `files` 字段（单/批量）；单文件 ≤50MB；
/// 拒绝 `output/` 路径与 `..`/绝对路径穿越。
pub async fn upload_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
    mut multipart: Multipart,
) -> Response {
    let dir = match get_workspace_dir(&state, &id) {
        Ok(d) => d,
        Err(r) => return r,
    };
    const MAX_UPLOAD_BYTES: u64 = 50 * 1024 * 1024;

    let mut uploaded: Vec<String> = Vec::new();
    let mut rejected: Vec<Value> = Vec::new();
    let mut failed = false;

    loop {
        let part = match multipart.next_field().await {
            Ok(Some(p)) => p,
            _ => break, // None / Err → 结束
        };
        // 只处理带文件名的部分；其余字段跳过。
        let Some(name) = part.file_name().map(|s| s.to_string()) else {
            continue;
        };
        let rel = match workspace::clean_upload_path(&name) {
            Ok(r) => r,
            Err(e) => {
                rejected.push(json!({ "path": name, "reason": e }));
                continue;
            }
        };
        let bytes = match part.bytes().await {
            Ok(b) => b,
            Err(_) => {
                rejected.push(json!({ "path": name, "reason": "read_failed" }));
                continue;
            }
        };
        if bytes.len() as u64 > MAX_UPLOAD_BYTES {
            rejected.push(json!({ "path": name, "reason": "too_large" }));
            continue;
        }
        let target = dir.join(&rel);
        if let Some(parent) = target.parent() {
            if std::fs::create_dir_all(parent).is_err() {
                rejected.push(json!({ "path": name, "reason": "write_failed" }));
                continue;
            }
        }
        if std::fs::write(&target, &bytes).is_err() {
            rejected.push(json!({ "path": name, "reason": "write_failed" }));
            failed = true;
            continue;
        }
        uploaded.push(rel);
    }

    if let Err(e) = workspace::note_uploaded_files(&state.db, &id, &uploaded) {
        log::warn!("[workspace] note_uploaded_files failed: {}", e);
    }
    if failed {
        return api_err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "upload_partial_failed",
            "部分文件写入失败",
        );
    }
    Json(json!({ "uploaded": uploaded, "rejected": rejected })).into_response()
}

pub async fn mark_deliverable_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let path = body.get("path").and_then(Value::as_str);
    let Some(path) = path else {
        return api_err(StatusCode::BAD_REQUEST, "missing_path", "缺少 path");
    };
    let deliverable = body
        .get("deliverable")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    let rel = match workspace::clean_rel_path(path) {
        Ok(r) => r,
        Err(e) => return api_err(StatusCode::BAD_REQUEST, "invalid_path", e),
    };
    let dir = match get_workspace_dir(&state, &id) {
        Ok(d) => d,
        Err(r) => return r,
    };
    // 标记对象必须是磁盘上存在的文件（目录/不存在 → 400）。
    let target = dir.join(&rel);
    let md = match std::fs::metadata(&target) {
        Ok(m) if m.is_file() => m,
        Ok(_) => {
            return api_err(StatusCode::BAD_REQUEST, "not_a_file", "只能标记文件为交付物");
        }
        Err(_) => return api_err(StatusCode::NOT_FOUND, "file_not_found", format!("文件 {} 不存在", rel)),
    };

    if let Err(e) = state.db.set_deliverable_mark(&id, &rel, deliverable) {
        return api_err(StatusCode::INTERNAL_SERVER_ERROR, "mark_failed", e);
    }
    let entry = workspace::FileEntry {
        path: rel.clone(),
        kind: "file".into(),
        size: md.len() as i64,
        mtime: md
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0),
        is_deliverable: deliverable,
    };
    Json(json!({ "success": true, "entry": entry })).into_response()
}

// ─── Artifacts / deliverables ───────────────────────────────────────────────

/// `GET /api/workspaces/:id/artifacts?sinceTurn=` → 按 turn 分组，新→旧。
pub async fn artifacts_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
    Query(params): Query<serde_json::Map<String, Value>>,
) -> Response {
    let since_turn = params
        .get("sinceTurn")
        .and_then(Value::as_i64)
        .or_else(|| params.get("sinceTurn").and_then(Value::as_u64).map(|v| v as i64));
    let rows = match state.db.list_artifacts(&id, since_turn) {
        Ok(r) => r,
        Err(e) => return api_err(StatusCode::INTERNAL_SERVER_ERROR, "query_failed", e),
    };
    // 按 turn 分组（list 已按 turn_id DESC 排序，组内 created_at ASC）。
    let mut turns: Vec<Value> = Vec::new();
    let mut current: Option<(i64, Vec<ArtifactView>, i64)> = None;
    for row in &rows {
        let view = artifact_view(row);
        match &mut current {
            Some((turn, items, _)) if *turn == view.turn_id => items.push(view),
            _ => {
                if let Some((turn, items, created)) = current.take() {
                    turns.push(json!({ "turnId": turn, "createdAt": created, "items": items }));
                }
                current = Some((view.turn_id, vec![view], rfc3339_to_ms(&row.created_at)));
            }
        }
    }
    if let Some((turn, items, created)) = current {
        turns.push(json!({ "turnId": turn, "createdAt": created, "items": items }));
    }
    Json(json!({ "turns": turns })).into_response()
}

/// `GET /api/workspaces/:id/deliverables` → 仅交付物（output/ ∪ 手动标记）。
pub async fn deliverables_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
) -> Response {
    match state.db.list_deliverable_artifacts(&id) {
        Ok(rows) => {
            let items: Vec<ArtifactView> = rows.iter().map(artifact_view).collect();
            Json(json!({ "items": items })).into_response()
        }
        Err(e) => api_err(StatusCode::INTERNAL_SERVER_ERROR, "query_failed", e),
    }
}

// ─── Download / zip ─────────────────────────────────────────────────────────

/// `GET /api/workspaces/:id/download?path=<rel>` → 单文件流式，路径锚定 real_dir。
pub async fn download_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
    Query(params): Query<std::collections::HashMap<String, String>>,
) -> Response {
    let dir = match get_workspace_dir(&state, &id) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let rel = match params.get("path") {
        Some(p) => p,
        None => return api_err(StatusCode::BAD_REQUEST, "missing_path", "缺少 path"),
    };
    let abs = match workspace::anchor_path(&dir, rel) {
        Ok(p) => p,
        Err(e) if e.starts_with("file not found") => {
            return api_err(StatusCode::NOT_FOUND, "file_not_found", e);
        }
        Err(e) => return api_err(StatusCode::BAD_REQUEST, "invalid_path", e),
    };
    let bytes = match tokio::fs::read(&abs).await {
        Ok(b) => b,
        Err(_) => return api_err(StatusCode::NOT_FOUND, "file_not_found", "读取文件失败"),
    };
    let fname = abs
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "download".into());
    let headers = [
        ("content-type", "application/octet-stream"),
        (
            "content-disposition",
            &format!("attachment; filename=\"{}\"", fname.replace('"', "")),
        ),
    ];
    (StatusCode::OK, headers, bytes).into_response()
}

/// `POST /api/workspaces/:id/zip` body `{"paths":[...]}` 或 `{"all":true}`
/// → 直接流式返回 `application/zip`（契约开放项 #1：推荐直接流式）。
pub async fn zip_handler(
    State(state): State<Arc<GatewayState>>,
    AxumPath(id): AxumPath<String>,
    Json(body): Json<Value>,
) -> Response {
    let dir = match get_workspace_dir(&state, &id) {
        Ok(d) => d,
        Err(r) => return r,
    };
    let all = body.get("all").and_then(Value::as_bool).unwrap_or(false);
    let paths: Vec<String> = body
        .get("paths")
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(Value::as_str)
                .map(|s| s.to_string())
                .collect()
        })
        .unwrap_or_default();
    if !all && paths.is_empty() {
        return api_err(StatusCode::BAD_REQUEST, "empty_selection", "paths 为空且未指定 all=true");
    }
    let bytes = match workspace::zip_files(&dir, &paths, all) {
        Ok(b) => b,
        Err(e) => return api_err(StatusCode::BAD_REQUEST, "zip_failed", e),
    };
    let name = state
        .db
        .get_project(&id)
        .map(|p| p.name)
        .unwrap_or_else(|| "workspace".into());
    let headers = [
        ("content-type", "application/zip"),
        (
            "content-disposition",
            &format!("attachment; filename=\"{}.zip\"", name.replace('"', "")),
        ),
    ];
    (StatusCode::OK, headers, bytes).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use axum::routing::{get, post};
    use axum::Router;
    use tower::ServiceExt;

    fn test_router(state: Arc<GatewayState>) -> Router {
        Router::new()
            .route("/api/workspaces", get(list_workspaces_handler).post(create_workspace_handler))
            .route(
                "/api/workspaces/:id",
                get(get_workspace_handler).delete(delete_workspace_handler),
            )
            .route("/api/workspaces/:id/files", get(files_handler))
            .route("/api/workspaces/:id/upload", post(upload_handler))
            .route(
                "/api/workspaces/:id/mark-deliverable",
                post(mark_deliverable_handler),
            )
            .route("/api/workspaces/:id/artifacts", get(artifacts_handler))
            .route("/api/workspaces/:id/deliverables", get(deliverables_handler))
            .route("/api/workspaces/:id/download", get(download_handler))
            .route("/api/workspaces/:id/zip", post(zip_handler))
            .with_state(state)
    }

    fn test_state(db: Arc<crate::gateway::db::Db>, data_dir: std::path::PathBuf) -> Arc<GatewayState> {
        use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP};
        let (event_tx, _) = tokio::sync::broadcast::channel(EVENT_CHANNEL_CAP);
        Arc::new(GatewayState {
            event_tx,
            inner: Arc::new(BrokerInner::default()),
            lan_ips: Arc::new(parking_lot::Mutex::new((
                std::time::Instant::now(),
                Vec::new(),
            ))),
            http_port: 0,
            pi_version: String::new(),
            pi_exe: std::path::PathBuf::new(),
            static_dir: std::path::PathBuf::new(),
            start_time: std::time::Instant::now(),
            db,
            data_dir: data_dir.clone(),
            chat_dist: std::path::PathBuf::new(),
            work_dist: None,
            connections: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            extension_cache: Arc::new(parking_lot::RwLock::new(std::collections::HashMap::new())),
            ui_clients: Arc::new(parking_lot::Mutex::new(std::collections::HashMap::new())),
            session_manager: Arc::new(parking_lot::Mutex::new(
                crate::gateway::session_manager::SessionManager::new(None),
            )),
            agent_end_hook: Arc::new(parking_lot::Mutex::new(None)),
            mdns: Arc::new(parking_lot::Mutex::new(None)),
            workspace_base_dir: Arc::new(parking_lot::Mutex::new(data_dir.clone())),
            migrations: Arc::new(parking_lot::Mutex::new(crate::gateway::migrate::MigrationState::default())),
        })
    }

    async fn body_json(resp: Response) -> Value {
        let bytes = axum::body::to_bytes(resp.into_body(), 10 * 1024 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn json_request(method: &str, uri: &str, body: Value) -> Request<Body> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/json")
            .body(Body::from(body.to_string()))
            .unwrap()
    }

    #[tokio::test]
    async fn workspace_rest_flow() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::gateway::db::Db::open(tmp.path()).unwrap();
        let state = test_state(db.clone(), tmp.path().to_path_buf());
        let router = test_router(state);

        // 1. 创建
        let resp = router
            .clone()
            .oneshot(json_request("POST", "/api/workspaces", json!({"name": "Demo"})))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let v = body_json(resp).await;
        let ws = v["workspace"].clone();
        let id = ws["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("ws_"));
        assert_eq!(ws["mode"], "ask");
        assert_eq!(ws["fileCount"], 0);
        let real_dir = ws["cwd"].as_str().unwrap().to_string();
        // 约束文件与扩展注册
        assert!(std::path::Path::new(&real_dir).join(".pi/extensions/constraint.ts").exists());

        // 2. 列表
        let resp = router.clone().oneshot(Request::builder().uri("/api/workspaces").body(Body::empty()).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["workspaces"].as_array().unwrap().len(), 1);

        // 3. 上传（multipart 单文件）
        let boundary = "test-boundary";
        let body = format!(
            "--{b}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"a.txt\"\r\nContent-Type: text/plain\r\n\r\nhello upload\r\n--{b}--\r\n",
            b = boundary
        );
        let resp = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/workspaces/{}/upload", id))
                    .header("content-type", format!("multipart/form-data; boundary={}", boundary))
                    .body(Body::from(body))
                    .unwrap(),
            )
            .await
            .unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["uploaded"], json!(["a.txt"]));
        assert_eq!(v["rejected"].as_array().unwrap().len(), 0);

        // 4. 文件列表：a.txt 在列、basePath 带尾斜杠
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}/files", id)).body(Body::empty()).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        let files = v["files"].as_array().unwrap();
        assert!(files.iter().any(|f| f["path"] == "a.txt"));
        assert!(v["basePath"].as_str().unwrap().ends_with('/'));

        // 5. 标记交付物
        let resp = router.clone().oneshot(json_request("POST", &format!("/api/workspaces/{}/mark-deliverable", id), json!({"path": "a.txt", "deliverable": true}))).await.unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["entry"]["isDeliverable"], true);
        assert_eq!(v["entry"]["type"], "file");
        // 再次拉文件列表应反映标记
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}/files", id)).body(Body::empty()).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        let a = v["files"].as_array().unwrap().iter().find(|f| f["path"] == "a.txt").unwrap();
        assert_eq!(a["isDeliverable"], true);

        // 6. 下载 + 穿越拒绝
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}/download?path=a.txt", id)).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(&bytes[..], b"hello upload");
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}/download?path=../outside.txt", id)).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);

        // 7. zip（all=true）
        let resp = router.clone().oneshot(json_request("POST", &format!("/api/workspaces/{}/zip", id), json!({"all": true}))).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(
            resp.headers().get("content-type").unwrap(),
            "application/zip"
        );
        let bytes = axum::body::to_bytes(resp.into_body(), 1024 * 1024).await.unwrap();
        let archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();
        assert!(archive.file_names().any(|n| n == "a.txt"));

        // 8. 快照 diff 走一遍 → artifacts 分组（new→旧）。
        // 上传的 a.txt 属于用户内容，进入基线不算产物；只有之后的真实改动才报。
        crate::gateway::workspace::ensure_session_baseline(&db, &id, "s1").unwrap();
        std::fs::create_dir_all(std::path::Path::new(&real_dir).join("src")).unwrap();
        std::fs::write(
            std::path::Path::new(&real_dir).join("src/new.txt"),
            "line1\nline2\n",
        )
        .unwrap();
        crate::gateway::workspace::capture_turn_artifacts(&db, tmp.path(), &id, "s1", 1, "snapshot").unwrap();
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}/artifacts", id)).body(Body::empty()).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        let turns = v["turns"].as_array().unwrap();
        assert_eq!(turns.len(), 1);
        assert_eq!(turns[0]["turnId"], 1);
        let items = turns[0]["items"].as_array().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0]["path"], "src/new.txt");
        assert_eq!(items[0]["op"], "new");
        assert_eq!(items[0]["linesAdded"], 2);

        // deliverables：a.txt 标记在列
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}/deliverables", id)).body(Body::empty()).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        let items = v["items"].as_array().unwrap();
        assert!(items.iter().any(|i| i["path"] == "a.txt"));

        // 9. 删除 → real_dir 与 DB 清空
        let resp = router.clone().oneshot(Request::builder().method("DELETE").uri(format!("/api/workspaces/{}", id)).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(!std::path::Path::new(&real_dir).exists());
        let resp = router.clone().oneshot(Request::builder().uri(format!("/api/workspaces/{}", id)).body(Body::empty()).unwrap()).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn upload_rejects_output_and_traversal() {
        let tmp = tempfile::tempdir().unwrap();
        let db = crate::gateway::db::Db::open(tmp.path()).unwrap();
        let state = test_state(db, tmp.path().to_path_buf());
        let router = test_router(state);

        let resp = router.clone().oneshot(json_request("POST", "/api/workspaces", json!({"name": "Demo"}))).await.unwrap();
        let id = body_json(resp).await["workspace"]["id"].as_str().unwrap().to_string();

        let boundary = "b2";
        let mk = |filename: &str| {
            format!(
                "--{b}\r\nContent-Disposition: form-data; name=\"files\"; filename=\"{f}\"\r\nContent-Type: text/plain\r\n\r\nx\r\n--{b}--\r\n",
                b = boundary, f = filename
            )
        };
        // output/ 拒绝
        let resp = router.clone().oneshot(Request::builder().method("POST").uri(format!("/api/workspaces/{}/upload", id)).header("content-type", format!("multipart/form-data; boundary={}", boundary)).body(Body::from(mk("output/report.md"))).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["rejected"][0]["reason"], "output_path_excluded");
        // 绝对路径/穿越拒绝
        let resp = router.clone().oneshot(Request::builder().method("POST").uri(format!("/api/workspaces/{}/upload", id)).header("content-type", format!("multipart/form-data; boundary={}", boundary)).body(Body::from(mk("../evil.txt"))).unwrap()).await.unwrap();
        let v = body_json(resp).await;
        assert_eq!(v["rejected"][0]["reason"], "path traversal not allowed");
    }

    /// 真服务器级验证：走 server.rs 的路由注册 + LAN 鉴权中间件（loopback 豁免），
    /// 确保 /api/workspaces/* 真的挂到了实际 Router 上（oneshot 用自建 Router，
    /// 发现不了 server.rs 的路由拼写错误）。
    #[tokio::test]
    async fn live_server_workspace_routes() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = tempfile::tempdir().unwrap();
        let (state, port) = GatewayState::start_gateway(
            std::path::PathBuf::from("pi-does-not-exist"),
            "0.0.0".to_string(),
            dist.path().to_path_buf(),
            None, // work SPA 未部署
            None, // 随机端口
            None,
            tmp.path().to_path_buf(),
        )
        .unwrap();

        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{}", port);

        // 创建
        let resp = client
            .post(format!("{}/api/workspaces", base))
            .json(&json!({ "name": "Live" }))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let v: Value = resp.json().await.unwrap();
        let id = v["workspace"]["id"].as_str().unwrap().to_string();
        assert!(id.starts_with("ws_"));

        // 列表 / 详情
        let v: Value = client.get(format!("{}/api/workspaces", base)).send().await.unwrap().json().await.unwrap();
        assert!(v["workspaces"].as_array().unwrap().iter().any(|w| w["id"] == id));
        let v: Value = client.get(format!("{}/api/workspaces/{}", base, id)).send().await.unwrap().json().await.unwrap();
        assert_eq!(v["workspace"]["name"], "Live");

        // 未知 id → 404（且 JSON 错误体，非 SPA HTML）
        let resp = client.get(format!("{}/api/workspaces/ws_nope", base)).send().await.unwrap();
        assert_eq!(resp.status(), 404);
        assert!(resp.headers().get("content-type").unwrap().to_str().unwrap().contains("json"));

        // 删除应连带清理工作空间拥有的会话（DB 行 + 会话 jsonl 文件）。
        let ws_dir = state.db.get_project(&id).unwrap().cwd;
        let session_file = format!("{}/sess.jsonl", ws_dir);
        state
            .db
            .register_session("ws_sess_test", &ws_dir, Some(&id))
            .unwrap();
        state
            .db
            .complete_session("ws_sess_test", &session_file)
            .unwrap();
        std::fs::write(&session_file, "{}\n").unwrap();

        let resp = client.delete(format!("{}/api/workspaces/{}", base, id)).send().await.unwrap();
        assert_eq!(resp.status(), 200);

        // 会话行已被连带删除，文件随 real_dir 一起移除。
        assert!(state.db.get_session_path("ws_sess_test").is_none());
        assert!(!std::path::Path::new(&session_file).exists());

        state.kill_all();
    }

    /// 多 SPA 分发 + /api/connections 的服务器级验证（0.3.0「工作视图与下载」）：
    /// - `/` → 307 重定向 `/chat`
    /// - `/chat` → chat SPA index.html
    /// - `/work`、`/workspaces/:id` → work SPA index.html（history fallback）
    /// - `/api/connections` → 空列表 JSON
    #[tokio::test]
    async fn live_server_spa_routes_and_connections() {
        let tmp = tempfile::tempdir().unwrap();
        let chat_dist = tempfile::tempdir().unwrap();
        let work_dist = tempfile::tempdir().unwrap();
        std::fs::write(chat_dist.path().join("index.html"), "<h1>chat</h1>").unwrap();
        std::fs::write(work_dist.path().join("index.html"), "<h1>work</h1>").unwrap();
        let (state, port) = GatewayState::start_gateway(
            std::path::PathBuf::from("pi-does-not-exist"),
            "0.0.0".to_string(),
            chat_dist.path().to_path_buf(),
            Some(work_dist.path().to_path_buf()),
            None, // 随机端口
            None,
            tmp.path().to_path_buf(),
        )
        .unwrap();

        // 关闭自动跟随重定向，才能断言 307
        let client = reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .unwrap();
        let base = format!("http://127.0.0.1:{}", port);

        // `/` → 307 /chat
        let resp = client.get(format!("{}/", base)).send().await.unwrap();
        assert_eq!(resp.status(), 307);
        assert_eq!(
            resp.headers().get("location").unwrap().to_str().unwrap(),
            "/chat"
        );

        // `/chat` → chat SPA
        let resp = client.get(format!("{}/chat", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert!(resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("text/html"));
        assert_eq!(resp.text().await.unwrap(), "<h1>chat</h1>");

        // `/work` → work SPA（history fallback 到 index.html）
        let resp = client.get(format!("{}/work", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "<h1>work</h1>");

        // `/workspaces/:id` → work SPA
        let resp = client.get(format!("{}/workspaces/ws_x", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "<h1>work</h1>");

        // /api/connections → 空列表（无 WS 连接时）
        let resp = client.get(format!("{}/api/connections", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let v: Value = resp.json().await.unwrap();
        assert_eq!(v["connections"].as_array().unwrap().len(), 0);

        state.kill_all();
    }

    /// work 未部署（work_dist=None）时，/work 返回 404 而非 HTML。
    #[tokio::test]
    async fn live_server_work_undeployed_returns_404() {
        let tmp = tempfile::tempdir().unwrap();
        let chat_dist = tempfile::tempdir().unwrap();
        std::fs::write(chat_dist.path().join("index.html"), "<h1>chat</h1>").unwrap();
        let (state, port) = GatewayState::start_gateway(
            std::path::PathBuf::from("pi-does-not-exist"),
            "0.0.0".to_string(),
            chat_dist.path().to_path_buf(),
            None, // work 未部署
            None,
            None,
            tmp.path().to_path_buf(),
        )
        .unwrap();

        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{}", port);
        let resp = client.get(format!("{}/work", base)).send().await.unwrap();
        assert_eq!(resp.status(), 404);

        // chat 侧不受影响
        let resp = client.get(format!("{}/chat", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert_eq!(resp.text().await.unwrap(), "<h1>chat</h1>");

        state.kill_all();
    }

    /// 分享页端点（0.3.0「分享与连接页」）：
    /// - `/api/lan-qr` 与 `/api/lan-qr?path=/work` 均返回 SVG（work 卡片二维码）
    /// - 白名单外 path 回落默认、不崩溃
    /// - `/api/mdns/status` 结构含 enabled / proto
    #[tokio::test]
    async fn live_server_share_endpoints() {
        let tmp = tempfile::tempdir().unwrap();
        let dist = tempfile::tempdir().unwrap();
        let (state, port) = GatewayState::start_gateway(
            std::path::PathBuf::from("pi-does-not-exist"),
            "0.0.0".to_string(),
            dist.path().to_path_buf(),
            None,
            None,
            None,
            tmp.path().to_path_buf(),
        )
        .unwrap();

        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{}", port);

        // chat QR（默认）
        let resp = client.get(format!("{}/api/lan-qr", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        assert!(resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("svg"));

        // work QR（?path=/work）
        let resp = client
            .get(format!("{}/api/lan-qr?path=/work", base))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert!(resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("svg"));

        // 白名单外 path 回落默认、不崩溃
        let resp = client
            .get(format!("{}/api/lan-qr?path=../etc", base))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);

        // mdns status 结构
        let resp = client
            .get(format!("{}/api/mdns/status", base))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        let v: Value = resp.json().await.unwrap();
        if v["enabled"] == true {
            assert!(v["proto"].is_string());
            assert!(v["port"].is_u64());
            assert!(v["instanceName"].is_string());
        } else {
            assert_eq!(v["enabled"], false);
        }

        state.kill_all();
    }

    /// 真实 Flutter Web 产物分发冒烟（需先 `flutter build web --base-href=/work/` + `pnpm build:chat`）。
    /// 手动运行：`cargo test -p pi_server live_server_work_web_dist -- --ignored --nocapture`
    #[tokio::test]
    #[ignore = "依赖 work/build/web 与 chat/dist 构建产物，手动运行"]
    async fn live_server_work_web_dist() {
        let root = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .to_path_buf();
        let work_dist = root.join("work").join("build").join("web");
        let chat_dist = root.join("chat").join("dist");
        assert!(work_dist.join("index.html").exists(), "先 flutter build web --base-href=/work/");
        assert!(chat_dist.join("index.html").exists(), "先 pnpm build:chat");

        let tmp = tempfile::tempdir().unwrap();
        let (state, port) = GatewayState::start_gateway(
            std::path::PathBuf::from("pi-does-not-exist"),
            "0.0.0".to_string(),
            chat_dist,
            Some(work_dist),
            None,
            None,
            tmp.path().to_path_buf(),
        )
        .unwrap();

        let client = reqwest::Client::new();
        let base = format!("http://127.0.0.1:{}", port);

        // /work → Flutter index.html
        let resp = client.get(format!("{}/work", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let html = resp.text().await.unwrap();
        assert!(html.contains("flutter"), "Flutter index.html 应含 flutter 引导");

        // 静态资源正确分发（MIME 表覆盖 js）
        let resp = client
            .get(format!("{}/work/flutter_bootstrap.js", base))
            .send()
            .await
            .unwrap();
        assert_eq!(resp.status(), 200);
        assert!(resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("javascript"));

        // /chat 不受影响（base=/chat/ 前缀后，资源也从 /chat/assets 命中）
        let resp = client.get(format!("{}/chat", base)).send().await.unwrap();
        assert_eq!(resp.status(), 200);
        let chat_html = resp.text().await.unwrap();
        assert!(chat_html.contains("<!doctype html>"));
        // 解析产物里第一个 /chat/assets/... 资源并验证可加载（base 前缀统一回归）。
        let asset = chat_html
            .split('"')
            .find(|s| s.starts_with("/chat/assets/") && s.ends_with(".js"))
            .expect("chat index.html 应引用 /chat/assets 前缀的 js");
        let resp = client.get(format!("{}{}", base, asset)).send().await.unwrap();
        assert_eq!(resp.status(), 200, "chat 资源 {} 应可从 /chat 前缀加载", asset);
        assert!(resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .contains("javascript"));

        state.kill_all();
    }
}
