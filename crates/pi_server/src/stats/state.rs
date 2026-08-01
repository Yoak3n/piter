//! Internal accumulation state shared by parsing and aggregation.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

#[derive(Default)]
pub(crate) struct Accum {
    pub(crate) total_cost: f64,
    pub(crate) messages: u64,
    pub(crate) total_tokens: u64,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read: u64,
    pub(crate) cache_write: u64,
    pub(crate) tool_calls: u64,
    pub(crate) session_count: u64,

    pub(crate) models: HashMap<String, ModelAgg>,
    pub(crate) tools: HashMap<String, ToolAgg>,
    pub(crate) projects: HashMap<String, ProjectAgg>,
    /// In-range day → total tokens (also drives active days / streaks).
    pub(crate) day_tokens: HashMap<String, u64>,
    /// In-range day → model → tokens.
    pub(crate) day_models: HashMap<String, HashMap<String, u64>>,
    /// All-time day → tokens (365-day heatmap).
    pub(crate) activity_tokens: HashMap<String, u64>,
    pub(crate) sessions: Vec<SessionAgg>,
}

#[derive(Default)]
pub(crate) struct ModelAgg {
    pub(crate) total_tokens: u64,
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cost: f64,
}

#[derive(Default)]
pub(crate) struct ToolAgg {
    pub(crate) count: u64,
    pub(crate) cost: f64,
}

#[derive(Default)]
pub(crate) struct ProjectAgg {
    pub(crate) sessions: u64,
    pub(crate) cost: f64,
}

#[derive(Default)]
pub(crate) struct SessionAgg {
    pub(crate) start: Option<DateTime<Utc>>,
    pub(crate) cwd: Option<String>,
    pub(crate) in_range: bool,
    pub(crate) total_tokens: u64,
    pub(crate) tool_calls: u64,
    pub(crate) total_cost: f64,
    pub(crate) model_tokens: HashMap<String, u64>,
    pub(crate) title_candidates: Vec<String>,
}

#[derive(Default)]
pub(crate) struct UsageBits {
    pub(crate) input_tokens: u64,
    pub(crate) output_tokens: u64,
    pub(crate) cache_read: u64,
    pub(crate) cache_write: u64,
    pub(crate) total_tokens: u64,
    pub(crate) cost: f64,
}
