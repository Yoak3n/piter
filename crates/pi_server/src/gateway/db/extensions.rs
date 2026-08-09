//! 扩展表 CRUD：全局扩展 + 项目的 added/excluded 扩展。
//!
//! 做什么：维护 `global_extensions`（全局启用列表，支持路径解析）、
//! `project_added_extensions`（项目叠加扩展）、`project_excluded_extensions`
//! （项目排除列表，即使全局启用也不加载）。均为整体替换式写入。
//! 不做什么：不扫描磁盘上的扩展候选（ext_cache.rs）；不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite；`Db` 定义于 db/mod.rs；扩展路径解析见 crate::gateway::project。

use rusqlite::params;

use super::Db;

impl Db {
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
            let path = crate::gateway::project::resolve_extension_name(ext, "")
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
        let path = crate::gateway::project::resolve_extension_name(name, "")
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
            let path = crate::gateway::project::resolve_extension_name(ext, &cwd)
                .map(|p| p.to_string_lossy().to_string());
            stmt.execute(params![project_id, ext, path])
                .map_err(|e| format!("insert project ext: {}", e))?;
        }
        Ok(())
    }
}
