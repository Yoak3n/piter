//! Unit tests for the usage dashboard aggregation.

use std::fs;
use std::path::PathBuf;

use chrono::{DateTime, Utc};
use tempfile::TempDir;

use super::*;

const NOW: &str = "2026-07-31T12:00:00Z";

fn header(cwd: &str, ts: &str) -> String {
    format!(
        r#"{{"type":"session","version":3,"id":"s","timestamp":"{ts}","cwd":"{cwd}"}}"#
    )
}

fn model_change(model: &str) -> String {
    format!(
        r#"{{"type":"model_change","id":"m","parentId":null,"timestamp":"2026-07-10T00:00:00Z","provider":"p","modelId":"{model}"}}"#
    )
}

#[allow(clippy::too_many_arguments)]
fn message(
    ts: &str,
    role: &str,
    model: &str,
    input: u64,
    output: u64,
    cache_read: u64,
    cache_write: u64,
    total: u64,
    cost: f64,
    tools: &[&str],
) -> String {
    let tool_blocks = tools
        .iter()
        .map(|t| format!(r#"{{"type":"toolCall","id":"x","name":"{t}","arguments":{{}}}}"#))
        .collect::<Vec<_>>()
        .join(",");
    let content = if tool_blocks.is_empty() {
        if role == "user" {
            r#"{"type":"text","text":"Hello there project"}"#.to_string()
        } else {
            r#"{"type":"text","text":"ok"}"#.to_string()
        }
    } else {
        format!(r#"{{"type":"text","text":"ok"}},{tool_blocks}"#)
    };
    format!(
        r#"{{"type":"message","id":"msg","parentId":null,"timestamp":"{ts}","message":{{"role":"{role}","content":[{content}],"api":"openai-completions","provider":"p","model":"{model}","usage":{{"input":{input},"output":{output},"cacheRead":{cache_read},"cacheWrite":{cache_write},"totalTokens":{total},"cost":{{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"total":{cost}}}}}}}}}"#
    )
}

fn write_file(dir: &TempDir, project: &str, name: &str, lines: &[String]) {
    let project_dir = dir.path().join(project);
    fs::create_dir_all(&project_dir).unwrap();
    fs::write(project_dir.join(name), lines.join("\n")).unwrap();
}

fn sample_sessions(dir: &TempDir) {
    // In-range project A: 2 assistant msgs (1 with tools, 1 with two tools) + 1 user msg.
    write_file(
        dir,
        "proj-a",
        "2026-07-10T00-00-00_aaa.jsonl",
        &[
            header(r"E:\\proj-a", "2026-07-10T00:00:00Z"),
            model_change("m1"),
            message("2026-07-10T00:00:01Z", "assistant", "m1", 100, 50, 10, 5, 165, 0.01, &["read"]),
            message("2026-07-10T00:00:02Z", "user", "m1", 0, 0, 0, 0, 0, 0.0, &[]),
            message("2026-07-10T00:00:03Z", "assistant", "m1", 200, 80, 20, 0, 300, 0.02, &["read", "write"]),
        ],
    );
    // In-range project B: 1 assistant msg, no tools.
    write_file(
        dir,
        "proj-b",
        "2026-07-20T00-00-00_bbb.jsonl",
        &[
            header(r"E:\\proj-b", "2026-07-20T00:00:00Z"),
            model_change("m2"),
            message("2026-07-20T00:00:01Z", "assistant", "m2", 500, 100, 0, 0, 600, 0.05, &[]),
        ],
    );
    // Out-of-range session: must be excluded from everything except the heatmap.
    write_file(
        dir,
        "proj-a",
        "2026-06-01T00-00-00_old.jsonl",
        &[
            header(r"E:\\proj-a", "2026-06-01T00:00:00Z"),
            model_change("m1"),
            message("2026-06-01T00:00:01Z", "assistant", "m1", 0, 0, 0, 0, 999, 0.5, &[]),
        ],
    );
}

fn run(dir: &TempDir, scope: Scope, current_cwd: Option<&str>) -> UsageDashboard {
    run_files(dir, scope, current_cwd, None)
}

fn run_files(
    dir: &TempDir,
    scope: Scope,
    current_cwd: Option<&str>,
    files: Option<Vec<PathBuf>>,
) -> UsageDashboard {
    let now = DateTime::parse_from_rfc3339(NOW)
        .unwrap()
        .with_timezone(&Utc);
    build_dashboard_at(dir.path(), RangePreset::D30, scope, current_cwd, files, now).unwrap()
}

#[test]
fn aggregates_in_range_sessions() {
    let dir = TempDir::new().unwrap();
    sample_sessions(&dir);
    let dash = run(&dir, Scope::All, None);

    // Overview
    let o = &dash.overview;
    assert_eq!(o.sessions, 2, "out-of-range session must be excluded");
    assert_eq!(o.messages, 4, "2 assistant + 1 user + 1 assistant");
    assert_eq!(o.total_tokens, 1065);
    assert_eq!(o.input_tokens, 800);
    assert_eq!(o.output_tokens, 230);
    assert_eq!(o.cache_read, 30);
    assert_eq!(o.cache_write, 5);
    assert_eq!(o.tool_calls, 3);
    assert!((o.total_cost - 0.08).abs() < 1e-9);
    assert_eq!(o.active_days, 2);
    assert_eq!(o.current_streak, 0, "neither today nor yesterday active");
    assert_eq!(o.longest_streak, 1);

    // Tools: cost split evenly per message, never double-counted.
    let tools: Vec<&ToolStat> = dash.usage.tools.iter().collect();
    assert_eq!(tools.len(), 2);
    assert_eq!(tools[0].name, "read");
    assert_eq!(tools[0].count, 2);
    assert!((tools[0].cost - 0.02).abs() < 1e-9, "0.01 + 0.02/2");
    assert_eq!(tools[1].name, "write");
    assert_eq!(tools[1].count, 1);
    assert!((tools[1].cost - 0.01).abs() < 1e-9);

    // Models: m1 (465 tokens) below m2 (600) by cost.
    assert_eq!(dash.models.len(), 2);
    assert_eq!(dash.models[0].name, "m2");
    assert_eq!(dash.models[0].total_tokens, 600);
    assert_eq!(dash.models[1].name, "m1");
    assert_eq!(dash.models[1].total_tokens, 465);

    // Projects grouped by cwd.
    assert_eq!(dash.projects.len(), 2);
    assert_eq!(dash.projects[0].name, "proj-b");
    assert_eq!(dash.projects[0].sessions, 1);
    assert_eq!(dash.projects[1].name, "proj-a");
    assert_eq!(dash.projects[1].sessions, 1);

    // Sessions sorted newest-first, title from first user message.
    assert_eq!(dash.sessions.len(), 2);
    assert_eq!(dash.sessions[0].title, "Untitled");
    assert_eq!(dash.sessions[0].model, "m2");
    assert_eq!(dash.sessions[1].title, "Hello there project");
    assert_eq!(dash.sessions[1].model, "m1");

    // Daily series covers the whole range window.
    assert_eq!(dash.daily.len(), 31, "2026-07-01 .. 2026-07-31");
    assert_eq!(dash.daily[9].key, "2026-07-10");
    assert_eq!(dash.daily[9].total, 465);
    assert_eq!(dash.daily[9].models.get("m1"), Some(&465));
    assert_eq!(dash.daily[19].total, 600);
    assert_eq!(dash.daily[19].models.get("m2"), Some(&600));

    // Heatmap is 365 days and includes out-of-range activity.
    assert_eq!(dash.activity.len(), 365);
    let old = dash.activity.iter().find(|a| a.key == "2026-06-01").unwrap();
    assert_eq!(old.value, 999);
    let day10 = dash.activity.iter().find(|a| a.key == "2026-07-10").unwrap();
    assert_eq!(day10.value, 465);

    // Range metadata.
    assert_eq!(dash.range.range, "30d");
    assert_eq!(dash.range.from, "2026-07-01");
    assert_eq!(dash.range.to, "2026-07-31");
}

#[test]
fn scope_current_filters_by_cwd() {
    let dir = TempDir::new().unwrap();
    sample_sessions(&dir);
    let dash = run(&dir, Scope::Current, Some(r"E:\proj-a"));

    assert_eq!(dash.overview.sessions, 1);
    assert_eq!(dash.overview.messages, 3);
    assert_eq!(dash.overview.total_tokens, 465);
    assert_eq!(dash.projects.len(), 1);
    assert_eq!(dash.projects[0].name, "proj-a");
}

#[test]
fn missing_dir_yields_empty_dashboard() {
    let dir = TempDir::new().unwrap();
    let dash = run(&dir, Scope::All, None);
    assert_eq!(dash.overview.sessions, 0);
    assert_eq!(dash.overview.total_cost, 0.0);
    assert!(dash.models.is_empty());
    assert!(dash.sessions.is_empty());
    assert_eq!(dash.daily.len(), 31);
    assert_eq!(dash.activity.len(), 365);
}

#[test]
fn files_limits_scope_to_registered_sessions() {
    let dir = TempDir::new().unwrap();
    sample_sessions(&dir);
    // Only proj-a's in-range file is "registered" in piter's DB; the
    // other two scanned files must be ignored even though they parse fine.
    let files = Some(vec![dir
        .path()
        .join("proj-a")
        .join("2026-07-10T00-00-00_aaa.jsonl")]);
    let dash = run_files(&dir, Scope::All, None, files);

    assert_eq!(dash.overview.sessions, 1);
    assert_eq!(dash.overview.messages, 3);
    assert_eq!(dash.overview.total_tokens, 465);
    assert_eq!(dash.overview.total_cost, 0.03);
    assert_eq!(dash.models.len(), 1);
    assert_eq!(dash.projects.len(), 1);
    assert_eq!(dash.projects[0].name, "proj-a");
}

#[test]
fn empty_file_list_yields_no_sessions() {
    let dir = TempDir::new().unwrap();
    sample_sessions(&dir);
    let dash = run_files(&dir, Scope::All, None, Some(Vec::new()));
    assert_eq!(dash.overview.sessions, 0);
    assert_eq!(dash.overview.total_tokens, 0);
    assert!(dash.sessions.is_empty());
}

#[test]
fn missing_files_in_list_are_skipped() {
    let dir = TempDir::new().unwrap();
    sample_sessions(&dir);
    // A registered path that no longer exists must not break the run.
    let files = Some(vec![
        dir.path().join("proj-b").join("2026-07-20T00-00-00_bbb.jsonl"),
        dir.path().join("proj-b").join("gone.jsonl"),
    ]);
    let dash = run_files(&dir, Scope::All, None, files);
    assert_eq!(dash.overview.sessions, 1);
    assert_eq!(dash.overview.total_tokens, 600);
}
