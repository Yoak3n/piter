//! Per-dimension aggregation over accumulated state.

use std::collections::HashMap;
use std::path::Path;

use chrono::{DateTime, Duration, NaiveDate, Utc};

use super::parse::fmt_day;
use super::state::{Accum, SessionAgg};
use super::types::{DailyPoint, DayActivity, ModelStat, ProjectStat, SessionStat};

pub(crate) fn build_models(acc: &Accum) -> Vec<ModelStat> {
    let total: u64 = acc.models.values().map(|m| m.total_tokens).sum();
    let mut models: Vec<ModelStat> = acc
        .models
        .iter()
        .map(|(name, m)| ModelStat {
            name: name.clone(),
            total_tokens: m.total_tokens,
            input_tokens: m.input_tokens,
            output_tokens: m.output_tokens,
            cost: m.cost,
            fraction: if total > 0 {
                m.total_tokens as f64 / total as f64
            } else {
                0.0
            },
        })
        .collect();
    models.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    models
}

pub(crate) fn build_projects(acc: &Accum) -> Vec<ProjectStat> {
    let total: f64 = acc.projects.values().map(|p| p.cost).sum();
    let mut projects: Vec<ProjectStat> = acc
        .projects
        .iter()
        .map(|(cwd, p)| ProjectStat {
            name: project_basename(cwd),
            cwd: cwd.clone(),
            sessions: p.sessions,
            cost: p.cost,
            fraction: if total > 0.0 { p.cost / total } else { 0.0 },
        })
        .collect();
    projects.sort_by(|a, b| b.cost.partial_cmp(&a.cost).unwrap_or(std::cmp::Ordering::Equal));
    projects
}

pub(crate) fn build_sessions(sessions: &mut Vec<SessionAgg>) -> Vec<SessionStat> {
    let mut out: Vec<SessionStat> = sessions
        .iter()
        .map(|s| SessionStat {
            title: derive_title(s),
            workspace: s.cwd.clone().unwrap_or_default(),
            model: most_used_model(&s.model_tokens),
            total_tokens: s.total_tokens,
            tool_calls: s.tool_calls,
            total_cost: s.total_cost,
            time: s
                .start
                .map(|t| t.to_rfc3339())
                .unwrap_or_default(),
        })
        .collect();
    out.sort_by(|a, b| b.time.cmp(&a.time));
    out.truncate(50);
    out
}

pub(crate) fn build_daily(
    from_day: NaiveDate,
    to_day: NaiveDate,
    acc: &Accum,
    models: &[ModelStat],
) -> Vec<DailyPoint> {
    // Only the top models are carried into the daily series so the payload
    // stays bounded no matter how many models were used over 90 days.
    let top: Vec<&str> = models.iter().take(8).map(|m| m.name.as_str()).collect();

    let mut daily = Vec::new();
    let mut cursor = from_day;
    while cursor <= to_day {
        let key = cursor.format("%Y-%m-%d").to_string();
        let day_models = acc.day_models.get(&key);
        let models_map = day_models
            .map(|m| {
                m.iter()
                    .filter(|(name, _)| top.iter().any(|t| *t == name.as_str()))
                    .map(|(name, tokens)| (name.clone(), *tokens))
                    .collect::<HashMap<_, _>>()
            })
            .unwrap_or_default();
        daily.push(DailyPoint {
            key: key.clone(),
            total: acc.day_tokens.get(&key).copied().unwrap_or(0),
            models: models_map,
        });
        cursor += Duration::days(1);
    }
    daily
}

pub(crate) fn build_activity(
    activity_tokens: &HashMap<String, u64>,
    now: DateTime<Utc>,
) -> Vec<DayActivity> {
    let today = now.date_naive();
    let start = today - Duration::days(364);
    let mut activity = Vec::with_capacity(365);
    for i in 0..365 {
        let day = start + Duration::days(i);
        let key = day.format("%Y-%m-%d").to_string();
        activity.push(DayActivity {
            key: key.clone(),
            value: activity_tokens.get(&key).copied().unwrap_or(0),
        });
    }
    activity
}

pub(crate) fn compute_streaks(day_tokens: &HashMap<String, u64>, now: DateTime<Utc>) -> (u64, u64) {
    let mut keys: Vec<String> = day_tokens.keys().cloned().collect();
    keys.sort();

    let mut longest = 0u64;
    let mut run = 0u64;
    let mut prev: Option<NaiveDate> = None;
    for key in &keys {
        let Some(date) = NaiveDate::parse_from_str(key, "%Y-%m-%d").ok() else {
            continue;
        };
        match prev {
            Some(p) if date.signed_duration_since(p).num_days() == 1 => run += 1,
            _ => run = 1,
        }
        longest = longest.max(run);
        prev = Some(date);
    }

    let today = now.date_naive();
    let start = if day_tokens.contains_key(&fmt_day(today)) {
        today
    } else if day_tokens.contains_key(&fmt_day(today - Duration::days(1))) {
        today - Duration::days(1)
    } else {
        return (0, longest);
    };
    let mut current = 0u64;
    let mut cursor = start;
    while day_tokens.contains_key(&fmt_day(cursor)) {
        current += 1;
        cursor -= Duration::days(1);
    }
    (current, longest)
}

// ─── Derived value helpers ─────────────────────────────────────────────────

fn derive_title(sess: &SessionAgg) -> String {
    let text = sess
        .title_candidates
        .iter()
        .find(|t| !t.trim().is_empty())
        .map(|t| t.split_whitespace().collect::<Vec<_>>().join(" "))
        .unwrap_or_default();
    if text.is_empty() {
        return "Untitled".to_string();
    }
    let mut chars = text.chars();
    let truncated: String = chars.by_ref().take(60).collect();
    if chars.next().is_some() {
        let cut = truncated.rfind(' ').unwrap_or(truncated.len());
        format!("{}…", &truncated[..cut])
    } else {
        truncated
    }
}

fn most_used_model(model_tokens: &HashMap<String, u64>) -> String {
    model_tokens
        .iter()
        .max_by_key(|(_, tokens)| **tokens)
        .map(|(name, _)| name.clone())
        .unwrap_or_else(|| "—".to_string())
}

fn project_basename(cwd: &str) -> String {
    Path::new(cwd)
        .file_name()
        .and_then(|n| n.to_str())
        .filter(|n| !n.is_empty())
        .map(|n| n.to_string())
        .unwrap_or_else(|| cwd.to_string())
}
