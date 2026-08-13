//! 工作空间领域逻辑（0.3.0）。
//!
//! 做什么：工作空间（projects 表 type='workspace' 的行）的生命周期与文件操作——
//! 创建/删除/文件扫描/快照 diff（turn_artifacts 数据源）/上传校验/下载锚定/
//! zip 打包/写边界软约束（`.pi/constraint.ts` 生成 + `.pi/approvals.json` 白名单）。
//! 不做什么：REST/WS 暴露（handlers/workspace.rs、ws/）；表 CRUD（db/workspace.rs）。
//! 依赖：Db、data_dir（real_dir 固定为 `<data_dir>/workspaces/<id>/`）；路径安全
//! 基于"real_dir 锚定 + 相对路径组件校验"（防 `..`/绝对路径穿越）。

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde_json::json;

use super::db::{ArtifactRow, Db};

// ─── Types ──────────────────────────────────────────────────────────────────

/// Workspace view for REST（= projects 表 type='workspace' 的行 + 文件统计 + mode）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
    /// real_dir（工作空间根目录，绝对路径）。
    pub cwd: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub file_count: i64,
    pub size_bytes: i64,
    /// 写边界模式：ask | allow | deny（默认 ask）。
    pub mode: String,
}

/// Flat directory-tree node（相对 real_dir）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    #[serde(rename = "type")]
    pub kind: String,
    pub size: i64,
    pub mtime: i64,
    pub is_deliverable: bool,
}

// ─── 目录约定 ──────────────────────────────────────────────────────────────

/// `<data_dir>/workspaces`
pub fn workspaces_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("workspaces")
}

/// `<data_dir>/workspaces/<id>`
pub fn workspace_dir(data_dir: &Path, id: &str) -> PathBuf {
    workspaces_dir(data_dir).join(id)
}

/// 扫描时跳过的目录（避免把 node_modules/.git 当产物扫进来；`.pi` 是 piter/pi
/// 自身的配置目录，同样不视为工作空间内容）。
const SKIP_DIRS: &[&str] = &[".git", "node_modules", ".pi"];

/// 目录可写校验（基目录首启/配置时用）：建目录 + 写探针文件。
pub fn dir_writable(path: &Path) -> bool {
    if std::fs::create_dir_all(path).is_err() {
        return false;
    }
    let probe = path.join(".piter-write-test");
    match std::fs::write(&probe, b"ok") {
        Ok(()) => {
            let _ = std::fs::remove_file(&probe);
            true
        }
        Err(_) => false,
    }
}

fn rfc3339_to_ms(s: &str) -> i64 {
    chrono::DateTime::parse_from_rfc3339(s)
        .map(|dt| dt.timestamp_millis())
        .unwrap_or(0)
}

// ─── 生命周期 ───────────────────────────────────────────────────────────────

/// 创建工作空间：生成 `ws_<hex8>` id → 建 real_dir → 写约束扩展 → 注册 project 行。
/// `base_dir` 为工作空间基目录（配置优先 → 安装目录 → data_dir 回退，见
/// state.rs）；real_dir = `<base_dir>/workspaces/<id>`。
pub fn create_workspace(db: &Db, base_dir: &Path, name: &str) -> Result<Workspace, String> {
    let id = format!(
        "ws_{}",
        &uuid::Uuid::new_v4().to_string().replace('-', "")[..8]
    );
    let dir = workspace_dir(base_dir, &id);
    std::fs::create_dir_all(&dir).map_err(|e| format!("create workspace dir: {}", e))?;
    db.create_workspace_project(&id, name, &dir.to_string_lossy())?;
    // 写边界软约束：constraint.ts + workspace.json（ask 默认）。
    write_boundary_files(&dir, "ask")?;
    // 把 constraint 注册为项目扩展 → 该工作空间所有会话 spawn 时自动带上。
    db.update_project(&id, None, Some(&["constraint".to_string()]))?;
    get_workspace(db, base_dir, &id).ok_or_else(|| "workspace create failed".to_string())
}

/// 删除工作空间：DB 行（artifacts/marks 级联，snapshots 显式清）→ 删 real_dir。
pub fn delete_workspace(db: &Db, _base_dir: &Path, id: &str) -> Result<(), String> {
    let dir = workspace_dir_from_id(db, id)?;
    let sessions = db.get_project_sessions(id);
    db.delete_workspace_artifacts(id)?;
    db.delete_workspace_marks(id)?;
    for s in sessions {
        let _ = db.delete_snapshot(&s);
    }
    db.delete_project(id)?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).map_err(|e| format!("remove workspace dir: {}", e))?;
    }
    Ok(())
}

