use std::sync::Arc;

use pi_server::gateway::ext_cache;
use pi_server::gateway::GatewayState;
use tauri::Emitter;

use crate::base::state::GatewaySlot;

/// Manage the global and per-project extension configuration stored in the
/// gateway's SQLite database (`global_extensions` / `project_added_extensions`
/// / `project_excluded_extensions` tables).
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
    /// Extensions this project adds on top of the global list.
    pub added: Vec<String>,
    /// Extensions explicitly excluded for this project (never loaded).
    pub excluded: Vec<String>,
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
///
/// Discovered candidates come from the cache (warmed at gateway startup and
/// refreshed in the background); enabled/excluded lists are read live from the
/// DB. Returns immediately from the snapshot, then triggers a background
/// rescan — if it differs, an `extension_overview_updated` event is emitted so
/// open Installed tabs refresh themselves.
#[tauri::command]
pub fn get_extension_overview(
    app: tauri::AppHandle,
    gw: tauri::State<'_, GatewaySlot>,
) -> Result<ExtensionOverview, String> {
    let state = gw
        .inner()
        .lock()
        .clone()
        .ok_or_else(|| "Pi gateway not available. Download Pi from Settings > Versions.".to_string())?;
    let db = state.db.clone();
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
        .map(|p| ProjectExtensionState {
            id: p.id.clone(),
            name: p.name,
            cwd: p.cwd.clone(),
            // Candidates are lazily loaded per project when the user selects
            // it (see `get_project_extension_overview`).
            extensions: Vec::new(),
            added: db.get_project_added_extensions(&p.id),
            excluded: db.get_project_excluded_extensions(&p.id),
        })
        .collect();
    let global = ext_cache::get_or_scan(&state, ext_cache::GLOBAL_KEY, None);
    let overview = ExtensionOverview {
        global_extensions: to_dto(global),
        enabled_global: db.get_global_extensions(),
        projects,
    };

    // Background rescan: diff against the snapshot and notify on change.
    refresh_extension_cache(state, app);

    Ok(overview)
}

/// Lazily load a single project's extension candidates (disk scan is the slow
/// part, so it only runs when the user actually selects the project).
/// `added`/`excluded` are read live from the DB.
#[tauri::command]
pub fn get_project_extension_overview(
    gw: tauri::State<'_, GatewaySlot>,
    project_id: String,
) -> Result<ProjectExtensionState, String> {
    let state = gw
        .inner()
        .lock()
        .clone()
        .ok_or_else(|| "Pi gateway not available. Download Pi from Settings > Versions.".to_string())?;
    let db = state.db.clone();
    let proj = db
        .get_project(&project_id)
        .ok_or_else(|| format!("project not found: {}", project_id))?;
    let entries = ext_cache::get_or_scan(&state, &project_id, Some(&proj.cwd));
    let to_dto = |entries: Vec<pi_server::gateway::project::ExtensionEntry>| {
        entries
            .into_iter()
            .map(|e| ExtensionEntryDto {
                name: e.name,
                path: e.path.map(|p| p.to_string_lossy().to_string()),
            })
            .collect::<Vec<_>>()
    };
    Ok(ProjectExtensionState {
        id: proj.id,
        name: proj.name,
        cwd: proj.cwd,
        extensions: to_dto(entries),
        added: db.get_project_added_extensions(&project_id),
        excluded: db.get_project_excluded_extensions(&project_id),
    })
}

/// Rescan all scopes off the current thread and emit an
/// `extension_overview_updated` event when the snapshot changed.
/// Never blocks the caller — scanning runs on its own thread.
pub fn refresh_extension_cache(state: Arc<GatewayState>, app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let changed = ext_cache::refresh_all(&state);
        if changed {
            let _ = app.emit("extension_overview_updated", ());
        }
    });
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

/// Replace the extension list a project adds on top of the global list.
#[tauri::command]
pub fn set_project_added_extensions(
    gw: tauri::State<'_, GatewaySlot>,
    project_id: String,
    extensions: Vec<String>,
) -> Result<(), String> {
    let db = gw_db(&gw.inner().lock())?;
    db.set_project_added_extensions(&project_id, &extensions)
}

/// Replace the extensions explicitly excluded for a project (never loaded,
/// even when enabled globally).
#[tauri::command]
pub fn set_project_excluded_extensions(
    gw: tauri::State<'_, GatewaySlot>,
    project_id: String,
    extensions: Vec<String>,
) -> Result<(), String> {
    let db = gw_db(&gw.inner().lock())?;
    db.set_project_excluded_extensions(&project_id, &extensions)
}
