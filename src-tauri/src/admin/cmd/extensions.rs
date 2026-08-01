use std::sync::Arc;

use pi_server::gateway::GatewayState;
use pi_server::gateway::project::{discover_project_extensions, discover_scope_extensions};

use crate::base::state::GatewaySlot;

/// Manage the global and per-project extension configuration stored in the
/// gateway's SQLite database (`global_extensions` / `project_extensions` tables).
///
/// These commands are read-only with respect to the filesystem: they record
/// which extensions Pi should use. Installing/uninstalling packages happens in
/// the Market tab (`install_pi_package` / `remove_pi_package`).

fn gw_db(gw: &Option<Arc<GatewayState>>) -> Result<std::sync::Arc<pi_server::gateway::db::Db>, String> {
    gw.as_ref()
        .map(|g| g.db.clone())
        .ok_or_else(|| "Pi gateway not available. Download Pi from Settings > Versions.".into())
}

/// A discovered extension candidate: name (DB key) and resolved entry path.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionEntryDto {
    pub name: String,
    pub path: Option<String>,
}

/// Per-project extension state for the overview.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ProjectExtensionState {
    pub id: String,
    pub name: String,
    pub cwd: String,
    /// Extension candidates discovered for this project (direct + packages).
    pub extensions: Vec<ExtensionEntryDto>,
    /// Extensions enabled in the database for this project.
    pub enabled: Vec<String>,
}

/// Full snapshot of the extension configuration: everything discovered on
/// disk (including installed package entry points) plus what is enabled,
/// for both global and project scopes.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub struct ExtensionOverview {
    /// Extension candidates discovered in the global scope.
    pub global_extensions: Vec<ExtensionEntryDto>,
    /// Extensions enabled globally (database `global_extensions`).
    pub enabled_global: Vec<String>,
    pub projects: Vec<ProjectExtensionState>,
}

/// Load the full extension overview (discovered + enabled) in one call.
#[tauri::command]
pub fn get_extension_overview(
    gw: tauri::State<'_, GatewaySlot>,
) -> Result<ExtensionOverview, String> {
    let db = gw_db(&gw.inner().lock())?;
    let agent_dir = pi_server::get_pi_agent_dir();
    let to_dto = |entries: Vec<pi_server::gateway::project::ExtensionEntry>| {
        entries
            .into_iter()
            .map(|e| ExtensionEntryDto {
                name: e.name,
                path: e.path.map(|p| p.to_string_lossy().to_string()),
            })
            .collect::<Vec<_>>()
    };
    let projects = db
        .list_projects(false)
        .into_iter()
        .map(|p| {
            let entries = discover_project_extensions(&agent_dir, &p.cwd);
            ProjectExtensionState {
                id: p.id.clone(),
                name: p.name,
                cwd: p.cwd.clone(),
                extensions: to_dto(entries),
                enabled: db.get_project_extensions(&p.id),
            }
        })
        .collect();
    Ok(ExtensionOverview {
        global_extensions: to_dto(discover_scope_extensions(
            &agent_dir.join("extensions"),
            &agent_dir,
            "",
        )),
        enabled_global: db.get_global_extensions(),
        projects,
    })
}

/// Replace the global extension list with the full enabled set.
#[tauri::command]
pub fn set_global_extensions(
    gw: tauri::State<'_, GatewaySlot>,
    extensions: Vec<String>,
) -> Result<(), String> {
    let db = gw_db(&gw.inner().lock())?;
    db.set_global_extensions(&extensions)
}

/// Replace the extension list configured for a project.
#[tauri::command]
pub fn set_project_extensions(
    gw: tauri::State<'_, GatewaySlot>,
    project_id: String,
    extensions: Vec<String>,
) -> Result<(), String> {
    let db = gw_db(&gw.inner().lock())?;
    db.set_project_extensions(&project_id, &extensions)
}
