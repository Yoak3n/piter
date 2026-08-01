//! SQLite database layer for project/session/extension management.
//!
//! Database location: `<piter_data_dir>/piter.db`
//!
//! Tables:
//! - `projects` — project metadata with pin/archive support
//! - `project_extensions` — per-project extension names
//! - `sessions` — session_path → project_id mapping
//! - `global_extensions` — global extension names

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use rusqlite::{params, Connection};

// ─── Database Handle ────────────────────────────────────────────────────────

pub struct Db {
    conn: Mutex<Connection>,
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
                pinned      INTEGER NOT NULL DEFAULT 0,
                archived    INTEGER NOT NULL DEFAULT 0,
                created_at  TEXT NOT NULL,
                updated_at  TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS project_extensions (
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
            ",
        )
        .map_err(|e| format!("migrate: {}", e))?;

        // Add extension_path column to existing tables (idempotent for upgrades)
        let _ = conn.execute_batch(
            "ALTER TABLE project_extensions ADD COLUMN extension_path TEXT;
             ALTER TABLE global_extensions ADD COLUMN extension_path TEXT;"
        );
        // Ignore errors if columns already exist

        Ok(())
    }

    // ── Projects ───────────────────────────────────────────────────────

    pub fn create_project(&self, id: &str, name: &str, cwd: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, cwd, pinned, archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, 0, 0, ?4, ?4)",
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
                "DELETE FROM project_extensions WHERE project_id = ?1",
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
                .prepare("INSERT INTO project_extensions (project_id, extension_name, extension_path) VALUES (?1, ?2, ?3)")
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
            "SELECT id, name, cwd, pinned, archived, created_at, updated_at
             FROM projects WHERE id = ?1",
            params![id],
            |row| {
                Ok(ProjectRow {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    cwd: row.get(2)?,
                    pinned: row.get(3)?,
                    archived: row.get(4)?,
                    created_at: row.get(5)?,
                    updated_at: row.get(6)?,
                })
            },
        )
        .ok()
    }

    pub fn list_projects(&self, include_archived: bool) -> Vec<ProjectRow> {
        let conn = self.conn.lock().unwrap();
        let sql = if include_archived {
            "SELECT id, name, cwd, pinned, archived, created_at, updated_at
             FROM projects ORDER BY pinned DESC, updated_at DESC"
        } else {
            "SELECT id, name, cwd, pinned, archived, created_at, updated_at
             FROM projects WHERE archived = 0 ORDER BY pinned DESC, updated_at DESC"
        };
        let mut stmt = conn.prepare(sql).unwrap();
        stmt.query_map([], |row| {
            Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                pinned: row.get(3)?,
                archived: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
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

    pub fn get_project_extensions(&self, project_id: &str) -> Vec<String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT extension_name FROM project_extensions WHERE project_id = ?1")
            .unwrap();
        stmt.query_map(params![project_id], |row| row.get(0))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Get project extensions with their resolved file paths.
    /// Returns Vec of (name, Option<path>).
    pub fn get_project_extensions_with_paths(&self, project_id: &str) -> Vec<(String, Option<String>)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT extension_name, extension_path FROM project_extensions WHERE project_id = ?1")
            .unwrap();
        stmt.query_map(params![project_id], |row| Ok((row.get(0)?, row.get(1)?)))
            .unwrap()
            .filter_map(|r| r.ok())
            .collect()
    }

    /// Update the resolved path for a specific project extension.
    pub fn set_project_extension_path(&self, project_id: &str, name: &str, path: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE project_extensions SET extension_path = ?1 WHERE project_id = ?2 AND extension_name = ?3",
            params![path, project_id, name],
        )
        .map_err(|e| format!("set_project_extension_path: {}", e))?;
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
        conn.execute(
            "DELETE FROM sessions WHERE session_path = ?1",
            params![session_path],
        )
        .map_err(|e| format!("delete_session: {}", e))?;
        Ok(())
    }

    /// Delete a session record by instance_id.
    pub fn delete_session_by_instance(&self, instance_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
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

    /// Get all sessions.
    pub fn all_sessions(&self) -> Vec<SessionRow> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT instance_id, session_path, project_id, cwd, name, created_at FROM sessions")
            .unwrap();
        stmt.query_map([], |row| {
            Ok(SessionRow {
                instance_id: row.get(0)?,
                session_path: row.get(1)?,
                project_id: row.get(2)?,
                cwd: row.get(3)?,
                name: row.get(4)?,
                created_at: row.get(5)?,
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
            "SELECT id, name, cwd, pinned, archived, created_at, updated_at \
             FROM projects WHERE cwd = ?1 AND name = ?2",
            params![cwd, name],
            |row| Ok(ProjectRow {
                id: row.get(0)?,
                name: row.get(1)?,
                cwd: row.get(2)?,
                pinned: row.get(3)?,
                archived: row.get(4)?,
                created_at: row.get(5)?,
                updated_at: row.get(6)?,
            }),
        )
        .ok()
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

    /// Replace a project's extension list, re-resolving extension paths.
    pub fn set_project_extensions(
        &self,
        project_id: &str,
        extensions: &[String],
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM project_extensions WHERE project_id = ?1",
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
            .prepare("INSERT INTO project_extensions (project_id, extension_name, extension_path) VALUES (?1, ?2, ?3)")
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
}

// ─── Helpers ────────────────────────────────────────────────────────────────

fn db_path(data_dir: &Path) -> PathBuf {
    data_dir.join("piter.db")
}
