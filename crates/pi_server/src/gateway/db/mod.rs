//! SQLite 数据层：连接管理 + 建表迁移 + 按领域分文件的 CRUD。
//!
//! 做什么：`Db::open` 打开/创建 `<piter_data_dir>/piter.db`，执行幂等建表与增量
//! 迁移（projects/sessions/扩展表/搜索索引/预算与 LAN 鉴权配置），并在打开时自动
//! 关联孤儿会话。对外 API（`gateway::db::Db` 及其方法、各 Row/Config 结构体）经
//! 本文件重导出，调用方零改动。
//! 不做什么：不承载领域查询逻辑——会话/项目/扩展/搜索/设置 CRUD 分别位于
//! sessions.rs / projects.rs / extensions.rs / search.rs / settings.rs。
//! 依赖：rusqlite；上层（state.rs / handlers / src-tauri）通过 `gateway::db` 使用。

pub mod checkpoints;
pub mod extensions;
pub mod projects;
pub mod search;
pub mod sessions;
pub mod settings;
pub mod workspace;

pub use checkpoints::*;
pub use projects::*;
pub use search::*;
pub use sessions::*;
pub use settings::*;
pub use workspace::*;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

// ─── Database Handle ────────────────────────────────────────────────────────

pub struct Db {
    conn: Mutex<Connection>,
    /// FTS5 availability (compile-time flag; false → search falls back to LIKE).
    fts_available: AtomicBool,
}

impl Db {
    /// Open (or create) the database inside `data_dir`.
    pub fn open(data_dir: &Path) -> Result<Arc<Self>, String> {
        let path = db_path(data_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("create db dir: {}", e))?;
        }
        let conn = Connection::open(&path)
            .map_err(|e| format!("open db {}: {}", path.display(), e))?;

        conn.execute_batch("PRAGMA foreign_keys = ON;")
            .map_err(|e| format!("pragma: {}", e))?;

