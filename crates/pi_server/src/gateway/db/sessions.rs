//! 会话表 CRUD：注册/完成/删除/查询 + 名称、置顶、模型持久化。
//!
//! 做什么：维护 `sessions` 表——按 instance_id 注册会话、回填 session_path、
//! 删除（连带清理搜索索引行）、按路径/实例/项目查询、设置名称/置顶/模型。
//! 不做什么：不解析会话文件（stats/search 模块）；不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite；`Db` 定义于 db/mod.rs。

use rusqlite::params;

use super::Db;

/// One row of the `sessions` table.
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
    /// 1 when pinned (sorts first within the owning project).
    pub pinned: i32,
}

impl Db {
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

    /// Pin/unpin a session. Unlike `set_pinned` (projects) this deliberately
    /// leaves `created_at` untouched, so unpinning restores the original
    /// updated_at order instead of bumping the session to the top.
    pub fn set_session_pinned(&self, instance_id: &str, pinned: i32) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute(
                "UPDATE sessions SET pinned = ?1 WHERE instance_id = ?2",
                params![pinned, instance_id],
            )
            .map_err(|e| format!("set_session_pinned: {}", e))?;
        if rows == 0 {
            return Err(format!("session not found: {}", instance_id));
        }
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
                        model_id, model_provider, pinned FROM sessions",
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
                pinned: row.get(8)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_pinned_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        db.register_session("i1", "/tmp/proj", None).unwrap();

        let rows = db.all_sessions();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].pinned, 0, "new sessions default to unpinned");

        db.set_session_pinned("i1", 1).unwrap();
        assert_eq!(db.all_sessions()[0].pinned, 1);

        // Unpin restores the default.
        db.set_session_pinned("i1", 0).unwrap();
        assert_eq!(db.all_sessions()[0].pinned, 0);

        // Unknown instance → error (mirrors set_pinned).
        assert!(db.set_session_pinned("missing", 1).is_err());
    }
}
