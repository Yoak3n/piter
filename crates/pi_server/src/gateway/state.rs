//! GatewayState 结构体与生命周期方法，以及"项目→会话"树构建。
//!
//! 做什么：定义 gateway 的共享状态（事件通道、broker 句柄、DB、会话管理器等），
//! 提供 spawn / URL / kill_all / LAN IP 等生命周期操作，并基于数据库 + 运行时状态
//! 构建 sessions_list 的 ProjectGroup 树。
//! 不做什么：不绑定端口、不建 Router（那是 server.rs）；不消费 broker 事件
//! （那是 event_loop.rs / responses.rs）。
//! 依赖：broker（SpawnBuilder/EventTx）、session_manager、db、broadcast（kill_all 推送）。

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tokio::sync::mpsc;

use crate::broker::types::{BrokerInner, EventTx};

use super::{
    broadcast::push_sessions_list_to_clients, db::SessionRow, helper::discover_lan_ips, handlers,
    mdns::MdnsRegistration, project::list_projects, session_manager,
};

// ─── Gateway State ─────────────────────────────────────────────────────────

/// 一条 WS 客户端连接记录（分享页「连接客户端」列表数据源）。
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ClientConnection {
    pub id: u64,
    /// 前端类型（由 WS 端点 path 决定）：`work`（/work-ws）| `chat`（/chat-ws）|
    /// `ui`（/ws、/ui-ws 历史/管理兼容）。
    pub kind: String,
    /// 形态：`web` | `app` | `unknown`（由 UA 判定，仅展示辅助）。
    pub form: String,
    pub ip: String,
    pub user_agent: String,
    /// 连接建立时间（epoch ms）。
    pub connected_at_ms: i64,
}

/// Clone-able state passed into every axum handler via `State`.
#[derive(Clone)]
pub struct GatewayState {
    pub event_tx: EventTx,
    pub inner: Arc<BrokerInner>,
    /// Cached LAN IPs, lazily refreshed with a short TTL so addresses stay
    /// accurate after network changes (e.g. switching WiFi).
    pub lan_ips: Arc<parking_lot::Mutex<(std::time::Instant, Vec<String>)>>,
    pub http_port: u16,
    pub pi_version: String,
    pub pi_exe: PathBuf,
    pub static_dir: PathBuf,
    pub start_time: std::time::Instant,
    /// SQLite database for project/session/extension management.
    pub db: Arc<crate::gateway::db::Db>,
    /// App data dir（piter.db 所在目录）；checkpoint 快照存 `<data_dir>/checkpoints/`。
    pub data_dir: PathBuf,
    /// Chat SPA 静态目录（多 SPA fallback 分发；0.3.0 起与 work 分离）。
    pub chat_dist: PathBuf,
    /// Work SPA 静态目录（`/work`、`/workspaces/*` 分发；None = 未部署 work 前端）。
    pub work_dist: Option<PathBuf>,
    /// WS 客户端连接注册表（`/api/connections` + join/leave 广播）。
    pub connections: Arc<parking_lot::Mutex<HashMap<u64, ClientConnection>>>,
    /// Cached discovered extension candidates (global / per-project), filled
    /// at startup and refreshed in the background. DB state is not cached.
    pub extension_cache: Arc<parking_lot::RwLock<HashMap<String, Vec<super::project::ExtensionEntry>>>>,
    /// Connected UI WebSocket clients.
    pub ui_clients: Arc<parking_lot::Mutex<HashMap<u64, mpsc::UnboundedSender<String>>>>,
    /// Session manager for message tracking and idle lifecycle.
    pub session_manager: Arc<parking_lot::Mutex<session_manager::SessionManager>>,
    /// Agent 完成观察点：会话 agent_end 时回调 (instance_id, session_label)。
    /// 由桌面壳层注册（用于系统通知——托盘隐藏时前端 WS 不可达，系统通知只能走 Rust 侧）；
    /// web / headless 场景保持 None。
    pub agent_end_hook: Arc<parking_lot::Mutex<Option<Box<dyn Fn(&str, &str) + Send + Sync>>>>,
    /// mDNS 广播注册句柄（None = 未注册/失败；mDNS 是便利层，失败不阻塞 gateway）。
    pub mdns: Arc<parking_lot::Mutex<Option<MdnsRegistration>>>,
    /// 工作空间基目录（启动时解析：配置优先 → 安装目录 → data_dir 回退；
    /// PUT /api/workspaces/base-dir 时更新）。real_dir = `<base>/workspaces/<id>`。
    pub workspace_base_dir: Arc<parking_lot::Mutex<PathBuf>>,
    /// 基目录迁移队列与状态（migrate.rs；标记文件持久化于 data_dir）。
    pub migrations: Arc<parking_lot::Mutex<crate::gateway::migrate::MigrationState>>,
}