        let db = Arc::new(Self {
            conn: Mutex::new(conn),
            fts_available: AtomicBool::new(false),
        });
        db.migrate()?;
        db.auto_link_sessions();
        Ok(db)
    }

    /// Link orphan sessions (project_id IS NULL) to projects by cwd match.
    /// Runs once at DB open to fix any inconsistent state.
    fn auto_link_sessions(&self) {
        let orphans = self.all_sessions().into_iter()
            .filter(|s| s.project_id.is_none())
            .collect::<Vec<_>>();
        if orphans.is_empty() {
            return;
        }
        let projects = self.list_projects(true); // include archived
        for session in &orphans {
            if let Some(proj) = projects.iter().find(|p| p.cwd == session.cwd) {
                let _ = self.register_session(&session.instance_id, &session.cwd, Some(&proj.id));
                log::info!("[db] auto-linked session {} to project '{}' (cwd={})",
                    session.instance_id, proj.name, session.cwd);
            }
        }
    }

    fn migrate(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();

        // Create tables (idempotent)
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS projects (
                id          TEXT PRIMARY KEY,
                name        TEXT NOT NULL,
                cwd         TEXT NOT NULL,
                type        TEXT NOT NULL DEFAULT 'normal',
                pinned      INTEGER NOT NULL DEFAULT 0,
                archived    INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS project_added_extensions (
                project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                extension_name  TEXT NOT NULL,
                extension_path  TEXT,
                PRIMARY KEY (project_id, extension_name)
            );

            CREATE TABLE IF NOT EXISTS sessions (
                instance_id   TEXT PRIMARY KEY,
                session_path  TEXT,
                project_id    TEXT REFERENCES projects(id) ON DELETE SET NULL,
                cwd           TEXT NOT NULL,
                name          TEXT,
                pinned        INTEGER NOT NULL DEFAULT 0,
                created_at    TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS global_extensions (
                extension_name  TEXT PRIMARY KEY,
                extension_path  TEXT
            );

            CREATE TABLE IF NOT EXISTS project_excluded_extensions (
                project_id      TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                extension_name  TEXT NOT NULL,
                PRIMARY KEY (project_id, extension_name)
            );

            -- Monthly budget (0.2.0 P3): a single user-configured row. Money is
            -- stored in cents so it never suffers float rounding; reset_day is
            -- the day of month a new cycle starts (clamped to the month length).
            CREATE TABLE IF NOT EXISTS budget_config (
                id                      INTEGER PRIMARY KEY CHECK (id = 1),
                monthly_budget_cents    INTEGER NOT NULL DEFAULT 0,
                reset_day               INTEGER NOT NULL DEFAULT 1,
                enabled                 INTEGER NOT NULL DEFAULT 0
            );
            INSERT OR IGNORE INTO budget_config (id) VALUES (1);

            -- LAN auth (0.2.0 P3): single config row + per-device tokens.
            -- The PIN is stored salted-hashed only (never plaintext); tokens
            -- are random per authorized device, expiring after 30 days.
            CREATE TABLE IF NOT EXISTS lan_auth_config (
                id          INTEGER PRIMARY KEY CHECK (id = 1),
                enabled     INTEGER NOT NULL DEFAULT 0,
                pin_hash    TEXT NOT NULL DEFAULT '',
                pin_salt    TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL
            );
            INSERT OR IGNORE INTO lan_auth_config (id) VALUES (1);
            CREATE TABLE IF NOT EXISTS lan_tokens (
                token       TEXT PRIMARY KEY,
                created_at  TEXT NOT NULL,
                expires_at  TEXT NOT NULL
            );

            -- File rollback checkpoints (消息撤回 0.2.0 P3): one row per
            -- completed agent turn in a git repo. `git_ref` is the stash-create
            -- commit-ish usable with `git restore --source`; `manifest` is a
            -- JSON array of {path, snapshot} for untracked files whose content
            -- was copied into <data_dir>/checkpoints/<id>/.
            CREATE TABLE IF NOT EXISTS checkpoints (
                id            INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id    TEXT NOT NULL,
                turn_seq      INTEGER NOT NULL,
                git_ref       TEXT NOT NULL,
                manifest      TEXT NOT NULL DEFAULT '[]',
                created_at_ms INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_checkpoints_session
                ON checkpoints(session_id, created_at_ms);

            -- Workspaces (0.3.0): artifacts = per-turn change sets collected
            -- by snapshot diff; workspace_snapshots = per-session latest tree
            -- (overwrite-only, diff baseline — does not grow unbounded).
            CREATE TABLE IF NOT EXISTS artifacts (
                id          TEXT PRIMARY KEY,
                project_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                session_id  TEXT NOT NULL,
                turn_id     INTEGER NOT NULL,
                rel_path    TEXT NOT NULL,
                op          TEXT NOT NULL,
                size        INTEGER NOT NULL DEFAULT 0,
                source      TEXT NOT NULL DEFAULT 'snapshot',
                deliverable INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_artifacts_project
                ON artifacts(project_id, turn_id);
            CREATE TABLE IF NOT EXISTS workspace_snapshots (
                session_id  TEXT PRIMARY KEY,
                tree_json   TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );
            -- Manually-marked deliverable paths (mark-deliverable), persistent
            -- across turns. Artifact rows also carry a per-turn `deliverable`
            -- computed at diff time (output/ ∪ marks).
            CREATE TABLE IF NOT EXISTS workspace_deliverable_marks (
                workspace_id  TEXT NOT NULL REFERENCES projects(id) ON DELETE CASCADE,
                rel_path      TEXT NOT NULL,
                updated_at    TEXT NOT NULL,
                PRIMARY KEY (workspace_id, rel_path)
            );

            -- Workspace base dir (0.3.0): single config row. '' = not configured
            -- → default install dir; Admin may point it at a data disk. When it
            -- changes, existing workspaces are migrated (gateway/migrate.rs).
            CREATE TABLE IF NOT EXISTS workspace_config (
                id          INTEGER PRIMARY KEY CHECK (id = 1),
                base_dir    TEXT NOT NULL DEFAULT '',
                updated_at  TEXT NOT NULL
            );
            INSERT OR IGNORE INTO workspace_config (id) VALUES (1);
            ",
        )
        .map_err(|e| format!("migrate: {}", e))?;

        // Additive migrations: persist the per-instance model (id + provider)
        // so each session can restore its own model after a restart.
        let cols: Vec<String> = conn
            .prepare("PRAGMA table_info(sessions)")
            .map_err(|e| format!("migrate pragma: {}", e))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("migrate pragma rows: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        if !cols.iter().any(|c| c == "model_id") {
            conn.execute("ALTER TABLE sessions ADD COLUMN model_id TEXT", [])
                .map_err(|e| format!("migrate add model_id: {}", e))?;
        }
        if !cols.iter().any(|c| c == "model_provider") {
            conn.execute("ALTER TABLE sessions ADD COLUMN model_provider TEXT", [])
                .map_err(|e| format!("migrate add model_provider: {}", e))?;
        }
        if !cols.iter().any(|c| c == "pinned") {
            conn.execute(
                "ALTER TABLE sessions ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| format!("migrate add pinned: {}", e))?;
        }

        // Workspace write-boundary mode (0.3.0): ask|allow|deny per workspace.
        let proj_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(projects)")
            .map_err(|e| format!("migrate projects pragma: {}", e))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("migrate projects pragma rows: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        if !proj_cols.iter().any(|c| c == "mode") {
            conn.execute(
                "ALTER TABLE projects ADD COLUMN mode TEXT NOT NULL DEFAULT 'ask'",
                [],
            )
            .map_err(|e| format!("migrate add projects.mode: {}", e))?;
        }

        // Workspace artifact line stats (0.3.1): per-turn added/deleted lines
        // (+N −M), computed by snapshot diff. Additive for existing DBs.
        let art_cols: Vec<String> = conn
            .prepare("PRAGMA table_info(artifacts)")
            .map_err(|e| format!("migrate artifacts pragma: {}", e))?
            .query_map([], |row| row.get::<_, String>(1))
            .map_err(|e| format!("migrate artifacts pragma rows: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        if !art_cols.iter().any(|c| c == "lines_added") {
            conn.execute(
                "ALTER TABLE artifacts ADD COLUMN lines_added INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| format!("migrate add artifacts.lines_added: {}", e))?;
        }
        if !art_cols.iter().any(|c| c == "lines_deleted") {
            conn.execute(
                "ALTER TABLE artifacts ADD COLUMN lines_deleted INTEGER NOT NULL DEFAULT 0",
                [],
            )
            .map_err(|e| format!("migrate add artifacts.lines_deleted: {}", e))?;
        }

        // ── Cross-session search index (0.2.0) ──────────────────────────
        // session_messages holds the indexed rows; the FTS5 virtual table is
        // best-effort over it (trigram tokenizer → CJK substring matching).
        // The index is kept fresh by `search::index_if_stale` (file-mtime
        // comparison) and cleaned up when a session is deleted.
        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS session_messages (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                session_id  TEXT NOT NULL,
                entry_id    TEXT,
                role        TEXT NOT NULL,
                content     TEXT NOT NULL,
                timestamp   INTEGER,
                file_mtime  TEXT NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_session_messages_session
                ON session_messages(session_id);
            CREATE TABLE IF NOT EXISTS search_index_state (
                session_id  TEXT PRIMARY KEY,
                file_mtime  TEXT NOT NULL,
                indexed_at  TEXT NOT NULL
            );
            ",
        )
        .map_err(|e| format!("migrate search tables: {}", e))?;

        // FTS5 with external content: the app only ever writes to
        // session_messages; triggers keep the FTS index in sync. If FTS5 is
        // compiled out, creation fails and search degrades to LIKE.
        let fts_ok = conn
            .execute_batch(
                "
                CREATE VIRTUAL TABLE IF NOT EXISTS session_messages_fts USING fts5(
                    content, role, session_id UNINDEXED, entry_id UNINDEXED, timestamp UNINDEXED,
                    content='session_messages', content_rowid='id', tokenize='trigram'
                );
                CREATE TRIGGER IF NOT EXISTS session_messages_ai AFTER INSERT ON session_messages BEGIN
                    INSERT INTO session_messages_fts(rowid, content, role, session_id, entry_id, timestamp)
                    VALUES (new.id, new.content, new.role, new.session_id, new.entry_id, new.timestamp);
                END;
                CREATE TRIGGER IF NOT EXISTS session_messages_ad AFTER DELETE ON session_messages BEGIN
                    INSERT INTO session_messages_fts(session_messages_fts, rowid, content, role, session_id, entry_id, timestamp)
                    VALUES('delete', old.id, old.content, old.role, old.session_id, old.entry_id, old.timestamp);
                END;
                ",
            )
            .is_ok();
        self.fts_available.store(fts_ok, Ordering::SeqCst);
        if !fts_ok {
            log::warn!("[db] FTS5 unavailable — cross-session search falls back to LIKE");
        }

        Ok(())
    }
}

fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("piter.db")
}