pub fn get_workspace(db: &Db, _base_dir: &Path, id: &str) -> Option<Workspace> {
    let row = db.get_project(id)?;
    if row.project_type != "workspace" {
        return None;
    }
    // 一律用 DB 中的 cwd（real_dir）——基目录迁移中途也不受影响。
    let dir = PathBuf::from(&row.cwd);
    let (file_count, size_bytes) = workspace_stats(&dir);
    Some(Workspace {
        id: row.id,
        name: row.name,
        cwd: row.cwd,
        created_at: rfc3339_to_ms(&row.created_at),
        updated_at: rfc3339_to_ms(&row.updated_at),
        file_count,
        size_bytes,
        mode: db.get_project_mode(id),
    })
}

pub fn list_workspaces(db: &Db, base_dir: &Path) -> Vec<Workspace> {
    db.list_projects(true)
        .into_iter()
        .filter(|p| p.project_type == "workspace" && !p.archived)
        .filter_map(|p| get_workspace(db, base_dir, &p.id))
        .collect()
}

/// 更新写边界模式：写 projects.mode + 刷新 `.pi/workspace.json`。
pub fn set_workspace_mode(
    db: &Db,
    _base_dir: &Path,
    id: &str,
    mode: &str,
) -> Result<Workspace, String> {
    db.set_project_mode(id, mode)?;
    let dir = workspace_dir_from_id(db, id)?;
    write_boundary_files(&dir, mode)?;
    get_workspace(db, _base_dir, id).ok_or_else(|| "workspace not found".to_string())
}

// ─── 文件扫描 / 统计 ───────────────────────────────────────────────────────

/// 递归扫描 real_dir：扁平 FileEntry 列表（含 dir 节点），跳过 SKIP_DIRS。
/// `is_marked` 判定该路径是否被手动标记为交付物（决定 isDeliverable）。
pub fn scan_files(real_dir: &Path, is_marked: impl Fn(&str) -> bool) -> Vec<FileEntry> {
    let mut out = Vec::new();
    let base = real_dir.to_path_buf();
    scan_dir(&base, &base, &is_marked, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

fn scan_dir(
    dir: &Path,
    base: &Path,
    is_marked: &impl Fn(&str) -> bool,
    out: &mut Vec<FileEntry>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        let Ok(rel) = p.strip_prefix(base) else {
            continue;
        };
        let rel_str = rel.to_string_lossy().replace('\\', "/");
        let is_dir = p.is_dir();
        if is_dir {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            out.push(FileEntry {
                kind: "dir".into(),
                path: rel_str.clone(),
                size: 0,
                mtime: mtime_ms(&p),
                is_deliverable: false,
            });
            scan_dir(&p, base, is_marked, out);
        } else {
            let size = std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(0);
            out.push(FileEntry {
                kind: "file".into(),
                path: rel_str.clone(),
                size,
                mtime: mtime_ms(&p),
                is_deliverable: is_marked(&rel_str) || is_output_path(&rel_str),
            });
        }
    }
}

/// 文件级 (path → (size, mtime_ms, lines)) 树，作为快照 diff 基线（仅文件）。
/// `lines` 为文件总行数（UTF-8 文本；二进制/读取失败 → 0），diff 时用于计算
/// 增删行数（+N −M）。
pub fn scan_tree(real_dir: &Path) -> BTreeMap<String, (i64, i64, i64)> {
    let mut map = BTreeMap::new();
    let base = real_dir.to_path_buf();
    collect_tree(&base, &base, &mut map);
    map
}

fn collect_tree(dir: &Path, base: &Path, map: &mut BTreeMap<String, (i64, i64, i64)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if SKIP_DIRS.contains(&name.as_str()) {
                continue;
            }
            collect_tree(&p, base, map);
        } else {
            let Ok(rel) = p.strip_prefix(base) else {
                continue;
            };
            let md = std::fs::metadata(&p).ok();
            map.insert(
                rel.to_string_lossy().replace('\\', "/"),
                (
                    md.as_ref().map(|m| m.len() as i64).unwrap_or(0),
                    md.and_then(|m| m.modified().ok())
                        .map(|t| t.duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis() as i64).unwrap_or(0))
                        .unwrap_or(0),
                    line_count(&p),
                ),
            );
        }
    }
}

/// 文件行数（UTF-8 文本行数）；读取失败（二进制/权限）→ 0。
fn line_count(p: &Path) -> i64 {
    std::fs::read_to_string(p)
        .map(|s| s.lines().count() as i64)
        .unwrap_or(0)
}

fn mtime_ms(p: &Path) -> i64 {
    std::fs::metadata(p)
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

/// (file_count, size_bytes)——只统计文件（跳过 SKIP_DIRS）。
pub fn workspace_stats(real_dir: &Path) -> (i64, i64) {
    let mut count = 0i64;
    let mut size = 0i64;
    let base = real_dir.to_path_buf();
    stats_dir(&base, &base, &mut count, &mut size);
    (count, size)
}

fn stats_dir(dir: &Path, base: &Path, count: &mut i64, size: &mut i64) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let p = entry.path();
        let name = entry.file_name().to_string_lossy().to_string();
        if p.is_dir() {
            if !SKIP_DIRS.contains(&name.as_str()) {
                stats_dir(&p, base, count, size);
            }
        } else {
            *count += 1;
            *size += std::fs::metadata(&p).map(|m| m.len() as i64).unwrap_or(0);
        }
    }
}