// ─── GatewayState lifecycle methods ────────────────────────────────────────
// start_gateway（端口绑定 + Router 构建 + 线程 spawn）见 server.rs。

impl GatewayState {
    /// Current workspace base dir（real_dir = `<base>/workspaces/<id>`）。
    pub fn workspace_base_dir(&self) -> PathBuf {
        self.workspace_base_dir.lock().clone()
    }

    /// 优雅注销 mDNS 广播（进程退出时调用；即使不调用，OS 退出也会释放组播 socket）。
    pub fn stop_mdns(&self) {
        if let Some(reg) = self.mdns.lock().take() {
            reg.stop();
        }
    }

    /// Clone the stdin sender for a running instance, if it exists.
    pub fn instance_stdin_tx(&self, instance_id: &str) -> Option<mpsc::UnboundedSender<String>> {
        self.inner
            .instances
            .lock()
            .get(instance_id)
            .and_then(|inst| inst.stdin_tx.clone())
    }

    /// 注册 agent_end 观察回调（桌面壳层用，发送系统通知）。回调运行在
    /// gateway 事件循环线程，必须快速返回、不可 panic。
    pub fn set_agent_end_hook<F>(&self, hook: F)
    where
        F: Fn(&str, &str) + Send + Sync + 'static,
    {
        *self.agent_end_hook.lock() = Some(Box::new(hook));
    }

    /// Start building a persistent pi process spawn.
    ///
    /// ```ignore
    /// let id = gw.spawn().cwd("/project").extensions(&exts).run()?;
    /// ```
    pub fn spawn(&self) -> crate::broker::process::SpawnBuilder {
        crate::broker::process::SpawnBuilder::new(
            self.inner.clone(),
            self.event_tx.clone(),
            self.pi_exe.clone(),
            self.static_dir.clone(),
            self.pi_version.clone(),
            true, // persistent
        )
    }

    /// Start building an ephemeral pi process spawn (no session).
    pub fn spawn_ephemeral(&self) -> crate::broker::process::SpawnBuilder {
        crate::broker::process::SpawnBuilder::new(
            self.inner.clone(),
            self.event_tx.clone(),
            self.pi_exe.clone(),
            self.static_dir.clone(),
            self.pi_version.clone(),
            false, // ephemeral
        )
    }

    pub fn ws_url(&self) -> String {
        format!("ws://127.0.0.1:{}/ws", self.http_port)
    }

    pub fn http_url(&self) -> String {
        format!("http://127.0.0.1:{}/", self.http_port)
    }

    pub fn port(&self) -> u16 {
        self.http_port
    }

    /// Current LAN IPs, rediscovered at most once per TTL so the addresses
    /// stay fresh after network changes without spawning a subprocess on
    /// every call.
    pub fn current_lan_ips(&self) -> Vec<String> {
        const LAN_IPS_TTL: std::time::Duration = std::time::Duration::from_secs(2);
        let mut cache = self.lan_ips.lock();
        if cache.0.elapsed() >= LAN_IPS_TTL {
            cache.1 = discover_lan_ips();
            cache.0 = std::time::Instant::now();
        }
        cache.1.clone()
    }

    pub fn lan_urls(&self) -> Vec<String> {
        self.current_lan_ips()
            .iter()
            .map(|ip| {
                format!(
                    "http://{}:{}/chat?brokerWs=ws://{}:{}/chat-ws",
                    ip, self.http_port, ip, self.http_port
                )
            })
            .collect()
    }

    pub fn uptime_secs(&self) -> u64 {
        self.start_time.elapsed().as_secs()
    }

    /// Kill all pi instances.
    pub fn kill_all(&self) {
        use std::sync::atomic::Ordering;
        let mut instances = self.inner.instances.lock();
        for (_, mut inst) in instances.drain() {
            inst.running.store(false, Ordering::SeqCst);
            inst.killed.store(true, Ordering::SeqCst);
            let _ = inst.child.kill();
        }
        drop(instances);
        log::info!("[gateway] all pi instances stopped");

        // Mark all tracked sessions unloaded (processes are gone) and push
        // the updated sessions list so clients immediately see the stopped state.
        {
            let mut mgr = self.session_manager.lock();
            let ids: Vec<String> = mgr.sessions.keys().cloned().collect();
            if !ids.is_empty() {
                mgr.mark_unloaded(&ids);
            }
        }
        push_sessions_list_to_clients(self);
    }

    pub fn has_active_processes(&self) -> bool {
        !self.inner.instances.lock().is_empty()
    }
}

