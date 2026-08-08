//! SQLite database layer for project/session/extension management.
//!
//! Database location: `<piter_data_dir>/piter.db`
//!
//! Tables:
//! - `projects` — project metadata with pin/archive support
//! - `project_added_extensions` — per-project extension names (added on top of global)
//! - `project_excluded_extensions` — per-project excluded extension names
//! - `sessions` — session_path → project_id mapping
//! - `global_extensions` — global extension names

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};
use serde::Serialize;

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

    // ── Projects ───────────────────────────────────────────────────────

    pub fn create_project(&self, id: &str, name: &str, cwd: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, cwd, type, pinned, archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'normal', 0, 0, ?4, ?4)",
            params![id, name, cwd, now],
        )
        .map_err(|e| format!("create_project: {}", e))?;
        Ok(())
    }

    pub fn update_project(
        &self,
        id: &str,
        name: Option<&str>,
        extensions: Option<&[String]>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();

        if let Some(n) = name {
            conn.execute(
                "UPDATE projects SET name = ?1, updated_at = ?2 WHERE id = ?3",
                params![n, now, id],
            )
            .map_err(|e| format!("update_project name: {}", e))?;
        }
        if let Some(exts) = extensions {
            conn.execute(
                "DELETE FROM project_added_extensions WHERE project_id = ?1",
                params![id],
            )
            .map_err(|e| format!("clear extensions: {}", e))?;
            // Get cwd for resolving extension paths
            let cwd: String = conn
                .query_row(
                    "SELECT cwd FROM projects WHERE id = ?1",
                    params![id],
                    |row| row.get(0),
                )
                .unwrap_or_default();
            let mut stmt = conn
                .prepare("INSERT INTO project_added_extensions (project_id, extension_name, extension_path) VALUES (?1, ?2, ?3)")
                .map_err(|e| format!("prepare ext insert: {}", e))?;
            for ext in exts {
                let path = super::project::resolve_extension_name(ext, &cwd)
                    .map(|p| p.to_string_lossy().to_string());
                stmt.execute(params![id, ext, path])
                    .map_err(|e| format!("insert extension: {}", e))?;
            }
            conn.execute(
                "UPDATE projects SET updated_at = ?1 WHERE id = ?2",
                params![now, id],
            )
            .map_err(|e| format!("update_project timestamp: {}", e))?;
        }
        Ok(())
    }

    pub fn delete_project(&self, id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM projects WHERE id = ?1", params![id])
            .map_err(|e| format!("delete_project: {}", e))?;
        if rows == 0 {
            return Err(format!("project not found: {}", id));
        }
        Ok(())
    }

    pub fn get_project(&self, id: &str) -> Option<ProjectRow> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, cwd, type, pinned, archived, created_at, updated_at
             FROM projects WHERE id = ?1",
            params![id],
            |row| {
                Ok(ProjectRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    cwd: row.get(2)?,
                    project_type: row.get(3)?,
                    pinned: row.get(4)?,
                    archived: row.get(5)?,
                    created_at: row.get(6)?,
                    updated_at: row.get(7)?,
                })
            },
        )
        .ok()
    }

    pub fn list_projects(&self, include_archived: bool) -> Vec<ProjectRow> {
        let conn = self.conn.lock().unwrap();
        let sql = if include_archived {
            "SELECT id, name, cwd, type, pinned, archived, created_at, updated_at
             FROM projects ORDER BY pinned DESC, updated_at DESC"
        } else {
            "SELECT id, name, cwd, type, pinned, archived, created_at, updated_at
             FROM projects WHERE archived = 0 ORDER BY pinned DESC, updated_at DESC"
        };
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map([], |row| {
            Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                project_type: row.get(3)?,
                pinned: row.get(4)?,
                archived: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn set_pinned(&self, id: &str, pinned: i32) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn
            .execute(
                "UPDATE projects SET pinned = ?1, updated_at = ?2 WHERE id = ?3",
                params![pinned, now, id],
            )
            .map_err(|e| format!("set_pinned: {}", e))?;
        if rows == 0 {
            return Err(format!("project not found: {}", id));
        }
        Ok(())
    }

    pub fn set_archived(&self, id: &str, archived: bool) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn
            .execute(
                "UPDATE projects SET archived = ?1, updated_at = ?2 WHERE id = ?3",
                params![archived as i32, now, id],
            )
            .map_err(|e| format!("set_archived: {}", e))?;
        if rows == 0 {
            return Err(format!("project not found: {}", id));
        }
        Ok(())
    }

    // ── Project Extensions ─────────────────────────────────────────────

    pub fn get_project_added_extensions(&self, project_id: &str) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT extension_name FROM project_added_extensions WHERE project_id = ?1")
            .unwrap();
        stmt.query_map(params![project_id], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Get the extensions this project explicitly excludes (never loaded even
    /// when enabled globally).
    pub fn get_project_excluded_extensions(&self, project_id: &str) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT extension_name FROM project_excluded_extensions WHERE project_id = ?1")
            .unwrap();
        stmt.query_map(params![project_id], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Replace a project's excluded extension list (full replace, same style
    /// as `set_project_added_extensions`).
    pub fn set_project_excluded_extensions(
        &self,
        project_id: &str,
        extensions: &[String],
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM project_excluded_extensions WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|e| format!("clear project excluded extensions: {}", e))?;
        let mut stmt = conn
            .prepare("INSERT INTO project_excluded_extensions (project_id, extension_name) VALUES (?1, ?2)")
            .map_err(|e| format!("prepare project excluded insert: {}", e))?;
        for ext in extensions {
            stmt.execute(params![project_id, ext])
                .map_err(|e| format!("insert project excluded: {}", e))?;
        }
        Ok(())
    }

    // ── Sessions ───────────────────────────────────────────────────────

    /// Register a new session in the database (before pi reports sessionPath).
    pub fn register_session(
        &self,
        instance_id: &str,
        cwd: &str,
        project_id: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO sessions (instance_id, session_path, project_id, cwd, created_at)
             VALUES (?1, NULL, ?2, ?3, ?4)",
            params![instance_id, project_id, cwd, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("register_session: {}", e))?;
        Ok(())
    }

    /// Complete a session with the actual session file path (from pi's get_state).
    pub fn complete_session(&self, instance_id: &str, session_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET session_path = ?1 WHERE instance_id = ?2",
            params![session_path, instance_id],
        )
        .map_err(|e| format!("complete_session: {}", e))?;
        Ok(())
    }

    /// Delete a session record by session_path.
    pub fn delete_session(&self, session_path: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        if let Ok(Some(iid)) = conn.query_row(
            "SELECT instance_id FROM sessions WHERE session_path = ?1",
            params![session_path],
            |row| row.get::<_, Option<String>>(0),
        ) {
            let _ = conn.execute("DELETE FROM session_messages WHERE session_id = ?1", params![iid]);
            let _ = conn.execute("DELETE FROM search_index_state WHERE session_id = ?1", params![iid]);
        }
        conn.execute(
            "DELETE FROM sessions WHERE session_path = ?1",
            params![session_path],
        )
        .map_err(|e| format!("delete_session: {}", e))?;
        Ok(())
    }

    /// Delete a session record by instance_id (also clears its search index).
    pub fn delete_session_by_instance(&self, instance_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM session_messages WHERE session_id = ?1",
            params![instance_id],
        )
        .map_err(|e| format!("delete_session_by_instance messages: {}", e))?;
        conn.execute(
            "DELETE FROM search_index_state WHERE session_id = ?1",
            params![instance_id],
        )
        .map_err(|e| format!("delete_session_by_instance state: {}", e))?;
        conn.execute(
            "DELETE FROM sessions WHERE instance_id = ?1",
            params![instance_id],
        )
        .map_err(|e| format!("delete_session_by_instance: {}", e))?;
        Ok(())
    }

    /// Get the session file path for an instance_id, if known.
    pub fn get_session_path(&self, instance_id: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT session_path FROM sessions WHERE instance_id = ?1",
            params![instance_id],
            |row| row.get(0),
        )
        .ok()
        .flatten()
    }

    /// Find the instance_id for a session file path, if registered.
    pub fn session_id_for_path(&self, session_path: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT instance_id FROM sessions WHERE session_path = ?1",
            params![session_path],
            |row| row.get(0),
        )
        .ok()
    }

    /// Get the project_id for a session by instance_id.
    pub fn get_session_project(&self, instance_id: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT project_id FROM sessions WHERE instance_id = ?1",
            params![instance_id],
            |row| row.get(0),
        )
        .ok()
        .flatten()
    }

    /// Get all instance_ids linked to a project.
    pub fn get_project_sessions(&self, project_id: &str) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT instance_id FROM sessions WHERE project_id = ?1")
            .unwrap();
        stmt.query_map(params![project_id], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Set the session name (auto-generated or user-set).
    pub fn set_session_name(&self, instance_id: &str, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET name = ?1 WHERE instance_id = ?2",
            params![name, instance_id],
        )
        .map_err(|e| format!("set_session_name: {}", e))?;
        Ok(())
    }

    /// Persist the model (id + provider) this instance is currently using,
    /// so the session can restore its own model after a restart.
    pub fn set_session_model(
        &self,
        instance_id: &str,
        model_id: Option<&str>,
        model_provider: Option<&str>,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET model_id = ?1, model_provider = ?2 WHERE instance_id = ?3",
            params![model_id, model_provider, instance_id],
        )
        .map_err(|e| format!("set_session_model: {}", e))?;
        Ok(())
    }

    /// Get all sessions.
    pub fn all_sessions(&self) -> Vec<SessionRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT instance_id, session_path, project_id, cwd, name, created_at, \
                        model_id, model_provider FROM sessions",
            )
            .unwrap();
        stmt.query_map([], |row| {
            Ok(SessionRow {
                instance_id: row.get(0)?,
                session_path: row.get(1)?,
                project_id: row.get(2)?,
                cwd: row.get(3)?,
                name: row.get(4)?,
                created_at: row.get(5)?,
                model_id: row.get(6)?,
                model_provider: row.get(7)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Find a project by its cwd path and name.
    pub fn find_project_by_cwd_and_name(&self, cwd: &str, name: &str) -> Option<ProjectRow> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, name, cwd, type, pinned, archived, created_at, updated_at \
             FROM projects WHERE cwd = ?1 AND name = ?2",
            params![cwd, name],
            |row| Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                project_type: row.get(3)?,
                pinned: row.get(4)?,
                archived: row.get(5)?,
                created_at: row.get(6)?,
                updated_at: row.get(7)?,
            }),
        )
        .ok()
    }

    // ── Cross-session Search ────────────────────────────────────────────

    /// Whether FTS5 (trigram tokenizer) is available; false → LIKE fallback.
    pub fn fts_available(&self) -> bool {
        self.fts_available.load(Ordering::SeqCst)
    }

    /// Replace a session's indexed messages (full re-index after mtime change).
    /// The FTS index stays in sync via triggers on `session_messages`.
    pub fn index_session_messages(
        &self,
        session_id: &str,
        entries: &[IndexedMessage],
        file_mtime: &str,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| format!("index tx: {e}"))?;
        tx.execute(
            "DELETE FROM session_messages WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("index delete: {e}"))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO session_messages (session_id, entry_id, role, content, timestamp, file_mtime)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| format!("index insert prepare: {e}"))?;
            for m in entries {
                stmt.execute(params![
                    session_id,
                    m.entry_id,
                    m.role,
                    m.content,
                    m.timestamp,
                    file_mtime
                ])
                .map_err(|e| format!("index insert: {e}"))?;
            }
        }
        tx.execute(
            "INSERT INTO search_index_state (session_id, file_mtime, indexed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET file_mtime = excluded.file_mtime, indexed_at = excluded.indexed_at",
            params![session_id, file_mtime, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("index state: {e}"))?;
        tx.commit().map_err(|e| format!("index commit: {e}"))?;
        Ok(())
    }

    /// Drop a session's indexed messages (and FTS rows via trigger).
    pub fn delete_session_fts(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM session_messages WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete fts: {e}"))?;
        conn.execute(
            "DELETE FROM search_index_state WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete fts state: {e}"))?;
        Ok(())
    }

    /// Wipe the whole search index (reindex_all).
    pub fn clear_search_index(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM session_messages", [])
            .map_err(|e| format!("clear search: {e}"))?;
        conn.execute("DELETE FROM search_index_state", [])
            .map_err(|e| format!("clear search state: {e}"))?;
        Ok(())
    }

    /// Last indexed file-mtime for a session (None = never indexed).
    pub fn search_index_mtime(&self, session_id: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT file_mtime FROM search_index_state WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .ok()
    }

    /// Full-text search across all indexed session messages, newest first.
    /// Uses FTS5 (trigram) for queries ≥ 3 chars when available, else LIKE.
    pub fn search_messages(&self, q: &str, limit: u32) -> Result<Vec<SearchHit>, String> {
        let q = q.trim();
        let qlen = q.chars().count();
        if q.is_empty() || qlen < 2 {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 200);
        let conn = self.conn.lock().unwrap();

        // (content, role, session_id, entry_id, timestamp)
        let rows: Vec<(String, String, String, Option<String>, Option<i64>)> =
            if self.fts_available() && qlen >= 3 {
                // trigram tokenizer: query must be a quoted phrase (≥3 chars)
                let fts_q = format!("\"{}\"", q.replace('"', "\"\""));
                let mut stmt = conn
                    .prepare(
                        "SELECT f.content, f.role, f.session_id, f.entry_id, f.timestamp
                         FROM session_messages_fts f
                         WHERE session_messages_fts MATCH ?1
                         ORDER BY f.timestamp IS NULL, f.timestamp DESC LIMIT ?2",
                    )
                    .map_err(|e| format!("search prepare: {e}"))?;
                stmt.query_map(params![fts_q, limit], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })
                .map_err(|e| format!("search query: {e}"))?
                .filter_map(|r| r.ok())
                .collect()
            } else {
                let like = format!(
                    "%{}%",
                    q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
                );
                let mut stmt = conn
                    .prepare(
                        "SELECT content, role, session_id, entry_id, timestamp
                         FROM session_messages
                         WHERE content LIKE ?1 ESCAPE '\\'
                         ORDER BY timestamp IS NULL, timestamp DESC LIMIT ?2",
                    )
                    .map_err(|e| format!("search prepare: {e}"))?;
                stmt.query_map(params![like, limit], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })
                .map_err(|e| format!("search query: {e}"))?
                .filter_map(|r| r.ok())
                .collect()
            };

        let mut out = Vec::with_capacity(rows.len());
        for (content, role, session_id, entry_id, ts) in rows {
            let (label, project_name) = conn
                .query_row(
                    "SELECT s.name, p.name FROM sessions s
                     LEFT JOIN projects p ON p.id = s.project_id
                     WHERE s.instance_id = ?1",
                    params![session_id],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .unwrap_or((None, None));
            out.push(SearchHit {
                session_id,
                project_name,
                label,
                role,
                snippet: make_snippet(&content, q),
                entry_id,
                timestamp: ts,
            });
        }
        Ok(out)
    }

    // ── Global Extensions ──────────────────────────────────────────────

    pub fn get_global_extensions(&self) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT extension_name FROM global_extensions")
            .unwrap();
        stmt.query_map([], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Get global extensions with their resolved file paths.
    pub fn get_global_extensions_with_paths(&self) -> Vec<(String, Option<String>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT extension_name, extension_path FROM global_extensions")
            .unwrap();
        stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    pub fn set_global_extensions(&self, extensions: &[String]) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        // Full replace: the caller (Extensions tab) sends the complete enabled
        // list, so a disabled extension (npm:/git:/path-backed included) must
        // be removed — no special-cased "package source" preservation.
        conn.execute("DELETE FROM global_extensions", [])
            .map_err(|e| format!("clear global exts: {}", e))?;
        let mut stmt = conn
            .prepare("INSERT INTO global_extensions (extension_name, extension_path) VALUES (?1, ?2)")
            .map_err(|e| format!("prepare global ext insert: {}", e))?;
        let agent_dir = crate::broker::util::get_pi_agent_dir();
        for ext in extensions {
            let path = super::project::resolve_extension_name(ext, "")
                .filter(|p| p.starts_with(&agent_dir))
                .map(|p| p.to_string_lossy().to_string());
            stmt.execute(params![ext, path])
                .map_err(|e| format!("insert global ext: {}", e))?;
        }
        Ok(())
    }

    /// Add a single global extension entry (insert-or-ignore).
    pub fn add_global_extension(&self, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let agent_dir = crate::broker::util::get_pi_agent_dir();
        let path = super::project::resolve_extension_name(name, "")
            .filter(|p| p.starts_with(&agent_dir))
            .map(|p| p.to_string_lossy().to_string());
        conn.execute(
            "INSERT OR IGNORE INTO global_extensions (extension_name, extension_path) VALUES (?1, ?2)",
            params![name, path],
        )
        .map_err(|e| format!("add global ext: {}", e))?;
        Ok(())
    }

    /// Remove a single global extension entry.
    pub fn remove_global_extension(&self, name: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM global_extensions WHERE extension_name = ?1",
            params![name],
        )
        .map_err(|e| format!("remove global ext: {}", e))?;
        Ok(())
    }

    /// Replace a project's added-extension list, re-resolving extension paths.
    pub fn set_project_added_extensions(
        &self,
        project_id: &str,
        extensions: &[String],
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM project_added_extensions WHERE project_id = ?1",
            params![project_id],
        )
        .map_err(|e| format!("clear project extensions: {}", e))?;
        let cwd: String = conn
            .query_row(
                "SELECT cwd FROM projects WHERE id = ?1",
                params![project_id],
                |row| row.get(0),
            )
            .unwrap_or_default();
        let mut stmt = conn
            .prepare("INSERT INTO project_added_extensions (project_id, extension_name, extension_path) VALUES (?1, ?2, ?3)")
            .map_err(|e| format!("prepare project ext insert: {}", e))?;
        for ext in extensions {
            let path = super::project::resolve_extension_name(ext, &cwd)
                .map(|p| p.to_string_lossy().to_string());
            stmt.execute(params![project_id, ext, path])
                .map_err(|e| format!("insert project ext: {}", e))?;
        }
        Ok(())
    }
}

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ProjectRow {
    pub id: String,
    pub name: String,
    pub cwd: String,
    /// `normal` or `workspace` (0.3.0 workspaces). Defaults to `normal`.
    pub project_type: String,
    pub pinned: i32,
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone)]
pub struct SessionRow {
    pub instance_id: String,
    pub session_path: Option<String>,
    pub project_id: Option<String>,
    pub cwd: String,
    pub name: Option<String>,
    pub created_at: String,
    /// Persisted model id (None until the first get_state/set_model report).
    pub model_id: Option<String>,
    /// Persisted provider for `model_id`.
    pub model_provider: Option<String>,
}