// ─── 快照 diff（turn_artifacts 数据源）──────────────────────────────────────

/// 树 JSON：`{"<rel>":{"s":size,"m":mtime,"l":lines}}`（紧凑、map 语义便于 O(1) diff）。
pub fn tree_to_json(map: &BTreeMap<String, (i64, i64, i64)>) -> String {
    let obj: serde_json::Map<String, serde_json::Value> = map
        .iter()
        .map(|(k, (s, m, l))| (k.clone(), json!({"s": s, "m": m, "l": l})))
        .collect();
    serde_json::Value::Object(obj).to_string()
}

pub fn tree_from_json(json_str: &str) -> BTreeMap<String, (i64, i64, i64)> {
    let Ok(obj) = serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(json_str)
    else {
        return BTreeMap::new();
    };
    obj.into_iter()
        .filter_map(|(k, v)| {
            let s = v.get("s").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let m = v.get("m").and_then(serde_json::Value::as_i64).unwrap_or(0);
            let l = v.get("l").and_then(serde_json::Value::as_i64).unwrap_or(0);
            Some((k, (s, m, l)))
        })
        .collect()
}

/// Diff 两棵文件树 → `(rel_path, op, size, lines_added, lines_deleted)`。
/// op ∈ new | modified | deleted。行数统计为净变化：
/// - new：added = 文件总行数；
/// - modified：added/deleted = 新旧总行数之差的正/负侧（整段改写净 0 时显示 0/0）；
/// - deleted：deleted = 原文件总行数。
pub fn diff_trees(
    old: &BTreeMap<String, (i64, i64, i64)>,
    new: &BTreeMap<String, (i64, i64, i64)>,
) -> Vec<(String, &'static str, i64, i64, i64)> {
    let mut out = Vec::new();
    for (path, new_v) in new {
        match old.get(path) {
            None => out.push((path.clone(), "new", new_v.0, new_v.2, 0)),
            Some(old_v) if old_v != new_v => out.push((
                path.clone(),
                "modified",
                new_v.0,
                (new_v.2 - old_v.2).max(0),
                (old_v.2 - new_v.2).max(0),
            )),
            _ => {}
        }
    }
    for (path, (size, _, lines)) in old {
        if !new.contains_key(path) {
            out.push((path.clone(), "deleted", *size, 0, *lines));
        }
    }
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// `output/` 前缀路径自动视为交付物。
pub fn is_output_path(rel: &str) -> bool {
    rel.starts_with("output/")
}

/// 上传成功后，把新文件并入各会话的快照基线——用户上传的内容不属于 agent
/// 产物，避免下一轮 turn_end diff 把上传文件误判为 `new` 产物。只更新已存在
/// 快照的会话；尚无快照的会话首轮 diff 会把它们当 new（可接受的边界）。
pub fn note_uploaded_files(
    db: &Db,
    workspace_id: &str,
    rel_paths: &[String],
) -> Result<(), String> {
    if rel_paths.is_empty() {
        return Ok(());
    }
    let dir = workspace_dir_from_id(db, workspace_id)?;
    let sessions = db.get_project_sessions(workspace_id);
    for session in sessions {
        if let Some(snap) = db.get_snapshot(&session) {
            let mut tree = tree_from_json(&snap.tree_json);
            for rel in rel_paths {
                let p = dir.join(rel);
                let md = std::fs::metadata(&p).ok();
                tree.insert(
                    rel.clone(),
                    (
                        md.as_ref().map(|m| m.len() as i64).unwrap_or(0),
                        md.and_then(|m| m.modified().ok())
                            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                            .map(|d| d.as_millis() as i64)
                            .unwrap_or(0),
                        line_count(&p),
                    ),
                );
            }
            db.set_snapshot(&session, &tree_to_json(&tree))?;
        }
    }
    Ok(())
}

/// 由 DB 的 workspace 行拿到 real_dir（不存在/非 workspace 类型 → 错误）。
pub fn workspace_dir_from_id(db: &Db, id: &str) -> Result<PathBuf, String> {
    let row = db
        .get_project(id)
        .ok_or_else(|| format!("workspace not found: {}", id))?;
    if row.project_type != "workspace" {
        return Err(format!("not a workspace: {}", id));
    }
    Ok(PathBuf::from(&row.cwd))
}

/// 把一个会话（instance_id）映射到其所属工作空间 id；非工作空间会话 → None。
/// 事件循环（turn_end / tool_execution）用它判断是否走 workspace 集成路径。
pub fn workspace_id_for_session(db: &Db, instance_id: &str) -> Option<String> {
    let row = db.all_sessions().into_iter().find(|s| s.instance_id == instance_id)?;
    let project_id = row.project_id?;
    let proj = db.get_project(&project_id)?;
    (proj.project_type == "workspace").then_some(proj.id)
}

/// 确保会话已有快照基线（无则静默扫描当前树建基线）。会话建立/恢复时调用，
/// 防止首轮 diff 把存量文件（读文件/历史文件）误报为 `new`。
pub fn ensure_session_baseline(
    db: &Db,
    workspace_id: &str,
    session_id: &str,
) -> Result<(), String> {
    if db.get_snapshot(session_id).is_some() {
        return Ok(());
    }
    let dir = workspace_dir_from_id(db, workspace_id)?;
    let tree = scan_tree(&dir);
    db.set_snapshot(session_id, &tree_to_json(&tree))
}

/// 快照 diff 主入口：读旧树 → 扫新树 → diff → 落 artifacts 行 → 覆盖更新快照。
/// 返回本轮的 ArtifactRow（WS turn_artifacts 推送用）。
/// 会话尚无快照基线（旧会话 / ensure 未跑）→ 只建基线不产出，避免存量误报。
pub fn capture_turn_artifacts(
    db: &Db,
    _base_dir: &Path,
    workspace_id: &str,
    session_id: &str,
    turn_id: i64,
    source: &str,
) -> Result<Vec<ArtifactRow>, String> {
    let dir = workspace_dir_from_id(db, workspace_id)?;
    let snapshot = db.get_snapshot(session_id);
    if snapshot.is_none() {
        let baseline = scan_tree(&dir);
        db.set_snapshot(session_id, &tree_to_json(&baseline))?;
        return Ok(Vec::new());
    }
    let old = tree_from_json(&snapshot.unwrap().tree_json);
    let new = scan_tree(&dir);
    let diffs = diff_trees(&old, &new);
    let now = chrono::Utc::now().to_rfc3339();
    let rows: Vec<ArtifactRow> = diffs
        .iter()
        .enumerate()
        .map(|(i, (path, op, size, added, deleted))| ArtifactRow {
            id: format!(
                "art_{}_{}",
                &uuid::Uuid::new_v4().to_string().replace('-', "")[..8],
                i
            ),
            workspace_id: workspace_id.to_string(),
            session_id: session_id.to_string(),
            turn_id,
            rel_path: path.clone(),
            op: op.to_string(),
            size: *size,
            lines_added: *added,
            lines_deleted: *deleted,
            source: source.to_string(),
            deliverable: is_output_path(path) || db.is_deliverable_marked(workspace_id, path),
            created_at: now.clone(),
        })
        .collect();
    db.insert_artifacts(&rows)?;
    db.set_snapshot(session_id, &tree_to_json(&new))?;
    Ok(rows)
}

// ─── 路径安全：校验 / 锚定 / 包含判定 ───────────────────────────────────────

/// 校验并规范化用户提供的相对路径（上传/下载/标记共用）。
/// 拒绝：空、绝对路径（`/`、盘符、UNC）、`..` 穿越、空组件。返回 `/` 分隔。
pub fn clean_rel_path(raw: &str) -> Result<String, String> {
    let raw = raw.trim().replace('\\', "/");
    let raw = raw.strip_prefix("./").unwrap_or(&raw);
    if raw.is_empty() {
        return Err("empty path".to_string());
    }
    if raw.starts_with('/') || raw.starts_with("//") {
        return Err("absolute path not allowed".to_string());
    }
    if raw.len() >= 2 && raw.as_bytes()[1] == b':' {
        return Err("absolute path not allowed".to_string());
    }
    for comp in raw.split('/') {
        if comp.is_empty() {
            return Err("empty path component".to_string());
        }
        if comp == ".." {
            return Err("path traversal not allowed".to_string());
        }
    }
    Ok(raw.to_string())
}

/// 上传路径额外校验：`output/` 目录属 agent 产物区，拒绝用户上传。
pub fn clean_upload_path(raw: &str) -> Result<String, String> {
    let rel = clean_rel_path(raw)?;
    if rel == "output" || rel.starts_with("output/") {
        return Err("output_path_excluded".to_string());
    }
    Ok(rel)
}

/// 把已校验的 rel 路径锚定到 real_dir 并返回规范化的绝对路径（用于下载读取）。
/// 双重保险：clean_rel_path 之后再做一次 canonicalize 包含判定。
pub fn anchor_path(real_dir: &Path, rel: &str) -> Result<PathBuf, String> {
    let rel = clean_rel_path(rel)?;
    let base = real_dir
        .canonicalize()
        .map_err(|e| format!("workspace dir missing: {}", e))?;
    let target = base.join(&rel);
    let target = target
        .canonicalize()
        .map_err(|e| format!("file not found: {}", e))?;
    if !target.starts_with(&base) {
        return Err("outside workspace".to_string());
    }
    Ok(target)
}

/// `target`（绝对路径）是否位于 `base`（绝对路径）内。
/// 比较前统一规范化：去 verbatim 前缀、`\`→`/`、Windows 忽略大小写、路径边界——
/// 否则 canonicalize 的 `\\?\` 前缀（或大小写差异）会让内部路径被误判为外部
/// （新文件 canonicalize 失败保留字面路径时必现），导致内部写入也弹询问。
pub fn is_inside(base: &Path, target: &Path) -> bool {
    let b = norm_for_cmp(base);
    let t = norm_for_cmp(target);
    t == b || t.starts_with(&format!("{}/", b))
}

fn norm_for_cmp(p: &Path) -> String {
    let s = crate::broker::util::strip_verbatim_prefix(p.to_string_lossy().as_ref())
        .replace('\\', "/");
    #[cfg(windows)]
    let s = s.to_lowercase();
    s.trim_end_matches('/').to_string()
}

// ─── zip 打包 ───────────────────────────────────────────────────────────────

/// 把选中的相对路径打包为 zip 字节流（内存态；单次 ≤ 整树，v1 直接流式返回）。
/// `all=true` 打包整树（跳过 SKIP_DIRS）；否则仅打包 `rel_paths`（逐个锚定校验）。
pub fn zip_files(real_dir: &Path, rel_paths: &[String], all: bool) -> Result<Vec<u8>, String> {
    let base = real_dir
        .canonicalize()
        .map_err(|e| format!("workspace dir missing: {}", e))?;
    let mut paths: Vec<String> = if all {
        scan_tree(&base).into_keys().collect()
    } else {
        let mut set = HashSet::new();
        for rel in rel_paths {
            set.insert(clean_rel_path(rel)?);
        }
        set.into_iter().collect()
    };
    paths.sort();

    let cursor = std::io::Cursor::new(Vec::new());
    let mut zip = zip::ZipWriter::new(cursor);
    let options: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated);
    for rel in &paths {
        let src = base.join(rel);
        let Ok(mut f) = std::fs::File::open(&src) else {
            continue;
        };
        zip.start_file(rel.clone(), options)
            .map_err(|e| format!("zip start_file {}: {}", rel, e))?;
        std::io::copy(&mut f, &mut zip)
            .map_err(|e| format!("zip write {}: {}", rel, e))?;
    }
    let finish = zip.finish().map_err(|e| format!("zip finish: {}", e))?;
    Ok(finish.into_inner())
}

