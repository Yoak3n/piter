//! 每轮 agent 完成时的文件快照（checkpoint）与撤回时的恢复（消息撤回 0.2.0 P3）。
//!
//! 做什么：
//! - `create_checkpoint`：agent_end 时若会话 cwd 是 git 仓库，`git stash create`
//!   取 tracked 快照 ref，把 untracked 文件内容复制到 `<data_dir>/checkpoints/<id>/`，
//!   一行落 DB（gateway/db/checkpoints.rs），并裁剪到最近 N 轮。
//! - `restore_checkpoint`：撤回时把工作区恢复到"被撤回消息之前最近 checkpoint"
//!   的状态——tracked 用 `git restore --source=<ref>`，untracked 按 manifest
//!   还原旧内容（曾存在）或删除（本轮新建）。
//! 不做什么：不做消息撤回本身（fork 链路在 ws/broker/command.rs + responses.rs）。
//! 依赖：gateway/git.rs（git 原语）、GatewayState（data_dir / db / session_manager）。

use std::path::PathBuf;

use serde_json::json;

use super::GatewayState;

/// 每会话最多保留的 checkpoint 数（超出连同快照目录一起清理，防膨胀）。
const KEEP_CHECKPOINTS: usize = 20;

/// untracked 快照排除清单：常见依赖/构建产物目录，避免每轮全量复制爆炸。
const EXCLUDE_DIRS: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "dist",
    "build",
    ".venv",
    "venv",
    "__pycache__",
    ".next",
    ".cache",
    ".turbo",
    "coverage",
];

/// agent_end 时调用：会话 cwd 是 git 仓库且工作区有改动时落一个 checkpoint。
/// 全部失败静默降级（仅记日志）——checkpoint 是增强功能，不能阻塞会话生命周期。
pub fn create_checkpoint(state: &GatewayState, instance_id: &str) {
    let Some(cwd) = state
        .session_manager
        .lock()
        .sessions
        .get(instance_id)
        .map(|s| s.cwd.clone())
    else {
        return;
    };
    let cwd = PathBuf::from(cwd);
    if !super::git::is_git_repo(&cwd) {
        return;
    }

    let turn_seq = state.session_manager.lock().turn_count(instance_id) as i32;
    let Some(git_ref) = super::git::stash_create(&cwd) else {
        return; // 工作区无改动，无需快照
    };

    // 先落 DB 行拿自增 id（同时作为快照目录名），再复制 untracked、回填 manifest。
    let id = match state
        .db
        .insert_checkpoint(instance_id, turn_seq, &git_ref, "[]")
    {
        Ok(id) => id,
        Err(e) => {
            log::warn!("[checkpoint] insert failed for {}: {}", instance_id, e);
            return;
        }
    };

    let snap_root = state.data_dir.join("checkpoints").join(id.to_string());
    let mut manifest = Vec::new();
    for rel in super::git::list_untracked(&cwd) {
        if exclude_path(&rel) {
            continue;
        }
        let src = cwd.join(&rel);
        let Ok(meta) = std::fs::metadata(&src) else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        let dst = snap_root.join(&rel);
        if let Some(parent) = dst.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if std::fs::copy(&src, &dst).is_ok() {
            manifest.push(json!({ "path": rel, "snapshot": dst.to_string_lossy() }));
        }
    }
    let manifest_str = serde_json::to_string(&manifest).unwrap_or_else(|_| "[]".to_string());
    if let Err(e) = state.db.update_checkpoint_manifest(id, &manifest_str) {
        log::warn!("[checkpoint] manifest update failed: {}", e);
    }

    // 保留最近 N 轮，清理超出的快照目录（尽力而为）。
    if let Ok(pruned) = state.db.prune_checkpoints(instance_id, KEEP_CHECKPOINTS) {
        for pid in pruned {
            let _ = std::fs::remove_dir_all(state.data_dir.join("checkpoints").join(pid.to_string()));
        }
    }
}

/// 撤回时调用：把工作区恢复到 `target_before_ms`（被撤回消息的时间戳）之前
/// 最近 checkpoint 的状态。无可恢复点（非 git / 无改动）时直接 Ok，仅消息撤回。
pub fn restore_checkpoint(
    state: &GatewayState,
    instance_id: &str,
    target_before_ms: i64,
) -> Result<(), String> {
    let Some(cwd) = state
        .session_manager
        .lock()
        .sessions
        .get(instance_id)
        .map(|s| s.cwd.clone())
    else {
        return Err("session not found".to_string());
    };
    let cwd = PathBuf::from(cwd);

    let Some(cp) = state.db.latest_checkpoint_before(instance_id, target_before_ms) else {
        return Ok(());
    };

    // ① tracked：工作区+暂存区重置到该轮前（git restore 只读历史 commit）。
    super::git::restore(&cwd, &cp.git_ref)?;

    // ② untracked：manifest 里的文件从快照还原（覆盖/重建）；其余（本轮
    //    新建、不在 manifest）删除。先 restore 再处理 untracked，顺序不可反
    //    ——restore 可能删除"本轮才变 tracked"的文件，随后由 manifest 重建。
    let entries: Vec<ManifestEntry> = serde_json::from_str(&cp.manifest).unwrap_or_default();
    let known: std::collections::HashSet<&str> =
        entries.iter().map(|e| e.path.as_str()).collect();
    for e in &entries {
        let target = cwd.join(&e.path);
        if let Some(parent) = target.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(err) = std::fs::copy(&e.snapshot, &target) {
            log::warn!("[checkpoint] restore {} failed: {}", e.path, err);
        }
    }
    for rel in super::git::list_untracked(&cwd) {
        if known.contains(rel.as_str()) {
            continue;
        }
        let p = cwd.join(&rel);
        let _ = if p.is_dir() {
            std::fs::remove_dir_all(&p)
        } else {
            std::fs::remove_file(&p)
        };
    }
    Ok(())
}

