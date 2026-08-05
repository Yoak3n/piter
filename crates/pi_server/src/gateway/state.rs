use crate::GatewayState;
use super::{
    RuntimeSessionInfo, session_manager,
    db::SessionRow, handlers, project::list_projects
};


/// Build project-session tree from database + session file metadata + runtime state.
pub fn build_project_session_tree(state: &GatewayState) -> Vec<handlers::ProjectGroup> {
    use handlers::{ProjectGroup, SessionInfo};
    use std::collections::HashMap;

    // Build lookup: session_file_path → (instance_id, state_info) from session manager
    let mgr = state.session_manager.lock();
    let mut runtime_by_iid: HashMap<String, RuntimeSessionInfo> = HashMap::new();
    for session in mgr.sessions.values() {
        let info = RuntimeSessionInfo {
            state: match &session.activity {
                session_manager::SessionActivity::Idle => "idle".to_string(),
                session_manager::SessionActivity::Busy => "busy".to_string(),
                session_manager::SessionActivity::WaitingReview => "waiting_review".to_string(),
                session_manager::SessionActivity::Unloaded => "unloaded".to_string(),
            },
            model: session.pi_state.as_ref().and_then(|p| p.model_id.clone()),
            model_provider: session.pi_state.as_ref().and_then(|p| p.model_provider.clone()),
            thinking_level: session.pi_state.as_ref().and_then(|p| p.thinking_level.clone()),
            message_count: session.messages.len() as u32,
            message_seq: session.message_seq,
            session_name: session.session_name.clone(),
            last_active_epoch: session.last_active_epoch,
        };
        runtime_by_iid.insert(session.instance_id.clone(), info);
    }
    drop(mgr);

    // Single DB query for all sessions (avoid O(n²))
    let all_db_sessions = state.db.all_sessions();
    let db_by_iid: HashMap<String, SessionRow> = all_db_sessions
        .into_iter()
        .map(|s| (s.instance_id.clone(), s))
        .collect();

    let db_projects = list_projects(&state.db, true);

    let mut result: Vec<ProjectGroup> = Vec::new();
    let mut archived_result: Vec<ProjectGroup> = Vec::new();

    for proj in &db_projects {
        let instance_ids = state.db.get_project_sessions(&proj.id);
        let mut sessions: Vec<SessionInfo> = Vec::new();

        for iid in &instance_ids {
            let rt = runtime_by_iid.get(iid);
            let db_row = db_by_iid.get(iid);

            // Label: runtime auto-title > DB name > instance id fallback
            let label = rt
                .and_then(|r| r.session_name.clone())
                .or_else(|| db_row.and_then(|r| r.name.clone()))
                .unwrap_or_else(|| iid.chars().take(8).collect());

            let state_str = rt
                .map(|r| r.state.clone())
                .unwrap_or_else(|| "unloaded".to_string());

            sessions.push(SessionInfo {
                id: iid.clone(),
                label,
                created_at: String::new(),
                file_path: db_row
                    .and_then(|r| r.session_path.clone())
                    .unwrap_or_default(),
                updated_at: rt.map(|r| r.last_active_epoch).unwrap_or_else(|| {
                    // Parse DB created_at RFC3339 string to epoch
                    db_row.and_then(|r| chrono::DateTime::parse_from_rfc3339(&r.created_at).ok())
                        .map(|dt| dt.timestamp() as u64)
                        .unwrap_or(0)
                }),
                preview: String::new(),
                cwd: proj.cwd.clone(),
                instance_id: Some(iid.clone()),
                state: Some(state_str),
                // Runtime state wins; fall back to the persisted DB model so a
                // session's own model survives a gateway restart.
                model: rt
                    .and_then(|r| r.model.clone())
                    .or_else(|| db_row.and_then(|r| r.model_id.clone())),
                model_provider: rt
                    .and_then(|r| r.model_provider.clone())
                    .or_else(|| db_row.and_then(|r| r.model_provider.clone())),
                thinking_level: rt.and_then(|r| r.thinking_level.clone()),
                message_count: rt.map(|r| r.message_count).unwrap_or(0),
                message_seq: rt.map(|r| r.message_seq).unwrap_or(0),
            });
        }

        sessions.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));

        let group = ProjectGroup {
            path: proj.cwd.clone(),
            name: proj.name.clone(),
            id: Some(proj.id.clone()),
            pinned: proj.pinned,
            archived: proj.archived,
            sessions,
        };
        if proj.archived {
            archived_result.push(group);
        } else {
            result.push(group);
        }
    }

    // Orphaned sessions (in DB but no project)
    let all_linked: std::collections::HashSet<String> = result
        .iter()
        .flat_map(|p| p.sessions.iter().filter_map(|s| s.instance_id.clone()))
        .collect();

    let orphans: Vec<SessionInfo> = db_by_iid
        .values()
        .filter(|s| s.project_id.is_none() && !all_linked.contains(&s.instance_id))
        .map(|s| {
            let rt = runtime_by_iid.get(&s.instance_id);
            let label = s.name.clone()
                .or_else(|| rt.and_then(|r| r.session_name.clone()))
                .unwrap_or_else(|| s.instance_id.chars().take(8).collect());
            SessionInfo {
                id: s.instance_id.clone(),
                label,
                created_at: String::new(),
                file_path: s.session_path.clone().unwrap_or_default(),
                updated_at: rt.map(|r| r.message_count as u64).unwrap_or(0),
                preview: String::new(),
                cwd: s.cwd.clone(),
                instance_id: Some(s.instance_id.clone()),
                state: Some(rt.map(|r| r.state.clone()).unwrap_or_else(|| "unloaded".to_string())),
                model: rt.and_then(|r| r.model.clone()).or_else(|| s.model_id.clone()),
                model_provider: rt
                    .and_then(|r| r.model_provider.clone())
                    .or_else(|| s.model_provider.clone()),
                thinking_level: None,
                message_count: rt.map(|r| r.message_count).unwrap_or(0),
                message_seq: rt.map(|r| r.message_seq).unwrap_or(0),
            }
        })
        .collect();

    if !orphans.is_empty() {
        result.push(ProjectGroup {
            path: String::new(),
            name: "Other".to_string(),
            id: None,
            pinned: 0,
            archived: false,
            sessions: orphans,
        });
    }

    // Archived projects stay visible but are grouped at the very end under the
    // "Archive" section by the frontend, so they can be restored anytime.
    result.extend(archived_result);

    result
}

