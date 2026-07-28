//! Project and session metadata management.
//!
//! All metadata is stored in SQLite (`~/.pi/agent/piter.db`).
//! Extension names are resolved to file paths at spawn time.
//!
//! Extension resolution order (per name):
//! 1. `~/.pi/agent/extensions/<name>.ts` (global)
//! 2. `~/.pi/agent/extensions/<name>/index.ts` (global directory)
//! 3. `<cwd>/.pi/extensions/<name>.ts` (project-local)
//! 4. `<cwd>/.pi/extensions/<name>/index.ts` (project-local directory)

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::db::Db;

// ─── Types ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub cwd: String,
    #[serde(default)]
    pub extensions: Vec<String>,
    #[serde(default)]
    pub pinned: i32,
    #[serde(default)]
    pub archived: bool,
    pub created_at: String,
    pub updated_at: String,
}

// ─── Project CRUD (delegates to db) ─────────────────────────────────────────

pub fn create_project(db: &Db, name: &str, cwd: &str, extensions: Vec<String>) -> Result<Project, String> {
    let id = uuid::Uuid::new_v4().to_string();
    db.create_project(&id, name, cwd)?;
    if !extensions.is_empty() {
        db.update_project(&id, None, Some(&extensions))?;
    }
    let now = chrono::Utc::now().to_rfc3339();
    Ok(Project {
        id,
        name: name.to_string(),
        cwd: cwd.to_string(),
        extensions,
        pinned: 0,
        archived: false,
        created_at: now.clone(),
        updated_at: now,
    })
}

pub fn update_project(db: &Db, id: &str, name: Option<&str>, extensions: Option<Vec<String>>) -> Result<Project, String> {
    db.update_project(id, name, extensions.as_deref())?;
    let row = db.get_project(id).ok_or_else(|| format!("project not found: {}", id))?;
    let exts = db.get_project_extensions(id);
    Ok(Project {
        id: row.id,
        name: row.name,
        cwd: row.cwd,
        extensions: exts,
        pinned: row.pinned,
        archived: row.archived,
        created_at: row.created_at,
        updated_at: row.updated_at,
    })
}

pub fn delete_project(db: &Db, id: &str) -> Result<(), String> {
    db.delete_project(id)
}

pub fn list_projects(db: &Db, include_archived: bool) -> Vec<Project> {
    db.list_projects(include_archived)
        .into_iter()
        .map(|row| {
            let exts = db.get_project_extensions(&row.id);
            Project {
                id: row.id,
                name: row.name,
                cwd: row.cwd,
                extensions: exts,
                pinned: row.pinned,
                archived: row.archived,
                created_at: row.created_at,
                updated_at: row.updated_at,
            }
        })
        .collect()
}

// ─── Extension Resolution ───────────────────────────────────────────────────

/// Resolve an extension name to a file path.
///
/// Search order:
/// 1. `~/.pi/agent/extensions/<name>.ts` (global single file)
/// 2. `~/.pi/agent/extensions/<name>/index.ts` (global directory)
/// 3. `<cwd>/.pi/extensions/<name>.ts` (project-local single file)
/// 4. `<cwd>/.pi/extensions/<name>/index.ts` (project-local directory)
pub fn resolve_extension_name(name: &str, cwd: &str) -> Option<PathBuf> {
    let global_dir = crate::broker::util::get_pi_agent_dir().join("extensions");

    let p = global_dir.join(format!("{}.ts", name));
    if p.is_file() {
        return Some(p);
    }

    let p = global_dir.join(name).join("index.ts");
    if p.is_file() {
        return Some(p);
    }

    let p = Path::new(cwd).join(".pi").join("extensions").join(format!("{}.ts", name));
    if p.is_file() {
        return Some(p);
    }

    let p = Path::new(cwd).join(".pi").join("extensions").join(name).join("index.ts");
    if p.is_file() {
        return Some(p);
    }

    None
}

/// Resolve project extensions to file paths for passing to pi via `-e` flags.
///
/// Only resolves project-specific extensions. Global extensions (from `settings.json`
/// `packages`) are loaded automatically by pi and should NOT be passed via `-e`.
pub fn resolve_project_extensions(db: &super::db::Db, project_id: &str, cwd: &str) -> Vec<String> {
    let names = db.get_project_extensions(project_id);
    let mut paths = Vec::new();
    for name in &names {
        match resolve_extension_name(name, cwd) {
            Some(p) => paths.push(p.to_string_lossy().to_string()),
            None => {
                log::warn!(
                    "[project] extension '{}' not found (project={}, cwd={})",
                    name, project_id, cwd
                );
            }
        }
    }
    paths
}
