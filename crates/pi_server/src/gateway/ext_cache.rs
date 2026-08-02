//! Extension candidate cache.
//!
//! The slow part of `get_extension_overview` is the disk scan (glob
//! expansion over `extensions/`, `npm/node_modules` and `git/` trees), so
//! candidates are cached per scope (`global` / each project id) and refreshed
//! in the background. Enabled/excluded lists are still read from the DB live
//! — only the discovered candidates are cached.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use super::project::{discover_project_extensions, discover_scope_extensions, ExtensionEntry};
use super::GatewayState;

/// Cache key for the global scope.
pub const GLOBAL_KEY: &str = "global";

fn agent_dir() -> std::path::PathBuf {
    crate::broker::util::get_pi_agent_dir()
}

/// Scan the global scope: `~/.pi/agent/extensions/` + installed packages.
fn scan_global() -> Vec<ExtensionEntry> {
    let dir = agent_dir();
    discover_scope_extensions(&dir.join("extensions"), &dir, "")
}

/// Merge project-local candidates into an already-scanned global list
/// (deduped by name) — avoids re-scanning the global scope per project.
fn project_from_global(cwd: &str, global: &[ExtensionEntry]) -> Vec<ExtensionEntry> {
    let proj_dir = Path::new(cwd).join(".pi");
    let proj = discover_scope_extensions(&proj_dir.join("extensions"), &proj_dir, cwd);
    let mut entries: Vec<ExtensionEntry> = global.to_vec();
    let mut seen: HashSet<String> = entries.iter().map(|e| e.name.clone()).collect();
    for e in proj {
        if seen.insert(e.name.clone()) {
            entries.push(e);
        }
    }
    entries
}

/// Rescan every scope (global + all projects) and update the cache.
/// Returns true when the cached snapshot changed, so callers can notify the
/// frontend. Runs on a blocking thread by callers.
pub fn refresh_all(state: &GatewayState) -> bool {
    let global = scan_global();
    let mut fresh: HashMap<String, Vec<ExtensionEntry>> = HashMap::new();
    fresh.insert(GLOBAL_KEY.to_string(), global.clone());
    for p in state.db.list_projects(false) {
        fresh.insert(p.id.clone(), project_from_global(&p.cwd, &global));
    }
    let changed = {
        let old = state.extension_cache.read();
        old.len() != fresh.len() || fresh.iter().any(|(k, v)| old.get(k) != Some(v))
    };
    if changed {
        *state.extension_cache.write() = fresh;
    }
    changed
}

/// Cached candidates for a scope; on a cache miss (cold start or a newly
/// created project) scan that scope synchronously and store it.
pub fn get_or_scan(state: &GatewayState, key: &str, cwd: Option<&str>) -> Vec<ExtensionEntry> {
    if let Some(v) = state.extension_cache.read().get(key) {
        return v.clone();
    }
    let entries = if key == GLOBAL_KEY {
        scan_global()
    } else if let Some(cwd) = cwd {
        // Project miss: reuse the cached global scan when available.
        let global = state.extension_cache.read().get(GLOBAL_KEY).cloned();
        match global {
            Some(g) => project_from_global(cwd, &g),
            None => discover_project_extensions(&agent_dir(), cwd),
        }
    } else {
        Vec::new()
    };
    state
        .extension_cache
        .write()
        .insert(key.to_string(), entries.clone());
    entries
}

/// Drop all cached scans (e.g. after a package install/uninstall changes the
/// on-disk layout). The next read rescans synchronously as a fallback.
pub fn invalidate_all(state: &GatewayState) {
    state.extension_cache.write().clear();
}
