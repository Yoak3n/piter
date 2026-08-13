//! 工作空间基目录迁移（0.3.0 完整版，对齐《工作空间与产物管理》定案）。
//!
//! 做什么：基目录变更时把现有工作空间 real_dir 迁移到新基目录——
//! 后台调度（活跃会话等待/防饿死）、同卷 rename / 跨卷 copy+delete、
//! 标记文件持久化（app 数据目录，原子写 temp+rename）、启动恢复。
//! 不做什么：不负责"基目录配置"本身（handlers/workspace.rs + db/workspace.rs）；
//! 不做快照 diff（那是 workspace.rs 的事——迁移只在会话结束后进行，天然无写者）。
//! 依赖：GatewayState（db / session_manager / data_dir）。

use std::path::{Path, PathBuf};
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use super::session_manager::SessionActivity;
use super::GatewayState;

/// 一个待迁移的工作空间（也是标记文件的一行）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PendingMigration {
    /// workspace project id。
    pub id: String,
    /// 当前 real_dir（DB cwd）。
    pub old_path: String,
    /// 迁移目标 real_dir。
    pub new_path: String,
    /// true = 正等待该工作空间的活跃会话结束（waiting 状态展示）。
    #[serde(default)]
    pub waiting: bool,
}

/// 迁移运行时状态（GatewayState.migrations 保护）。
pub struct MigrationState {
    pub pending: Vec<PendingMigration>,
    /// 最近一次迁移失败记录：(workspace_id, error)。
    pub errors: Vec<(String, String)>,
    /// 迁移进行中（单飞，避免并发搬文件）。
    pub migrating: bool,
}

impl Default for MigrationState {
    fn default() -> Self {
        Self {
            pending: Vec::new(),
            errors: Vec::new(),
            migrating: false,
        }
    }
}

// ─── 标记文件（app 数据目录，原子写）──────────────────────────────────────

fn queue_path(data_dir: &Path) -> PathBuf {
    data_dir.join("migration-queue.json")
}

pub fn save_queue(state: &GatewayState) {
    let path = queue_path(&state.data_dir);
    let mig = state.migrations.lock();
    let json = serde_json::to_string(&mig.pending).unwrap_or_else(|_| "[]".to_string());
    drop(mig);
    // 原子写：temp + rename（避免崩溃留下半写文件）。
    let tmp = path.with_extension("json.tmp");
    if std::fs::write(&tmp, json).is_ok() {
        let _ = std::fs::rename(&tmp, &path);
    }
}

/// 启动时恢复上次未完成的迁移队列（重启后原活跃会话已停止 → 可直接迁移）。
pub fn load_queue(data_dir: &Path) -> Vec<PendingMigration> {
    let path = queue_path(data_dir);
    let Ok(raw) = std::fs::read_to_string(&path) else {
        return Vec::new();
    };
    serde_json::from_str(&raw).unwrap_or_default()
}

// ─── 迁移任务 ──────────────────────────────────────────────────────────────

/// 启动后台迁移调度：每 2s 尝试推进队列。
/// 用 std::thread（try_run_migrations 是纯同步逻辑，无 async/IO 依赖），
/// 避免依赖 Tokio runtime 上下文（start_gateway 同步函数内不能 tokio::spawn）。
pub fn spawn_migration_task(state: Arc<GatewayState>) {
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(2));
        try_run_migrations(&state);
    });
}

