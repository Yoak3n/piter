//! Query parameters and the response payload types.
//!
//! Field names are snake_case, mirroring Picot's `cost-dashboard` payload.

use std::collections::HashMap;

use serde::Serialize;

// ─── Query parameters ──────────────────────────────────────────────────────

/// Time-range presets, matching Picot's `/api/cost-dashboard?range=`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RangePreset {
    D7,
    D30,
    D90,
}

impl RangePreset {
    pub fn from_str(s: &str) -> Self {
        match s {
            "7d" => RangePreset::D7,
            "90d" => RangePreset::D90,
            _ => RangePreset::D30,
        }
    }

    pub fn days(self) -> i64 {
        match self {
            RangePreset::D7 => 7,
            RangePreset::D30 => 30,
            RangePreset::D90 => 90,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            RangePreset::D7 => "7d",
            RangePreset::D30 => "30d",
            RangePreset::D90 => "90d",
        }
    }
}

/// Scope of the aggregation. `Current` restricts to sessions whose cwd matches
/// the provided `current_cwd`; when no cwd is supplied it degrades to `All`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    All,
    Current,
}

impl Scope {
    pub fn from_str(s: &str) -> Self {
        if s.eq_ignore_ascii_case("current") {
            Scope::Current
        } else {
            Scope::All
        }
    }
}

// ─── Payload types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Default, Serialize)]
pub struct UsageDashboard {
    pub range: RangeInfo,
    pub overview: Overview,
    pub usage: Usage,
    pub models: Vec<ModelStat>,
    pub projects: Vec<ProjectStat>,
    pub sessions: Vec<SessionStat>,
    /// Per-day totals + per-model token split within the selected range.
    pub daily: Vec<DailyPoint>,
    /// Per-day token totals for the last 365 days (activity heatmap).
    pub activity: Vec<DayActivity>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RangeInfo {
    pub range: String,
    pub from: String,
    pub to: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Overview {
    pub total_cost: f64,
    pub sessions: u64,
    pub messages: u64,
    pub total_tokens: u64,
    pub active_days: u64,
    pub current_streak: u64,
    pub longest_streak: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub tool_calls: u64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct Usage {
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read: u64,
    pub cache_write: u64,
    pub tool_calls: u64,
    pub tools: Vec<ToolStat>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ToolStat {
    pub name: String,
    pub count: u64,
    pub cost: f64,
    /// Cost share relative to all tools (0..=1).
    pub fraction: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ModelStat {
    pub name: String,
    pub total_tokens: u64,
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cost: f64,
    /// Token share relative to all models (0..=1).
    pub fraction: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ProjectStat {
    pub name: String,
    pub cwd: String,
    pub sessions: u64,
    pub cost: f64,
    pub fraction: f64,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SessionStat {
    pub title: String,
    pub workspace: String,
    pub model: String,
    pub total_tokens: u64,
    pub tool_calls: u64,
    pub total_cost: f64,
    pub time: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DailyPoint {
    pub key: String,
    pub total: u64,
    pub models: HashMap<String, u64>,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct DayActivity {
    pub key: String,
    pub value: u64,
}
