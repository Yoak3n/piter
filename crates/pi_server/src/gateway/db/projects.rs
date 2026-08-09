//! 项目表 CRUD：创建/更新/删除/查询 + 置顶/归档 + 按 cwd+name 查找。
//!
//! 做什么：维护 `projects` 表（name/cwd/type/pinned/archived/时间戳），
//! `update_project` 同时整体替换项目的 added 扩展列表（含路径解析）。
//! 不做什么：不含全局/排除扩展管理（extensions.rs）；不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite；`Db` 定义于 db/mod.rs；扩展路径解析见 crate::gateway::project。

use rusqlite::params;

use super::Db;

/// One row of the `projects` table.
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

impl Db {
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
                let path = crate::gateway::project::resolve_extension_name(ext, &cwd)
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
}