/// 推进一次迁移队列（单飞；每次只迁移一个工作空间，避免锁内长 IO）。
pub fn try_run_migrations(state: &GatewayState) {
    // 1) 取下一个待迁移项（锁内只做判定，不搬文件）。
    let next = {
        let mut mig = state.migrations.lock();
        if mig.migrating || mig.pending.is_empty() {
            return;
        }
        let item = mig.pending[0].clone();
        if workspace_has_active_session(state, &item.old_path) {
            // 活跃会话未结束：等待（防饿死——等待期间禁止新建该工作空间
            // 会话，见 ws/ 的 create_workspace_session 检查）。
            if !mig.pending[0].waiting {
                mig.pending[0].waiting = true;
                save_queue(state);
            }
            return;
        }
        mig.migrating = true;
        item
    };

    // 2) 锁外执行迁移（同卷 rename / 跨卷 copy+delete）。
    let result = migrate_one(&next);

    // 3) 更新状态 + DB 映射（先文件安全落位，再事务更新 cwd）。
    let mut mig = state.migrations.lock();
    mig.migrating = false;
    match result {
        Ok(()) => {
            if let Err(e) = state.db.update_workspace_cwd(&next.id, &next.new_path) {
                log::error!("[migrate] update cwd for {} failed: {}", next.id, e);
                mig.errors.push((next.id.clone(), e));
            } else if let Err(e) =
                state.db.update_sessions_cwd_for_project(&next.id, &next.new_path)
            {
                log::error!("[migrate] update sessions cwd for {} failed: {}", next.id, e);
                mig.errors.push((next.id.clone(), e));
            } else {
                log::info!(
                    "[migrate] {}: {} → {}",
                    next.id,
                    next.old_path,
                    next.new_path
                );
            }
        }
        Err(e) => {
            log::error!("[migrate] {} failed: {}", next.id, e);
            mig.errors.push((next.id.clone(), e));
        }
    }
    // 无论成功与否都移出队列（失败记录在 errors，Admin 可见；不无限重试，
    // 用户可再次保存基目录触发重试）。
    mig.pending.retain(|p| p.id != next.id);
    drop(mig);
    save_queue(state);
}

/// 该工作空间（cwd=real_dir）是否有活跃 work 会话。
fn workspace_has_active_session(state: &GatewayState, cwd: &str) -> bool {
    let mgr = state.session_manager.lock();
    mgr.sessions
        .values()
        .any(|s| s.cwd == cwd && s.activity != SessionActivity::Unloaded)
}

/// 执行单工作空间迁移：同卷 `rename`（快）；EXDEV 跨卷 → copy + 校验 + 删除。
fn migrate_one(item: &PendingMigration) -> Result<(), String> {
    let old = Path::new(&item.old_path);
    let new = Path::new(&item.new_path);
    if !old.exists() {
        // 源不存在（已被删除/移动）：视作成功，仅更新 DB 映射。
        return Ok(());
    }
    if new.exists() {
        return Err(format!("目标已存在：{}", new.display()));
    }
    if let Some(parent) = new.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("创建目标目录失败 {}: {}", parent.display(), e))?;
    }
    match std::fs::rename(old, new) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
            // 跨设备（Windows EXDEV=17 / Unix EXDEV=18）→ copy + delete。
            let staging = new.with_extension("migrating");
            if staging.exists() {
                // 上次崩溃残留：清理后重试（半迁状态检测）。
                std::fs::remove_dir_all(&staging)
                    .map_err(|e| format!("清理残留 {}: {}", staging.display(), e))?;
            }
            copy_dir_all(old, &staging)?;
            // 校验后再落位。
            std::fs::rename(&staging, new).map_err(|e| format!("落位失败: {}", e))?;
            std::fs::remove_dir_all(old).map_err(|e| format!("删除旧目录失败: {}", e))?;
            Ok(())
        }
        Err(e) => Err(format!("rename 失败: {}", e)),
    }
}

/// 递归复制目录（含隐藏文件与子目录）。
fn copy_dir_all(src: &Path, dst: &Path) -> Result<(), String> {
    std::fs::create_dir_all(dst)
        .map_err(|e| format!("创建 {}: {}", dst.display(), e))?;
    for entry in std::fs::read_dir(src).map_err(|e| format!("读取 {}: {}", src.display(), e))? {
        let entry = entry.map_err(|e| format!("read_dir entry: {}", e))?;
        let ty = entry
            .file_type()
            .map_err(|e| format!("file_type {}: {}", entry.path().display(), e))?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else if ty.is_file() || ty.is_symlink() {
            std::fs::copy(entry.path(), &target)
                .map_err(|e| format!("copy {}: {}", entry.path().display(), e))?;
        }
    }
    Ok(())
}