/// Lightweight runtime info from session manager for enriching project tree.
#[derive(Clone)]
struct RuntimeSessionInfo {
    state: String,
    model: Option<String>,
    model_provider: Option<String>,
    thinking_level: Option<String>,
    message_count: u32,
    message_seq: u64,
    session_name: Option<String>,
    last_active_epoch: u64,
}

/// Build project-session tree from database + session file metadata + runtime state.
pub fn build_project_session_tree(state: &GatewayState) -> Vec<handlers::ProjectGroup> {
    use handlers::{ProjectGroup, SessionInfo};

    // Build lookup: session_file_path → (instance_id, state_info) from session manager
    let mgr = state.session_manager.lock();
    let mut runtime_by_iid: HashMap<String, RuntimeSessionInfo> = HashMap::new();
    for session in mgr.sessions.values() {
        let info = RuntimeSessionInfo {
            state: match &session.activity {
                session_manager::SessionActivity::Idle => "idle".to_string(),
                session_manager::SessionActivity::Busy => "busy".to_string(),
                session_manager::SessionActivity::WaitingReview => "waiting_review".to_string(),
                session_manager::SessionActivity::Unloaded => "unloaded".to_string(),
            },
            model: session.pi_state.as_ref().and_then(|p| p.model_id.clone()),
            model_provider: session.pi_state.as_ref().and_then(|p| p.model_provider.clone()),
            thinking_level: session.pi_state.as_ref().and_then(|p| p.thinking_level.clone()),
            message_count: session.messages.len() as u32,
            message_seq: session.message_seq,
            session_name: session.session_name.clone(),
            last_active_epoch: session.last_active_epoch,
        };
        runtime_by_iid.insert(session.instance_id.clone(), info);
    }
    drop(mgr);

    // Single DB query for all sessions (avoid O(n²))
    let all_db_sessions = state.db.all_sessions();
    let db_by_iid: HashMap<String, SessionRow> = all_db_sessions
        .into_iter()
        .map(|s| (s.instance_id.clone(), s))
        .collect();

    let db_projects = list_projects(&state.db, true);

    let mut result: Vec<ProjectGroup> = Vec::new();
    let mut archived_result: Vec<ProjectGroup> = Vec::new();

    for proj in &db_projects {
        let instance_ids = state.db.get_project_sessions(&proj.id);
        let mut sessions: Vec<SessionInfo> = Vec::new();

        for iid in &instance_ids {
            let rt = runtime_by_iid.get(iid);
            let db_row = db_by_iid.get(iid);

            // Label: runtime auto-title > DB name > instance id fallback
            let label = rt
                .and_then(|r| r.session_name.clone())
                .or_else(|| db_row.and_then(|r| r.name.clone()))
                .unwrap_or_else(|| iid.chars().take(8).collect());

            let state_str = rt
                .map(|r| r.state.clone())
                .unwrap_or_else(|| "unloaded".to_string());

            sessions.push(SessionInfo {
                id: iid.clone(),
                label,
                created_at: String::new(),
                file_path: db_row
                    .and_then(|r| r.session_path.clone())
                    .unwrap_or_default(),
                updated_at: rt.map(|r| r.last_active_epoch).unwrap_or_else(|| {
                    // Parse DB created_at RFC3339 string to epoch
                    db_row.and_then(|r| chrono::DateTime::parse_from_rfc3339(&r.created_at).ok())
                        .map(|dt| dt.timestamp() as u64)
                        .unwrap_or(0)
                }),
                preview: String::new(),
                cwd: proj.cwd.clone(),
                instance_id: Some(iid.clone()),
                state: Some(state_str),
                // Runtime state wins; fall back to the persisted DB model so a
                // session's own model survives a gateway restart.
                model: rt
                    .and_then(|r| r.model.clone())
                    .or_else(|| db_row.and_then(|r| r.model_id.clone())),
                model_provider: rt
                    .and_then(|r| r.model_provider.clone())
                    .or_else(|| db_row.and_then(|r| r.model_provider.clone())),
                thinking_level: rt.and_then(|r| r.thinking_level.clone()),
                message_count: rt.map(|r| r.message_count).unwrap_or(0),
                message_seq: rt.map(|r| r.message_seq).unwrap_or(0),
                pinned: db_row.map(|r| r.pinned).unwrap_or(0),
            });
        }

        // Pinned sessions stay at the top of their project; the rest keep the
        // last-active order (matches the project-level pinned sort).
        sessions.sort_by(|a, b| {
            b.pinned
                .cmp(&a.pinned)
                .then_with(|| b.updated_at.cmp(&a.updated_at))
        });

        let group = ProjectGroup {
            path: proj.cwd.clone(),
            name: proj.name.clone(),
            id: Some(proj.id.clone()),
            project_type: proj.project_type.clone(),
            pinned: proj.pinned,
            archived: proj.archived,
            sessions,
        };
        if proj.archived {
            archived_result.push(group);
        } else {
            result.push(group);
        }
    }

    // Orphaned sessions (in DB but no project)
    let all_linked: std::collections::HashSet<String> = result
        .iter()
        .flat_map(|p| p.sessions.iter().filter_map(|s| s.instance_id.clone()))
        .collect();

    let mut orphans: Vec<SessionInfo> = db_by_iid
        .values()
        .filter(|s| s.project_id.is_none() && !all_linked.contains(&s.instance_id))
        .map(|s| {
            let rt = runtime_by_iid.get(&s.instance_id);
            let label = s.name.clone()
                .or_else(|| rt.and_then(|r| r.session_name.clone()))
                .unwrap_or_else(|| s.instance_id.chars().take(8).collect());
            SessionInfo {
                id: s.instance_id.clone(),
                label,
                created_at: String::new(),
                file_path: s.session_path.clone().unwrap_or_default(),
                updated_at: rt.map(|r| r.message_count as u64).unwrap_or(0),
                preview: String::new(),
                cwd: s.cwd.clone(),
                instance_id: Some(s.instance_id.clone()),
                state: Some(rt.map(|r| r.state.clone()).unwrap_or_else(|| "unloaded".to_string())),
                model: rt.and_then(|r| r.model.clone()).or_else(|| s.model_id.clone()),
                model_provider: rt
                    .and_then(|r| r.model_provider.clone())
                    .or_else(|| s.model_provider.clone()),
                thinking_level: None,
                message_count: rt.map(|r| r.message_count).unwrap_or(0),
                message_seq: rt.map(|r| r.message_seq).unwrap_or(0),
                pinned: s.pinned,
            }
        })
        .collect();

    // Same ordering as project sessions: pinned orphans stay at the top.
    orphans.sort_by(|a, b| {
        b.pinned
            .cmp(&a.pinned)
            .then_with(|| b.updated_at.cmp(&a.updated_at))
    });

    if !orphans.is_empty() {
        result.push(ProjectGroup {
            path: String::new(),
            name: "Other".to_string(),
            id: None,
            project_type: String::new(),
            pinned: 0,
            archived: false,
            sessions: orphans,
        });
    }

    // Archived projects stay visible but are grouped at the very end under the
    // "Archive" section by the frontend, so they can be restored anytime.
    result.extend(archived_result);

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::broker::types::{BrokerInner, EVENT_CHANNEL_CAP};
    use crate::gateway::db::Db;
    use crate::gateway::session_manager::SessionManager;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn test_state(db: Arc<Db>) -> Arc<GatewayState> {
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
            data_dir: std::path::PathBuf::new(),
            chat_dist: std::path::PathBuf::new(),
            work_dist: None,
            connections: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            extension_cache: Arc::new(parking_lot::RwLock::new(HashMap::new())),
            ui_clients: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            session_manager: Arc::new(parking_lot::Mutex::new(SessionManager::new(None))),
            agent_end_hook: Arc::new(parking_lot::Mutex::new(None)),
            mdns: Arc::new(parking_lot::Mutex::new(None)),
            workspace_base_dir: Arc::new(parking_lot::Mutex::new(std::path::PathBuf::new())),
            migrations: Arc::new(parking_lot::Mutex::new(crate::gateway::migrate::MigrationState::default())),
        })
    }

    /// 置顶会话在所属项目内排最前；取消后恢复 updated_at 排序（回归：DB 持久化
    /// + 排序两处都能工作）。created_at 为秒级精度，注册间隔 1.1s 保证可判定序。
    #[test]
    fn pinned_session_sorts_first_within_project() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.create_project("proj1", "Project One", "/tmp/proj").unwrap();
        db.register_session("s1", "/tmp/proj", Some("proj1")).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(1100));
        db.register_session("s2", "/tmp/proj", Some("proj1")).unwrap();

        let state = test_state(db.clone());
        let ids = |state: &Arc<GatewayState>| -> Vec<String> {
            build_project_session_tree(state)
                .into_iter()
                .find(|g| g.id.as_deref() == Some("proj1"))
                .map(|g| {
                    g.sessions
                        .into_iter()
                        .map(|s| s.instance_id.unwrap())
                        .collect()
                })
                .unwrap()
        };

        // Base order: newest activity first.
        assert_eq!(ids(&state), vec!["s2", "s1"]);

        // Pin the older session → it jumps to the top of its project.
        db.set_session_pinned("s1", 1).unwrap();
        assert_eq!(ids(&state), vec!["s1", "s2"]);

        // Unpin → back to updated_at order.
        db.set_session_pinned("s1", 0).unwrap();
        assert_eq!(ids(&state), vec!["s2", "s1"]);
    }
}