// ─── 写边界软约束（constraint.ts + approvals.json）──────────────────────────

/// 约束扩展读取的边界配置：`.pi/workspace.json`。
pub fn boundary_config_file(real_dir: &Path) -> PathBuf {
    real_dir.join(".pi").join("workspace.json")
}

/// 审批白名单文件：`.pi/approvals.json`（`{"approvedPaths": [abs...]}`）。
pub fn approvals_file(real_dir: &Path) -> PathBuf {
    real_dir.join(".pi").join("approvals.json")
}

/// 当前审批白名单（绝对路径集合，规范化后）。
pub fn approvals_set(real_dir: &Path) -> HashSet<String> {
    let path = approvals_file(real_dir);
    let Ok(text) = std::fs::read_to_string(&path) else {
        return HashSet::new();
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return HashSet::new();
    };
    v.get("approvedPaths")
        .and_then(|a| a.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|x| x.as_str())
                .map(|s| crate::broker::util::strip_verbatim_prefix(s).replace('\\', "/"))
                .collect()
        })
        .unwrap_or_default()
}

/// 把一个绝对路径加入审批白名单（规范化：去 verbatim 前缀、`\`→`/`，追加 + 排序去重）。
pub fn add_approval(real_dir: &Path, abs_path: &str) -> Result<(), String> {
    let abs = crate::broker::util::strip_verbatim_prefix(abs_path).replace('\\', "/");
    let pi_dir = real_dir.join(".pi");
    std::fs::create_dir_all(&pi_dir).map_err(|e| format!("create .pi dir: {}", e))?;
    let mut set = approvals_set(real_dir);
    set.insert(abs);
    let mut paths: Vec<String> = set.into_iter().collect();
    paths.sort();
    let tmp = approvals_file(real_dir);
    let json_str = json!({ "approvedPaths": paths }).to_string();
    std::fs::write(&tmp, json_str).map_err(|e| format!("write approvals: {}", e))?;
    Ok(())
}

