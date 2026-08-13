//! 工作空间表 CRUD（0.3.0）：artifacts / workspace_snapshots / 交付物标记 / projects.mode。
//!
//! 做什么：维护 `artifacts`（每轮快照 diff 的变化集，按 turn 分组）、
//! `workspace_snapshots`（每会话最新文件树，仅覆盖写——diff 基线，不无限增长）、
//! `workspace_deliverable_marks`（手动标记的交付物路径集合）、
//! projects.mode（写边界模式 ask|allow|deny）。
//! 不做什么：不做文件扫描/快照 diff（gateway/workspace.rs）；不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite；`Db` 定义于 db/mod.rs。

use rusqlite::params;

use super::Db;

/// One row of the `artifacts` table: a per-turn change set entry produced by
/// snapshot diff. `deliverable` is computed at diff time (output/ ∪ manual marks).
#[derive(Debug, Clone)]
pub struct ArtifactRow {
    pub id: String,
    pub workspace_id: String,
    /// instance_id of the session that produced the turn.
    pub session_id: String,
    /// Turn sequence number (message seq at turn_end).
    pub turn_id: i64,
    /// Relative path inside the workspace (always `/`-separated).
    pub rel_path: String,
    /// `new` | `modified` | `deleted`.
    pub op: String,
    /// Byte size at diff time (0 for deleted).
    pub size: i64,
    /// Added lines (new file = total lines; modified = net growth).
    pub lines_added: i64,
    /// Deleted lines (deleted file = total lines; modified = net shrink).
    pub lines_deleted: i64,
    /// `snapshot` | `live`.
    pub source: String,
    pub deliverable: bool,
    /// RFC3339 creation time.
    pub created_at: String,
}

/// Latest file tree snapshot for a session (overwrite-only diff baseline).
#[derive(Debug, Clone)]
pub struct WorkspaceSnapshotRow {
    pub session_id: String,
    pub tree_json: String,
    pub updated_at: String,
}

impl Db {
    // ── Workspace project rows (type='workspace' in projects table) ─────────

