//! Usage dashboard — aggregates pi session files (usage/cost) for the
//! admin "Usage" tab. Mirrors Picot's `/api/cost-dashboard` payload shape.
//!
//! Only session files registered in piter's DB (i.e. sessions accepted for
//! management) are aggregated — stray session files created by other clients
//! are ignored.

use std::path::PathBuf;

use pi_server::stats::{self, RangePreset, Scope};
use tauri::{AppHandle, Manager, State};

use crate::base::state::GatewaySlot;

/// Return aggregated usage/cost stats for the requested range.
///
/// Params mirror Picot's `/api/cost-dashboard`:
/// - `range`: "7d" | "30d" | "90d" (default "30d")
/// - `granularity`: "day" (the only supported granularity for now)
/// - `scope`: "all" | "current" (default "all"). "current" restricts to the
///   most recently active project's working directory when the gateway is up.
#[tauri::command]
pub async fn get_cost_dashboard(
    app: AppHandle,
    gw: State<'_, GatewaySlot>,
    range: Option<String>,
    granularity: Option<String>,
    scope: Option<String>,
) -> Result<serde_json::Value, String> {
    let range = RangePreset::from_str(range.as_deref().unwrap_or("30d"));
    let scope = Scope::from_str(scope.as_deref().unwrap_or("all"));
    let _ = granularity; // only "day" is produced today

    // "current" scope = sessions under the most recently created session's cwd.
    let current_cwd = if scope == Scope::Current {
        gw.inner()
            .lock()
            .as_ref()
            .and_then(|g| {
                g.db
                    .all_sessions()
                    .into_iter()
                    .max_by_key(|s| s.created_at.clone())
            })
            .map(|s| s.cwd)
            .filter(|cwd| !cwd.is_empty())
    } else {
        None
    };

    // Sessions piter manages: prefer the live gateway DB, fall back to opening
    // the DB file directly so stats stay accurate even without a running pi.
    // DB paths are read directly (no directory scan); when the DB is entirely
    // unreadable the aggregation falls back to scanning the sessions dir.
    let files = resolve_managed_sessions(&app, gw.inner());

    let sessions_dir = pi_server::get_pi_agent_dir().join("sessions");
    let result = tokio::task::spawn_blocking(move || {
        stats::build_dashboard(&sessions_dir, range, scope, current_cwd.as_deref(), files)
    })
    .await
    .map_err(|e| format!("stats task join error: {e}"))?;

    result
        .map(|dashboard| {
            serde_json::to_value(dashboard).map_err(|e| format!("serialize stats: {e}"))
        })?
}

/// File paths of all sessions registered in piter's DB.
///
/// Returns `None` only when the DB cannot be read at all (neither the live
/// gateway nor the on-disk file) — the caller then falls back to aggregating
/// everything it scans. Any other result (including an empty list) strictly
/// limits aggregation to registered sessions.
fn resolve_managed_sessions(app: &AppHandle, gw: &GatewaySlot) -> Option<Vec<PathBuf>> {
    let sessions: Vec<String> = if let Some(gw) = gw.lock().as_ref() {
        gw.db
            .all_sessions()
            .into_iter()
            .filter_map(|s| s.session_path)
            .collect()
    } else {
        let data_dir = app.path().app_data_dir().ok()?;
        match pi_server::gateway::db::Db::open(&data_dir) {
            Ok(db) => db
                .all_sessions()
                .into_iter()
                .filter_map(|s| s.session_path)
                .collect(),
            Err(e) => {
                log::warn!("[stats] cannot read piter DB at {}: {e}", data_dir.display());
                return None;
            }
        }
    };

    Some(sessions.into_iter().map(PathBuf::from).collect())
}