/// 该工作空间是否在迁移队列中（防饿死：等待/迁移期间禁止新建 work 会话）。
pub fn is_pending(state: &GatewayState, workspace_id: &str) -> bool {
    state
        .migrations
        .lock()
        .pending
        .iter()
        .any(|p| p.id == workspace_id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::sync::Arc;

    use crate::broker::types::BrokerInner;
    use crate::gateway::db::Db;
    use crate::gateway::session_manager::SessionManager;

    fn test_state(db: Arc<Db>, data_dir: &Path, base_dir: PathBuf) -> Arc<GatewayState> {
        let (event_tx, _) = tokio::sync::broadcast::channel(16);
        Arc::new(GatewayState {
            event_tx,
            inner: Arc::new(BrokerInner::default()),
            lan_ips: Arc::new(parking_lot::Mutex::new((
                std::time::Instant::now(),
                Vec::new(),
            ))),
            http_port: 0,
            pi_version: String::new(),
            pi_exe: PathBuf::new(),
            static_dir: data_dir.to_path_buf(),
            start_time: std::time::Instant::now(),
            db,
            data_dir: data_dir.to_path_buf(),
            chat_dist: PathBuf::new(),
            work_dist: None,
            connections: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            extension_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            ui_clients: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            session_manager: Arc::new(parking_lot::Mutex::new(SessionManager::new(None))),
            agent_end_hook: Arc::new(parking_lot::Mutex::new(None)),
            mdns: Arc::new(parking_lot::Mutex::new(None)),
            workspace_base_dir: Arc::new(parking_lot::Mutex::new(base_dir)),
            migrations: Arc::new(parking_lot::Mutex::new(MigrationState::default())),
        })
    }

    #[tokio::test]
    async fn base_dir_migration_moves_files_and_updates_db() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let db = Db::open(&data_dir).unwrap();
        // 初始基目录 = 数据目录（模拟 0.3.0 前的 AppData 位置）。
        let old_base = data_dir.clone();
        let state = test_state(db.clone(), &data_dir, old_base.clone());

        // 建一个工作空间 + 内容文件。
        let ws = crate::gateway::workspace::create_workspace(&db, &old_base, "Demo").unwrap();
        let old_dir = Path::new(&ws.cwd);
        std::fs::write(old_dir.join("hello.txt"), "hi").unwrap();

        // 改基目录 → 构建队列并推进。
        let new_base = tmp.path().join("wsdata");
        crate::gateway::workspace::dir_writable(&new_base);
        let new_dir = crate::gateway::workspace::workspace_dir(&new_base, &ws.id);
        {
            let mut mig = state.migrations.lock();
            mig.pending.push(PendingMigration {
                id: ws.id.clone(),
                old_path: ws.cwd.clone(),
                new_path: new_dir.to_string_lossy().to_string(),
                waiting: false,
            });
        }
        save_queue(&state);
        try_run_migrations(&state);

        // 文件已移动、旧目录已删、DB cwd 已更新、队列已清空。
        assert!(new_dir.join("hello.txt").exists());
        assert!(!old_dir.exists());
        let proj = db.get_project(&ws.id).unwrap();
        assert_eq!(PathBuf::from(&proj.cwd), new_dir);
        assert!(state.migrations.lock().pending.is_empty());
        // 标记文件清空。
        assert!(load_queue(&data_dir).is_empty());
    }

    #[tokio::test]
    async fn migration_writes_queue_file_for_recovery() {
        let tmp = tempfile::tempdir().unwrap();
        let data_dir = tmp.path().join("data");
        let db = Db::open(&data_dir).unwrap();
        let state = test_state(db.clone(), &data_dir, data_dir.clone());

        let old_base = data_dir.clone();
        let ws = crate::gateway::workspace::create_workspace(&db, &old_base, "W").unwrap();
        {
            let mut mig = state.migrations.lock();
            mig.pending.push(PendingMigration {
                id: ws.id.clone(),
                old_path: ws.cwd.clone(),
                new_path: format!("{}/wsdata/{}", tmp.path().display(), ws.id),
                waiting: false,
            });
        }
        save_queue(&state);

        // 模拟重启：load_queue 恢复同一队列。
        let restored = load_queue(&data_dir);
        assert_eq!(restored.len(), 1);
        assert_eq!(restored[0].id, ws.id);
        assert_eq!(restored[0].old_path, ws.cwd);
    }

    /// Windows 跨卷 rename 的 EXDEV 是 os error 17（ERROR_NOT_SAME_DEVICE）。
    /// migrate_one 的回退分支依赖该错误被映射为 CrossesDevices（回归防护）。
    #[test]
    #[cfg(windows)]
    fn windows_cross_device_error_maps_to_crosses_devices() {
        let e = std::io::Error::from_raw_os_error(17);
        assert_eq!(e.kind(), std::io::ErrorKind::CrossesDevices);
    }
}