/// 写边界配置文件 + 约束扩展（真实目录、会话可用的工作空间创建/模式变更时调用）。
/// `constraint` 作为项目扩展注册后，工作空间会话 spawn 自动携带。
pub fn write_boundary_files(real_dir: &Path, mode: &str) -> Result<(), String> {
    let pi_dir = real_dir.join(".pi");
    let ext_dir = pi_dir.join("extensions");
    std::fs::create_dir_all(&ext_dir).map_err(|e| format!("create .pi dir: {}", e))?;
    std::fs::write(boundary_config_file(real_dir), json!({ "mode": mode }).to_string())
        .map_err(|e| format!("write workspace.json: {}", e))?;
    std::fs::write(ext_dir.join("constraint.ts"), constraint_ts_source(real_dir))
        .map_err(|e| format!("write constraint.ts: {}", e))?;
    Ok(())
}

/// 生成的 constraint 扩展源码：在 tool_call 阶段拦截 write/edit 的越界写入。
/// - `allow` 模式：放行一切；
/// - `deny` 模式：工作空间外一律 block（无批准通道）；
/// - `ask` 模式：工作空间外 block，除非该绝对路径在 `.pi/approvals.json` 白名单。
/// 模式从 `.pi/workspace.json` 实时读取，改模式无需重写扩展。
pub fn constraint_ts_source(real_dir: &Path) -> String {
    // 工作空间根目录以字符串形式烙进源码（运行时 cwd 即此目录）。
    let cwd_esc = real_dir.to_string_lossy().replace('\\', "/");
    format!(
        r#"// Auto-generated by piter (0.3.0 workspace write boundary).
// Blocks write/edit outside the workspace unless approved in .pi/approvals.json.
import * as fs from "node:fs";
import * as path from "node:path";
import type {{ ExtensionAPI }} from "@earendil-works/pi-coding-agent";

export default function workspaceBoundary(pi: ExtensionAPI) {{
  const cwd = path.resolve("{cwd_esc}");

  const mode = (): string => {{
    try {{
      const cfg = JSON.parse(fs.readFileSync(path.join(cwd, ".pi", "workspace.json"), "utf8"));
      return typeof cfg.mode === "string" ? cfg.mode : "ask";
    }} catch {{ return "ask"; }}
  }};

  const approved = (): Set<string> => {{
    try {{
      const data = JSON.parse(fs.readFileSync(path.join(cwd, ".pi", "approvals.json"), "utf8"));
      return new Set((data.approvedPaths ?? []).map((p: string) => path.normalize(p)));
    }} catch {{ return new Set(); }}
  }};

  const inside = (target: string): boolean => {{
    const rel = path.relative(cwd, target);
    return rel === "" || (!rel.startsWith("..") && !path.isAbsolute(rel));
  }};

  for (const tool of ["write", "edit"] as const) {{
    pi.on("tool_call", async (event, ctx) => {{
      if (event.toolName !== tool) return undefined;
      const raw = (event.input as {{ path?: unknown }}).path as string | undefined;
      if (!raw) return undefined;
      const abs = path.resolve(cwd, raw);
      const m = mode();
      if (m === "allow") return undefined;
      if (inside(abs) || approved().has(path.normalize(abs))) return undefined;
      const reason = m === "deny"
        ? `写入位置应在工作空间内（cwd=${{cwd}}）`
        : `写入位置应在工作空间内（cwd=${{cwd}}）；如确实需要请在工作空间产物页批准`;
      return {{ block: true, reason }};
    }});
  }}
}}
"#
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::db::Db;

    fn make_db_and_dir() -> (tempfile::TempDir, std::sync::Arc<Db>) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        (dir, db)
    }

    #[test]
    fn clean_rel_path_rejects_traversal() {
        assert_eq!(clean_rel_path("./a/b.txt").unwrap(), "a/b.txt");
        assert_eq!(clean_rel_path("a\\b.txt").unwrap(), "a/b.txt");
        assert!(clean_rel_path("../etc/passwd").is_err());
        assert!(clean_rel_path("a/../../b").is_err());
        assert!(clean_rel_path("/abs").is_err());
        assert!(clean_rel_path("C:/x").is_err());
        assert!(clean_rel_path("//server/share").is_err());
        assert!(clean_rel_path("").is_err());
        assert!(clean_rel_path("a//b").is_err());
        // upload 额外拒绝 output/（agent 产物区）
        assert_eq!(clean_upload_path("docs/readme.md").unwrap(), "docs/readme.md");
        assert!(clean_upload_path("output/report.md").is_err());
    }

    #[test]
    fn scan_and_stats_skip_heavy_dirs() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/main.rs"), "fn main() {}").unwrap();
        std::fs::create_dir_all(dir.path().join("node_modules/pkg")).unwrap();
        std::fs::write(dir.path().join("node_modules/pkg/index.js"), "x").unwrap();
        std::fs::write(dir.path().join("README.md"), "hi").unwrap();

        let files = scan_files(dir.path(), |_| false);
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"README.md"));
        assert!(paths.contains(&"src"));
        assert!(paths.contains(&"src/main.rs"));
        assert!(!paths.contains(&"node_modules"));
        assert!(!paths.iter().any(|p| p.starts_with("node_modules")));

        let (count, size) = workspace_stats(dir.path());
        assert_eq!(count, 2);
        assert!(size >= 10);
    }

    #[test]
    fn diff_trees_classifies_ops() {
        use std::collections::BTreeMap;
        let mut old = BTreeMap::new();
        old.insert("a.txt".into(), (10, 100, 4));
        old.insert("gone.txt".into(), (5, 200, 2));
        old.insert("same.txt".into(), (7, 300, 3));
        let mut new = BTreeMap::new();
        new.insert("a.txt".into(), (12, 100, 7)); // size+lines 变 → modified
        new.insert("same.txt".into(), (7, 300, 3)); // 未变
        new.insert("fresh.txt".into(), (3, 400, 1)); // 新增

        let diffs = diff_trees(&old, &new);
        let ops: Vec<(&str, &str)> = diffs.iter().map(|(p, o, _, _, _)| (p.as_str(), *o)).collect();
        assert_eq!(ops, vec![
            ("a.txt", "modified"),
            ("fresh.txt", "new"),
            ("gone.txt", "deleted"),
        ]);
        // 行数统计：modified 净 +3；new 全量；deleted 全量。
        let a = diffs.iter().find(|(p, _, _, _, _)| p == "a.txt").unwrap();
        assert_eq!((a.3, a.4), (3, 0));
        let fresh = diffs.iter().find(|(p, _, _, _, _)| p == "fresh.txt").unwrap();
        assert_eq!((fresh.3, fresh.4), (1, 0));
        let gone = diffs.iter().find(|(p, _, _, _, _)| p == "gone.txt").unwrap();
        assert_eq!((gone.3, gone.4), (0, 2));
    }

    #[test]
    fn first_capture_with_existing_files_is_baseline_only() {
        // 会话无基线但工作空间已有文件（读文件/历史存量）→ 首轮只建基线，
        // 不把存量文件误报为 new。
        let (dir, db) = make_db_and_dir();
        let ws = create_workspace(&db, dir.path(), "Demo").unwrap();
        let wdir = workspace_dir(dir.path(), &ws.id);
        std::fs::create_dir_all(wdir.join("src")).unwrap();
        std::fs::write(wdir.join("src/main.rs"), "fn main() {}\nfn run() {}\n").unwrap();
        std::fs::write(wdir.join("README.md"), "hi\n").unwrap();

        let rows = capture_turn_artifacts(&db, dir.path(), &ws.id, "s1", 1, "snapshot").unwrap();
        assert!(rows.is_empty(), "无基线首轮不应产出存量文件");
        assert!(db.get_snapshot("s1").is_some(), "应建立基线快照");

        // 仅修改一个文件 → 下一轮只报它，且带行数。
        std::fs::write(wdir.join("src/main.rs"), "fn main() {}\n").unwrap();
        let rows = capture_turn_artifacts(&db, dir.path(), &ws.id, "s1", 2, "snapshot").unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].rel_path, "src/main.rs");
        assert_eq!(rows[0].op, "modified");
        assert_eq!((rows[0].lines_added, rows[0].lines_deleted), (0, 1));

        // ensure_session_baseline：已有基线时幂等；无基线时补建。
        ensure_session_baseline(&db, &ws.id, "s1").unwrap();
        ensure_session_baseline(&db, &ws.id, "s_other").unwrap();
        assert!(db.get_snapshot("s_other").is_some());
    }

    #[test]
    fn create_workspace_and_capture_turn_artifacts() {
        let (dir, db) = make_db_and_dir();
        let ws = create_workspace(&db, dir.path(), "Demo").unwrap();
        assert!(ws.id.starts_with("ws_"));
        assert!(workspace_dir(dir.path(), &ws.id).exists());

        // 基线：空工作空间首轮 → 无产物。
        let rows = capture_turn_artifacts(&db, dir.path(), &ws.id, "s1", 1, "snapshot").unwrap();
        assert!(rows.is_empty());

        // 写入一个文件 + output 产物 → 下一轮 diff 出 new 条目。
        let wdir = workspace_dir(dir.path(), &ws.id);
        std::fs::create_dir_all(wdir.join("output")).unwrap();
        std::fs::create_dir_all(wdir.join("src")).unwrap();
        std::fs::write(wdir.join("output/report.md"), "# report").unwrap();
        std::fs::write(wdir.join("src/main.rs"), "fn main() {}").unwrap();

        let rows = capture_turn_artifacts(&db, dir.path(), &ws.id, "s1", 2, "snapshot").unwrap();
        assert_eq!(rows.len(), 2);
        let report = rows.iter().find(|r| r.rel_path == "output/report.md").unwrap();
        assert_eq!(report.op, "new");
        assert!(report.deliverable, "output/ 路径自动可交付");
        // new 文件行数 = 文件总行数（"# report" 1 行；"fn main() {}" 1 行）。
        assert_eq!((report.lines_added, report.lines_deleted), (1, 0));
        let main = rows.iter().find(|r| r.rel_path == "src/main.rs").unwrap();
        assert!(!main.deliverable);
        assert_eq!((main.lines_added, main.lines_deleted), (1, 0));

        // 快照覆盖更新：删掉文件 → 第三轮出 deleted。
        std::fs::remove_file(wdir.join("src/main.rs")).unwrap();
        let rows = capture_turn_artifacts(&db, dir.path(), &ws.id, "s1", 3, "snapshot").unwrap();
        let del = rows.iter().find(|r| r.rel_path == "src/main.rs").unwrap();
        assert_eq!(del.op, "deleted");

        // artifacts 分组查询：新→旧。
        let all = db.list_artifacts(&ws.id, None).unwrap();
        assert_eq!(all.first().unwrap().turn_id, 3);
        assert!(db.list_artifacts(&ws.id, Some(2)).unwrap().iter().all(|r| r.turn_id > 2));

        // 手动标记 → deliverables 命中。
        db.set_deliverable_mark(&ws.id, "src/main.rs", true).unwrap();
        let delivs = db.list_deliverable_artifacts(&ws.id).unwrap();
        assert!(delivs.iter().any(|r| r.rel_path == "src/main.rs"));

        // 删除工作空间 → real_dir 与 DB 数据清空。
        delete_workspace(&db, dir.path(), &ws.id).unwrap();
        assert!(!workspace_dir(dir.path(), &ws.id).exists());
        assert!(db.list_artifacts(&ws.id, None).unwrap().is_empty());
    }

    #[test]
    fn approval_and_boundary_files() {
        let (dir, db) = make_db_and_dir();
        let ws = create_workspace(&db, dir.path(), "Bound").unwrap();
        let wdir = workspace_dir(dir.path(), &ws.id);

        // 默认 ask + constraint.ts 已生成且已注册为项目扩展。
        assert_eq!(ws.mode, "ask");
        assert!(wdir.join(".pi/extensions/constraint.ts").exists());
        assert!(wdir.join(".pi/workspace.json").exists());
        assert!(db.get_project_added_extensions(&ws.id).contains(&"constraint".to_string()));

        // approvals 白名单往返。
        assert!(approvals_set(&wdir).is_empty());
        add_approval(&wdir, r"E:\other\proj\x.txt").unwrap();
        let set = approvals_set(&wdir);
        assert!(set.contains("E:/other/proj/x.txt"));
        assert_eq!(set.len(), 1);

        // 模式切换刷新 workspace.json。
        set_workspace_mode(&db, dir.path(), &ws.id, "deny").unwrap();
        let cfg: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(wdir.join(".pi/workspace.json")).unwrap())
                .unwrap();
        assert_eq!(cfg["mode"], "deny");
    }

    #[test]
    #[cfg(windows)]
    fn is_inside_normalizes_verbatim_and_case() {
        use std::path::Path;
        let base = Path::new(r"\\?\E:\data\workspaces\ws_ab12cd");
        // 内部：大小写不同、正反斜杠混用、verbatim 缺失（新文件 canonicalize 失败场景）。
        assert!(is_inside(base, Path::new(r"e:\data\workspaces\ws_ab12cd\src/main.rs")));
        assert!(is_inside(base, Path::new(r"\\?\E:\data\workspaces\ws_ab12cd\output\a.md")));
        assert!(is_inside(base, Path::new(r"E:/data/workspaces/ws_ab12cd")));
        // 外部：前缀相似但不是同一目录。
        assert!(!is_inside(base, Path::new(r"E:\data\workspaces\ws_ab12cdef\x.txt")));
        assert!(!is_inside(base, Path::new(r"E:\data\workspaces\ws_ab12cd2\x.txt")));
        assert!(!is_inside(base, Path::new(r"E:\other\proj\x.txt")));
    }

    #[test]
    fn zip_builds_archive() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("a.txt"), "aaa").unwrap();
        std::fs::write(dir.path().join("sub/b.txt"), "bbbb").unwrap();

        let bytes = zip_files(dir.path(), &["a.txt".into(), "sub/b.txt".into()], false).unwrap();
        let cursor = std::io::Cursor::new(bytes);
        let archive = zip::ZipArchive::new(cursor).unwrap();
        assert_eq!(archive.len(), 2);
        let mut names = archive.file_names().collect::<Vec<_>>();
        names.sort();
        assert_eq!(names, vec!["a.txt", "sub/b.txt"]);

        // all=true 包含全部文件。
        let all_bytes = zip_files(dir.path(), &[], true).unwrap();
        let all_archive = zip::ZipArchive::new(std::io::Cursor::new(all_bytes)).unwrap();
        assert_eq!(all_archive.len(), 2);
    }
}