/// A message ready to be inserted into the search index (search module output).
#[derive(Debug, Clone)]
pub struct IndexedMessage {
    pub role: String,
    pub content: String,
    pub entry_id: Option<String>,
    /// Epoch milliseconds (matches the frontend Message.timestamp).
    pub timestamp: Option<i64>,
}

/// One cross-session search hit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub session_id: String,
    pub project_name: Option<String>,
    pub label: Option<String>,
    pub role: String,
    /// Context window around the first match, ready for display.
    pub snippet: String,
    pub entry_id: Option<String>,
    /// Epoch milliseconds; used by the frontend to scroll to the message.
    pub timestamp: Option<i64>,
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Char-safe context window around the first case-insensitive match.
/// `find` on a lowercased copy yields byte offsets that may split UTF-8
/// (case folding changes byte length), so we convert to a char offset first.
fn make_snippet(content: &str, q: &str) -> String {
    const RADIUS: usize = 60;
    let c_lower = content.to_lowercase();
    let Some(rel) = c_lower.find(&q.to_lowercase()) else {
        return truncate_chars(content, 160);
    };
    let char_pos = c_lower[..rel].chars().count();
    let total_chars = content.chars().count();
    let start_char = char_pos.saturating_sub(RADIUS);
    let end_char = (char_pos + q.chars().count() + RADIUS).min(total_chars);
    let mut out: String = content
        .chars()
        .skip(start_char)
        .take(end_char - start_char)
        .collect();
    if start_char > 0 {
        out.insert_str(0, "…");
    }
    if end_char < total_chars {
        out.push('…');
    }
    out
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}

fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("piter.db")
}