fn exclude_path(rel: &str) -> bool {
    rel.split('/').any(|c| EXCLUDE_DIRS.contains(&c))
}

/// 一条 manifest 记录：untracked 文件的相对路径 + 快照绝对路径。
#[derive(Debug, serde::Deserialize)]
struct ManifestEntry {
    path: String,
    snapshot: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP};
    use crate::gateway::db::Db;
    use crate::gateway::session_manager::SessionManager;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn git(repo: &std::path::Path, args: &[&str]) {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(repo)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {:?}: {}", args, String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn exclude_list_matches_components() {
        assert!(exclude_path("node_modules/pkg/index.js"));
        assert!(exclude_path("src/node_modules/x"));
        assert!(exclude_path("target/debug/app.exe"));
        assert!(!exclude_path("src/main.rs"));
        assert!(!exclude_path("build_notes.md"));
    }

    /// 端到端：turn1 agent 改 tracked + 建 untracked → checkpoint；turn2 再改再建；
    /// 撤回 turn2 → tracked 回到 turn1 末尾状态、turn2 新建的 untracked 被删、
    /// turn1 的 untracked 从快照还原。
    #[test]
    fn create_and_restore_checkpoint_end_to_end() {
        let repo = tempfile::tempdir().unwrap();
        let cwd = repo.path();
        git(cwd, &["init", "-q"]);
        git(cwd, &["config", "user.email", "t@t"]);
        git(cwd, &["config", "user.name", "t"]);
        std::fs::write(cwd.join("a.txt"), "v1\n").unwrap();
        git(cwd, &["add", "."]);
        git(cwd, &["commit", "-qm", "init"]);

        let data_dir = tempfile::tempdir().unwrap();
        let db = Db::open(data_dir.path()).unwrap();
        let sm = Arc::new(parking_lot::Mutex::new(SessionManager::new(None)));
        SessionManager::register_instance(&sm, "s1", cwd.to_str().unwrap(), 1);
        // turn_count 由 TurnEnd 事件驱动（与生产一致）
        let _ = sm.lock().on_event(&json!({"type": "turn_end"}), "s1");

        let state = Arc::new(GatewayState {
            event_tx: tokio::sync::broadcast::channel(EVENT_CHANNEL_CAP).0,
            inner: Arc::new(BrokerInner::default()),
            lan_ips: Arc::new(parking_lot::Mutex::new((std::time::Instant::now(), Vec::new()))),
            http_port: 0,
            pi_version: String::new(),
            pi_exe: std::path::PathBuf::new(),
            static_dir: std::path::PathBuf::new(),
            start_time: std::time::Instant::now(),
            db: db.clone(),
            data_dir: data_dir.path().to_path_buf(),
            chat_dist: std::path::PathBuf::new(),
            work_dist: None,
            connections: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            extension_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            ui_clients: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            session_manager: sm.clone(),
            agent_end_hook: Arc::new(parking_lot::Mutex::new(None)),
            mdns: Arc::new(parking_lot::Mutex::new(None)),
            workspace_base_dir: Arc::new(parking_lot::Mutex::new(std::path::PathBuf::new())),
            migrations: Arc::new(parking_lot::Mutex::new(crate::gateway::migrate::MigrationState::default())),
        });

        // ── turn 1：改 tracked a.txt→v2、新建 untracked agent_file.txt ──
        std::fs::write(cwd.join("a.txt"), "v2\n").unwrap();
        std::fs::write(cwd.join("agent_file.txt"), "created-in-turn1\n").unwrap();
        create_checkpoint(&state, "s1");
        std::thread::sleep(std::time::Duration::from_millis(5));
        let between = crate::gateway::now_epoch_ms();

        // ── turn 2：a.txt→v3、再建 untracked new2.txt、改 agent_file.txt ──
        let _ = sm.lock().on_event(&json!({"type": "turn_end"}), "s1");
        std::fs::write(cwd.join("a.txt"), "v3\n").unwrap();
        std::fs::write(cwd.join("new2.txt"), "created-in-turn2\n").unwrap();
        std::fs::write(cwd.join("agent_file.txt"), "modified-in-turn2\n").unwrap();
        create_checkpoint(&state, "s1");

        let rows = db.latest_checkpoint_before("s1", between);
        assert!(rows.is_some(), "checkpoint 应已落库");

        // ── 撤回 turn2 的消息 → 恢复到 turn1 末尾 ──
        restore_checkpoint(&state, "s1", between).unwrap();
        assert_eq!(
            std::fs::read_to_string(cwd.join("a.txt")).unwrap().trim_end(),
            "v2",
            "tracked 回到 turn1 末尾"
        );
        assert_eq!(
            std::fs::read_to_string(cwd.join("agent_file.txt")).unwrap(),
            "created-in-turn1\n",
            "turn1 的 untracked 从快照还原（turn2 的修改被回滚）"
        );
        assert!(
            !cwd.join("new2.txt").exists(),
            "turn2 新建的 untracked 被删除"
        );
    }
}
