//! Session file parsing: read a `.jsonl` and fold its events into `Accum`.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;

use super::state::{Accum, SessionAgg, UsageBits};
use super::types::Scope;

pub(crate) fn parse_session_file(
    path: &Path,
    from: DateTime<Utc>,
    scope: Scope,
    current_cwd: Option<&str>,
    acc: &mut Accum,
) -> Result<(), String> {
    let file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let reader = BufReader::new(file);

    let mut sess = SessionAgg::default();
    let mut current_model: Option<String> = None;

    for line in reader.lines() {
        let line = line.map_err(|e| format!("read line: {e}"))?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let Some(etype) = value.get("type").and_then(|t| t.as_str()) else {
            continue;
        };

        match etype {
            "session" => {
                sess.start = parse_ts(&value);
                sess.cwd = value.get("cwd").and_then(|c| c.as_str()).map(String::from);
                if scope == Scope::Current {
                    if let Some(cwd) = current_cwd {
                        if sess.cwd.as_deref() != Some(cwd) {
                            return Ok(()); // not part of the current project
                        }
                    }
                }
            }
            "model_change" => {
                current_model = value
                    .get("modelId")
                    .and_then(|m| m.as_str())
                    .or_else(|| value.get("model").and_then(|m| m.as_str()))
                    .map(String::from);
            }
            "message" => {
                let Some(ts) = parse_ts(&value) else { continue };
                let Some(msg) = value.get("message") else { continue };
                let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
                if role != "user" && role != "assistant" {
                    continue;
                }

                // First user message becomes the session title regardless of
                // whether it falls inside the selected range.
                if role == "user" && sess.title_candidates.len() < 8 {
                    let text = extract_text(msg);
                    if !text.trim().is_empty() {
                        sess.title_candidates.push(text);
                    }
                }

                // All-time activity (heatmap): token intensity by day.
                if let Some(tokens) = msg_usage_total(msg) {
                    *acc
                        .activity_tokens
                        .entry(day_key(ts))
                        .or_default() += tokens;
                }

                if ts < from {
                    continue;
                }

                let usage = parse_usage(msg);
                let model = msg
                    .get("model")
                    .and_then(|m| m.as_str())
                    .or(current_model.as_deref())
                    .unwrap_or("unknown")
                    .to_string();

                acc.messages += 1;
                acc.total_tokens += usage.total_tokens;
                acc.input_tokens += usage.input_tokens;
                acc.output_tokens += usage.output_tokens;
                acc.cache_read += usage.cache_read;
                acc.cache_write += usage.cache_write;
                acc.total_cost += usage.cost;

                let day = day_key(ts);
                *acc.day_tokens.entry(day.clone()).or_default() += usage.total_tokens;
                if usage.total_tokens > 0 {
                    *acc
                        .day_models
                        .entry(day)
                        .or_default()
                        .entry(model.clone())
                        .or_default() += usage.total_tokens;
                }

                let model_agg = acc.models.entry(model.clone()).or_default();
                model_agg.total_tokens += usage.total_tokens;
                model_agg.input_tokens += usage.input_tokens;
                model_agg.output_tokens += usage.output_tokens;
                model_agg.cost += usage.cost;

                let tools = collect_tools(msg);
                if !tools.is_empty() {
                    acc.tool_calls += tools.len() as u64;
                    // Split each message's cost evenly across the tools it
                    // called so the sum of tool costs never double-counts.
                    let per_tool = usage.cost / tools.len() as f64;
                    for tool in &tools {
                        let tool_agg = acc.tools.entry(tool.clone()).or_default();
                        tool_agg.count += 1;
                        tool_agg.cost += per_tool;
                    }
                }

                sess.in_range = true;
                sess.total_tokens += usage.total_tokens;
                sess.tool_calls += tools.len() as u64;
                sess.total_cost += usage.cost;
                *sess.model_tokens.entry(model).or_default() += usage.total_tokens;
            }
            _ => {}
        }
    }

    if !sess.in_range {
        return Ok(());
    }

    acc.session_count += 1;
    if let Some(cwd) = &sess.cwd {
        let project = acc.projects.entry(cwd.clone()).or_default();
        project.sessions += 1;
        project.cost += sess.total_cost;
    }
    acc.sessions.push(sess);

    Ok(())
}

// ─── Field parsing helpers ─────────────────────────────────────────────────

fn parse_ts(value: &Value) -> Option<DateTime<Utc>> {
    let s = value.get("timestamp")?.as_str()?;
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.with_timezone(&Utc))
}

fn day_key(ts: DateTime<Utc>) -> String {
    fmt_day(ts.date_naive())
}

/// Format a date as the `YYYY-MM-DD` day key used across the aggregation.
pub(crate) fn fmt_day(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn msg_usage_total(msg: &Value) -> Option<u64> {
    msg.get("usage")?.get("totalTokens")?.as_u64()
}

fn parse_usage(msg: &Value) -> UsageBits {
    let mut bits = UsageBits::default();
    let Some(usage) = msg.get("usage") else {
        return bits;
    };
    bits.input_tokens = usage.get("input").and_then(|v| v.as_u64()).unwrap_or(0);
    bits.output_tokens = usage.get("output").and_then(|v| v.as_u64()).unwrap_or(0);
    bits.cache_read = usage.get("cacheRead").and_then(|v| v.as_u64()).unwrap_or(0);
    bits.cache_write = usage.get("cacheWrite").and_then(|v| v.as_u64()).unwrap_or(0);
    bits.total_tokens = usage.get("totalTokens").and_then(|v| v.as_u64()).unwrap_or(0);
    bits.cost = usage
        .get("cost")
        .and_then(|c| c.get("total"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    bits
}

/// Collect distinct tool names called by a message. Tools may appear as
/// `message.toolCall` or as `content[]` blocks with `type == "toolCall"`.
fn collect_tools(msg: &Value) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(name) = msg
        .get("toolCall")
        .and_then(|t| t.get("name"))
        .and_then(|n| n.as_str())
    {
        out.push(name.to_string());
    }
    if let Some(blocks) = msg.get("content").and_then(|c| c.as_array()) {
        for block in blocks {
            if block.get("type").and_then(|t| t.as_str()) == Some("toolCall") {
                if let Some(name) = block.get("name").and_then(|n| n.as_str()) {
                    out.push(name.to_string());
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// Extract plain text from a message (string content or text content blocks).
fn extract_text(msg: &Value) -> String {
    let Some(content) = msg.get("content") else {
        return String::new();
    };
    if let Some(s) = content.as_str() {
        return s.to_string();
    }
    if let Some(blocks) = content.as_array() {
        return blocks
            .iter()
            .filter(|b| b.get("type").and_then(|t| t.as_str()) == Some("text"))
            .filter_map(|b| b.get("text").and_then(|t| t.as_str()))
            .collect::<Vec<_>>()
            .join("\n");
    }
    String::new()
}
