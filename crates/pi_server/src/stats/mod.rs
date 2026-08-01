//! Usage/cost dashboard — aggregates pi session files into stats payloads.
//!
//! Session files live in `<pi agent dir>/sessions/**/*.jsonl`. Every line is a
//! JSON event; `message` events carry a `message` object with `usage`/`cost`
//! fields (Picot-compatible), e.g.:
//!
//! ```json
//! {"type":"message","timestamp":"...","message":{
//!   "role":"assistant","model":"deepseek-v4-flash",
//!   "usage":{"input":1,"output":2,"cacheRead":3,"cacheWrite":0,"totalTokens":6,
//!            "cost":{"input":..,"output":..,"cacheRead":..,"cacheWrite":..,"total":..}}
//! }}
//! ```
//!
//! This module is pure (no gateway dependencies) so the web standalone build
//! can later expose the same aggregation over REST.
//!
//! Layout:
//! - [`types`] — query parameters and the response payload types
//! - [`state`] — internal accumulation state
//! - [`parse`] — session file parsing
//! - [`aggregate`] — per-dimension aggregation

pub mod types;
mod aggregate;
mod parse;
mod state;

pub use types::*;

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};

use aggregate::{
    build_activity, build_daily, build_models, build_projects, build_sessions, compute_streaks,
};
use parse::parse_session_file;
use state::Accum;

// ─── Entry point ───────────────────────────────────────────────────────────

/// Aggregate pi session files for the given query.
///
/// `current_cwd` is only used when `scope == Scope::Current`; if absent the
/// scope degrades to `All`. A missing sessions directory yields an empty
/// dashboard (not an error).
///
/// `files` lists the exact session files to aggregate — the caller resolves
/// them (e.g. from piter's DB, i.e. only sessions accepted for management),
/// so we read those paths directly instead of scanning the whole directory.
/// When `files` is `None` the directory is scanned as a fallback (DB
/// unavailable), aggregating every session file found.
pub fn build_dashboard(
    sessions_dir: &Path,
    range: RangePreset,
    scope: Scope,
    current_cwd: Option<&str>,
    files: Option<Vec<PathBuf>>,
) -> Result<UsageDashboard, String> {
    build_dashboard_at(sessions_dir, range, scope, current_cwd, files, Utc::now())
}

/// Testable variant of [`build_dashboard`] with an injected clock.
pub(crate) fn build_dashboard_at(
    sessions_dir: &Path,
    range: RangePreset,
    scope: Scope,
    current_cwd: Option<&str>,
    files: Option<Vec<PathBuf>>,
    now: DateTime<Utc>,
) -> Result<UsageDashboard, String> {
    let from = now - Duration::days(range.days());
    let from_day = from.date_naive();
    let to_day = now.date_naive();

    let mut acc = Accum::default();

    // Prefer the caller-resolved file list (e.g. DB-registered sessions) so we
    // never scan unrelated files; fall back to a full directory scan.
    let files: Vec<PathBuf> = match files {
        Some(files) => files.into_iter().collect::<HashSet<_>>().into_iter().collect(),
        None => walk_jsonl(sessions_dir),
    };

    for file in files {
        if let Err(e) = parse_session_file(&file, from, scope, current_cwd, &mut acc) {
            log::warn!("[stats] skipping {}: {}", file.display(), e);
        }
    }

    let total_tool_cost: f64 = acc.tools.values().map(|t| t.cost).sum();

    let models = build_models(&acc);
    let projects = build_projects(&acc);
    let sessions = build_sessions(&mut acc.sessions);
    let daily = build_daily(from_day, to_day, &acc, &models);
    let activity = build_activity(&acc.activity_tokens, now);
    let (current_streak, longest_streak) = compute_streaks(&acc.day_tokens, now);

    let overview = Overview {
        total_cost: acc.total_cost,
        sessions: acc.session_count,
        messages: acc.messages,
        total_tokens: acc.total_tokens,
        active_days: acc.day_tokens.len() as u64,
        current_streak,
        longest_streak,
        input_tokens: acc.input_tokens,
        output_tokens: acc.output_tokens,
        cache_read: acc.cache_read,
        cache_write: acc.cache_write,
        tool_calls: acc.tool_calls,
    };

    let usage = Usage {
        total_tokens: acc.total_tokens,
        input_tokens: acc.input_tokens,
        output_tokens: acc.output_tokens,
        cache_read: acc.cache_read,
        cache_write: acc.cache_write,
        tool_calls: acc.tool_calls,
        tools: {
            let mut tools: Vec<ToolStat> = acc
                .tools
                .into_iter()
                .map(|(name, t)| ToolStat {
                    name,
                    count: t.count,
                    cost: t.cost,
                    fraction: if total_tool_cost > 0.0 {
                        t.cost / total_tool_cost
                    } else {
                        0.0
                    },
                })
                .collect();
            tools.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
            tools
        },
    };

    Ok(UsageDashboard {
        range: RangeInfo {
            range: range.as_str().to_string(),
            from: from_day.format("%Y-%m-%d").to_string(),
            to: to_day.format("%Y-%m-%d").to_string(),
        },
        overview,
        usage,
        models,
        projects,
        sessions,
        daily,
        activity,
    })
}

// ─── File discovery ────────────────────────────────────────────────────────

fn walk_jsonl(dir: &Path) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if !dir.is_dir() {
        return out;
    }
    let mut stack = vec![dir.to_path_buf()];
    while let Some(current) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(&current) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else if path.extension().and_then(|e| e.to_str()) == Some("jsonl") {
                out.push(path);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests;