    /// Create a project row with `type='workspace'` (workspaces live in
    /// `<data_dir>/workspaces/<id>/`, registered as a normal project so the
    /// existing sessions/projects machinery applies unchanged).
    pub fn create_workspace_project(&self, id: &str, name: &str, cwd: &str) -> Result<(), String> {
        let now = chrono::Utc::now().to_rfc3339();
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO projects (id, name, cwd, type, pinned, archived, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'workspace', 0, 0, ?4, ?4)",
            params![id, name, cwd, now],
        )
        .map_err(|e| format!("create_workspace_project: {}", e))?;
        Ok(())
    }

    /// Write-boundary mode of a workspace project (`ask` | `allow` | `deny`).
    /// Missing row / column → `ask` (safe default).
    pub fn get_project_mode(&self, id: &str) -> String {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT mode FROM projects WHERE id = ?1",
            params![id],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_else(|_| "ask".to_string())
    }

    pub fn set_project_mode(&self, id: &str, mode: &str) -> Result<(), String> {
        if !matches!(mode, "ask" | "allow" | "deny") {
            return Err(format!("invalid mode: {}", mode));
        }
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        let rows = conn
            .execute(
                "UPDATE projects SET mode = ?1, updated_at = ?2 WHERE id = ?3",
                params![mode, now, id],
            )
            .map_err(|e| format!("set_project_mode: {}", e))?;
        if rows == 0 {
            return Err(format!("project not found: {}", id));
        }
        Ok(())
    }

    // ── Artifacts ───────────────────────────────────────────────────────────

    pub fn insert_artifact(&self, row: &ArtifactRow) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO artifacts
                (id, project_id, session_id, turn_id, rel_path, op, size, lines_added, lines_deleted, source, deliverable, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
            params![
                row.id,
                row.workspace_id,
                row.session_id,
                row.turn_id,
                row.rel_path,
                row.op,
                row.size,
                row.lines_added,
                row.lines_deleted,
                row.source,
                row.deliverable as i32,
                row.created_at,
            ],
        )
        .map_err(|e| format!("insert_artifact: {}", e))?;
        Ok(())
    }

    /// Batch insert of a turn's change set (single transaction).
    pub fn insert_artifacts(&self, rows: &[ArtifactRow]) -> Result<(), String> {
        if rows.is_empty() {
            return Ok(());
        }
        let conn = self.conn.lock().unwrap();
        conn.execute("BEGIN", [])
            .map_err(|e| format!("insert_artifacts begin: {}", e))?;
        for row in rows {
            if let Err(e) = conn.execute(
                "INSERT INTO artifacts
                    (id, project_id, session_id, turn_id, rel_path, op, size, lines_added, lines_deleted, source, deliverable, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                params![
                    row.id,
                    row.workspace_id,
                    row.session_id,
                    row.turn_id,
                    row.rel_path,
                    row.op,
                    row.size,
                    row.lines_added,
                    row.lines_deleted,
                    row.source,
                    row.deliverable as i32,
                    row.created_at,
                ],
            ) {
                let _ = conn.execute("ROLLBACK", []);
                return Err(format!("insert_artifacts row: {}", e));
            }
        }
        conn.execute("COMMIT", [])
            .map_err(|e| format!("insert_artifacts commit: {}", e))?;
        Ok(())
    }

    /// All artifacts of a workspace, newest turn first (new→old per contract).
    /// `since_turn` filters to turns strictly greater than it (incremental sync).
    pub fn list_artifacts(
        &self,
        workspace_id: &str,
        since_turn: Option<i64>,
    ) -> Result<Vec<ArtifactRow>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, session_id, turn_id, rel_path, op, size, lines_added, lines_deleted, source, deliverable, created_at
                 FROM artifacts
                 WHERE project_id = ?1 AND (?2 IS NULL OR turn_id > ?2)
                 ORDER BY turn_id DESC, created_at ASC",
            )
            .map_err(|e| format!("list_artifacts prepare: {}", e))?;
        let rows: Result<Vec<_>, _> = stmt
            .query_map(params![workspace_id, since_turn], |row| {
                Ok(ArtifactRow {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    session_id: row.get(2)?,
                    turn_id: row.get(3)?,
                    rel_path: row.get(4)?,
                    op: row.get(5)?,
                    size: row.get(6)?,
                    lines_added: row.get(7)?,
                    lines_deleted: row.get(8)?,
                    source: row.get(9)?,
                    deliverable: row.get::<_, i32>(10)? != 0,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|e| format!("list_artifacts query: {}", e))?
            .collect();
        rows.map_err(|e| format!("list_artifacts rows: {}", e))
    }

    /// Artifacts whose row flag is set, merged with paths carrying a manual
    /// deliverable mark (marks synthesized with their latest update time).
    pub fn list_deliverable_artifacts(
        &self,
        workspace_id: &str,
    ) -> Result<Vec<ArtifactRow>, String> {
        // Single lock: nested locking of `conn` would deadlock, so the marks
        // query runs inline on the same connection.
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id, project_id, session_id, turn_id, rel_path, op, size, lines_added, lines_deleted, source, deliverable, created_at
                 FROM artifacts
                 WHERE project_id = ?1 AND deliverable = 1
                 ORDER BY turn_id DESC, created_at ASC",
            )
            .map_err(|e| format!("list_deliverable_artifacts prepare: {}", e))?;
        let mut out: Vec<ArtifactRow> = stmt
            .query_map(params![workspace_id], |row| {
                Ok(ArtifactRow {
                    id: row.get(0)?,
                    workspace_id: row.get(1)?,
                    session_id: row.get(2)?,
                    turn_id: row.get(3)?,
                    rel_path: row.get(4)?,
                    op: row.get(5)?,
                    size: row.get(6)?,
                    lines_added: row.get(7)?,
                    lines_deleted: row.get(8)?,
                    source: row.get(9)?,
                    deliverable: row.get::<_, i32>(10)? != 0,
                    created_at: row.get(11)?,
                })
            })
            .map_err(|e| format!("list_deliverable_artifacts query: {}", e))?
            .filter_map(|r| r.ok())
            .collect();

        // Manual marks that never appeared in a diff get a synthetic entry so
        // they still show up in the deliverables list.
        let mut mark_stmt = conn
            .prepare(
                "SELECT rel_path, updated_at FROM workspace_deliverable_marks WHERE workspace_id = ?1",
            )
            .map_err(|e| format!("list_deliverable_marks prepare: {}", e))?;
        let marked: Vec<(String, String)> = mark_stmt
            .query_map(params![workspace_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })
            .map_err(|e| format!("list_deliverable_marks query: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        let existing: std::collections::HashSet<String> =
            out.iter().map(|a| a.rel_path.clone()).collect();
        for (path, updated_at) in marked {
            if !existing.contains(&path) {
                out.push(ArtifactRow {
                    id: format!("mark:{}", crate::gateway::now_epoch_ms()),
                    workspace_id: workspace_id.to_string(),
                    session_id: String::new(),
                    turn_id: 0,
                    rel_path: path,
                    op: "new".to_string(),
                    size: 0,
                    lines_added: 0,
                    lines_deleted: 0,
                    source: "manual".to_string(),
                    deliverable: true,
                    created_at: updated_at,
                });
            }
        }
        Ok(out)
    }

    pub fn delete_workspace_artifacts(&self, workspace_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM artifacts WHERE project_id = ?1",
            params![workspace_id],
        )
        .map_err(|e| format!("delete_workspace_artifacts: {}", e))?;
        Ok(())
    }

    /// Delete artifacts of a session (used when the session is deleted).
    pub fn delete_session_artifacts(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM artifacts WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete_session_artifacts: {}", e))?;
        Ok(())
    }

    // ── Workspace snapshots (per-session latest tree, overwrite-only) ───────

    pub fn get_snapshot(&self, session_id: &str) -> Option<WorkspaceSnapshotRow> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT session_id, tree_json, updated_at FROM workspace_snapshots WHERE session_id = ?1",
            params![session_id],
            |row| {
                Ok(WorkspaceSnapshotRow {
                    session_id: row.get(0)?,
                    tree_json: row.get(1)?,
                    updated_at: row.get(2)?,
                })
            },
        )
        .ok()
    }

    /// Upsert the latest tree snapshot for a session.
    pub fn set_snapshot(&self, session_id: &str, tree_json: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        let now = chrono::Utc::now().to_rfc3339();
        conn.execute(
            "INSERT INTO workspace_snapshots (session_id, tree_json, updated_at)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET tree_json = ?2, updated_at = ?3",
            params![session_id, tree_json, now],
        )
        .map_err(|e| format!("set_snapshot: {}", e))?;
        Ok(())
    }

    pub fn delete_snapshot(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM workspace_snapshots WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete_snapshot: {}", e))?;
        Ok(())
    }

    // ── Deliverable marks (manual, cross-turn) ───────────────────────────────

    /// Mark/unmark a path as deliverable. Returns true when the mark changed.
    pub fn set_deliverable_mark(
        &self,
        workspace_id: &str,
        path: &str,
        deliverable: bool,
    ) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();
        if deliverable {
            let now = chrono::Utc::now().to_rfc3339();
            conn.execute(
                "INSERT INTO workspace_deliverable_marks (workspace_id, rel_path, updated_at)
                 VALUES (?1, ?2, ?3)
                 ON CONFLICT(workspace_id, rel_path) DO UPDATE SET updated_at = ?3",
                params![workspace_id, path, now],
            )
            .map_err(|e| format!("set_deliverable_mark: {}", e))?;
        } else {
            conn.execute(
                "DELETE FROM workspace_deliverable_marks WHERE workspace_id = ?1 AND rel_path = ?2",
                params![workspace_id, path],
            )
            .map_err(|e| format!("clear_deliverable_mark: {}", e))?;
        }
        Ok(true)
    }

    pub fn is_deliverable_marked(&self, workspace_id: &str, path: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT 1 FROM workspace_deliverable_marks WHERE workspace_id = ?1 AND rel_path = ?2",
            params![workspace_id, path],
            |_| Ok(()),
        )
        .is_ok()
    }

    /// All manual marks as `(path, updated_at RFC3339)`.
    pub fn list_deliverable_marks(&self, workspace_id: &str) -> Vec<(String, String)> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT rel_path, updated_at FROM workspace_deliverable_marks WHERE workspace_id = ?1",
            )
            .unwrap();
        stmt.query_map(params![workspace_id], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    pub fn delete_workspace_marks(&self, workspace_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM workspace_deliverable_marks WHERE workspace_id = ?1",
            params![workspace_id],
        )
        .map_err(|e| format!("delete_workspace_marks: {}", e))?;
        Ok(())
    }

    // ── Workspace base dir config (0.3.0: default install dir + Admin 可配置) ──

    /// Configured workspace base dir ('' = not configured → default install dir).
    pub fn get_workspace_base_dir(&self) -> String {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT base_dir FROM workspace_config WHERE id = 1",
            [],
            |row| row.get::<_, String>(0),
        )
        .unwrap_or_default()
    }

    /// Persist the workspace base dir ('' clears the override → back to default).
    pub fn set_workspace_base_dir(&self, base_dir: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO workspace_config (id, base_dir, updated_at)
             VALUES (1, ?1, ?2)
             ON CONFLICT(id) DO UPDATE SET base_dir = ?1, updated_at = ?2",
            params![base_dir, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("set_workspace_base_dir: {}", e))?;
        Ok(())
    }

    // ── Migration helpers (基目录变更 → real_dir 迁移后同步 DB) ─────────────

    /// Update a workspace project's cwd after files were moved.
    pub fn update_workspace_cwd(&self, id: &str, new_cwd: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE projects SET cwd = ?1, updated_at = ?2 WHERE id = ?3 AND type = 'workspace'",
            params![new_cwd, chrono::Utc::now().to_rfc3339(), id],
        )
        .map_err(|e| format!("update_workspace_cwd: {}", e))?;
        Ok(())
    }

    /// Sync linked sessions' cwd after a workspace migration (sessions.cwd 与
    /// projects.cwd 保持一致——work 会话 cwd = workspace real_dir)。
    pub fn update_sessions_cwd_for_project(
        &self,
        project_id: &str,
        new_cwd: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE sessions SET cwd = ?1 WHERE project_id = ?2",
            params![new_cwd, project_id],
        )
        .map_err(|e| format!("update_sessions_cwd_for_project: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(db: &Db, id: &str, name: &str, cwd: &str) {
        db.create_workspace_project(id, name, cwd).unwrap();
    }

    fn artifact(ws: &str, session: &str, turn: i64, path: &str, op: &str, deliv: bool) -> ArtifactRow {
        ArtifactRow {
            id: format!("a_{}_{}_{}", ws, turn, path),
            workspace_id: ws.to_string(),
            session_id: session.to_string(),
            turn_id: turn,
            rel_path: path.to_string(),
            op: op.to_string(),
            size: 100,
            lines_added: 0,
            lines_deleted: 0,
            source: "snapshot".to_string(),
            deliverable: deliv,
            created_at: chrono::Utc::now().to_rfc3339(),
        }
    }

    #[test]
    fn artifacts_roundtrip_and_turn_grouping() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        ws(&db, "ws1", "W", "/ws1");
        ws(&db, "ws2", "W2", "/ws2");

        db.insert_artifacts(&[
            artifact("ws1", "s1", 3, "output/report.md", "modified", true),
            artifact("ws1", "s1", 3, "src/lib.rs", "new", false),
            artifact("ws1", "s1", 5, "output/report.md", "new", true),
        ])
        .unwrap();

        // Newest turn first.
        let all = db.list_artifacts("ws1", None).unwrap();
        let turns: Vec<i64> = all.iter().map(|a| a.turn_id).collect();
        assert_eq!(turns, vec![5, 3, 3]);
        // since_turn incremental sync.
        let since = db.list_artifacts("ws1", Some(3)).unwrap();
        assert_eq!(since.len(), 1);
        assert_eq!(since[0].turn_id, 5);
        // Other workspace isolated.
        assert!(db.list_artifacts("ws2", None).unwrap().is_empty());

        // Deliverables: flagged rows only (mark table empty here).
        let deliv = db.list_deliverable_artifacts("ws1").unwrap();
        assert_eq!(deliv.len(), 2);
        assert!(deliv.iter().all(|a| a.deliverable));
    }

    #[test]
    fn deliverable_marks_persist_and_feed_deliverables() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        ws(&db, "ws1", "W", "/ws1");

        db.set_deliverable_mark("ws1", "assets/logo.png", true).unwrap();
        assert!(db.is_deliverable_marked("ws1", "assets/logo.png"));
        assert!(!db.is_deliverable_marked("ws1", "nope.txt"));
        assert_eq!(db.list_deliverable_marks("ws1").len(), 1);

        // A mark without an artifact row still shows up in deliverables.
        let deliv = db.list_deliverable_artifacts("ws1").unwrap();
        assert_eq!(deliv.len(), 1);
        assert_eq!(deliv[0].rel_path, "assets/logo.png");
        assert!(deliv[0].deliverable);

        // Unmark removes it.
        db.set_deliverable_mark("ws1", "assets/logo.png", false).unwrap();
        assert!(db.list_deliverable_artifacts("ws1").unwrap().is_empty());
    }

    #[test]
    fn snapshot_overwrite_and_delete() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        assert!(db.get_snapshot("s1").is_none());

        db.set_snapshot("s1", r#"{"files":[]}"#).unwrap();
        let snap = db.get_snapshot("s1").unwrap();
        assert_eq!(snap.tree_json, r#"{"files":[]}"#);

        // Overwrite-only: latest tree wins, no growth.
        db.set_snapshot("s1", r#"{"files":[{"path":"a.txt"}]}"#).unwrap();
        let snap2 = db.get_snapshot("s1").unwrap();
        assert_eq!(snap2.tree_json, r#"{"files":[{"path":"a.txt"}]}"#);

        db.delete_snapshot("s1").unwrap();
        assert!(db.get_snapshot("s1").is_none());
    }

    #[test]
    fn project_mode_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        ws(&db, "ws1", "W", "/ws1");

        // Default is ask (safe).
        assert_eq!(db.get_project_mode("ws1"), "ask");
        db.set_project_mode("ws1", "deny").unwrap();
        assert_eq!(db.get_project_mode("ws1"), "deny");
        assert!(db.set_project_mode("ws1", "bogus").is_err());
        // Unknown project → default ask.
        assert_eq!(db.get_project_mode("nope"), "ask");
    }

    #[test]
    fn workspace_delete_cascades_artifacts_and_marks() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();
        ws(&db, "ws1", "W", "/ws1");
        db.insert_artifact(&artifact("ws1", "s1", 1, "a.txt", "new", true)).unwrap();
        db.set_deliverable_mark("ws1", "b.txt", true).unwrap();

        db.delete_project("ws1").unwrap();
        assert!(db.list_artifacts("ws1", None).unwrap().is_empty());
        assert!(db.list_deliverable_marks("ws1").is_empty());
    }
}
