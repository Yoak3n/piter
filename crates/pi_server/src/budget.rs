//! Monthly budget tracking — how much the current billing cycle has cost.
//!
//! The budget is a user-defined monthly cap (cents) with a configurable reset
//! day. "Used" is the sum of per-message costs in the current cycle, computed
//! by reusing the stats parser filtered from the cycle start.
//!
//! Cycle boundaries are derived from the request time (no timers): a cycle
//! starts on the configured reset day of the current month when today is on or
//! after it, otherwise on the previous month's reset day. Reset days that fall
//! past the end of a month clamp to the month's last day (e.g. day 31 in
//! February = 28/29).
//!
//! Dates use UTC (matching the rest of the stats module).

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;

use crate::stats::sum_cost_at;

// ─── Tiers ─────────────────────────────────────────────────────────────────

/// Reminder tiers are fixed at 50/80/100 percent of the budget (v1: not
/// configurable). `tier` in [`BudgetStatus`] maps to these constants:
/// 0 = below 50%, 1 = ≥50%, 2 = ≥80%, 3 = ≥100%.
pub const TIER_50: u32 = 1;
pub const TIER_80: u32 = 2;
pub const TIER_100: u32 = 3;

// ─── Cache ─────────────────────────────────────────────────────────────────

/// Re-aggregating every request is expensive (full parse of every session
/// file), so the computed status is cached for a short TTL. New session files
/// are therefore picked up within ~1 minute, which is fine for a budget gauge.
const CACHE_TTL: Duration = Duration::from_secs(60);

#[derive(PartialEq, Eq, Clone)]
struct CacheKey {
    budget_cents: i64,
    reset_day: u32,
    enabled: bool,
    cycle_start: String,
    cycle_end: String,
}

struct CacheEntry {
    key: CacheKey,
    status: BudgetStatus,
    at: Instant,
}

static CACHE: Mutex<Option<CacheEntry>> = Mutex::new(None);

#[cfg(test)]
fn reset_cache() {
    *CACHE.lock().unwrap() = None;
}

// ─── Payload ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetStatus {
    /// Cost of the current cycle in cents.
    pub used: i64,
    /// User-configured budget in cents.
    pub budget: i64,
    /// `used / budget × 100`. 0 when the budget is 0 or tracking is disabled.
    pub percent: f64,
    /// Reminder tier: 0 / [`TIER_50`] / [`TIER_80`] / [`TIER_100`].
    pub tier: u32,
    /// Day of month the cycle resets (as configured, 1..=31).
    pub reset_day: u32,
    /// Cycle start date (`YYYY-MM-DD`).
    pub cycle_start: String,
    /// Date the next cycle starts (`YYYY-MM-DD`) — the reset date; also the
    /// exclusive end of the current cycle (drives the countdown display).
    pub cycle_end: String,
}

// ─── Entry point ───────────────────────────────────────────────────────────

/// Compute the budget status for the cycle containing `now`.
///
/// `files` lists the managed session files to aggregate (mirroring the stats
/// dashboard); an empty list yields `used = 0`.
pub fn budget_status(
    files: Vec<PathBuf>,
    budget_cents: i64,
    reset_day: u32,
    enabled: bool,
) -> Result<BudgetStatus, String> {
    budget_status_at(files, budget_cents, reset_day, enabled, Utc::now())
}

/// Testable variant with an injected clock.
pub(crate) fn budget_status_at(
    files: Vec<PathBuf>,
    budget_cents: i64,
    reset_day: u32,
    enabled: bool,
    now: DateTime<Utc>,
) -> Result<BudgetStatus, String> {
    let reset_day = reset_day.clamp(1, 31);
    let today = now.date_naive();
    let (start, end) = cycle_bounds(today, reset_day);
    let cycle_start = fmt_day(start);
    let cycle_end = fmt_day(end);

    let key = CacheKey {
        budget_cents,
        reset_day,
        enabled,
        cycle_start: cycle_start.clone(),
        cycle_end: cycle_end.clone(),
    };
    if let Some(entry) = CACHE.lock().unwrap().as_ref() {
        if entry.at.elapsed() < CACHE_TTL && entry.key == key {
            return Ok(entry.status.clone());
        }
    }

    let status = if !enabled || budget_cents <= 0 {
        // 未设置 / 未启用：percent 0，不提醒（UI 显示"未设置"）
        BudgetStatus {
            used: 0,
            budget: budget_cents.max(0),
            percent: 0.0,
            tier: 0,
            reset_day,
            cycle_start,
            cycle_end,
        }
    } else {
        let from = DateTime::from_naive_utc_and_offset(
            start.and_hms_opt(0, 0, 0).expect("midnight is valid"),
            Utc,
        );
        let used = (sum_cost_at(&files, from) * 100.0).round() as i64;
        let percent = used as f64 / budget_cents as f64 * 100.0;
        BudgetStatus {
            used,
            budget: budget_cents,
            percent,
            tier: tier_for(percent),
            reset_day,
            cycle_start,
            cycle_end,
        }
    };

    *CACHE.lock().unwrap() = Some(CacheEntry {
        key,
        status: status.clone(),
        at: Instant::now(),
    });
    Ok(status)
}

// ─── Cycle computation ─────────────────────────────────────────────────────

/// `(cycle_start, cycle_end)` for the cycle containing `today`. `cycle_end` is
/// the next cycle's start date.
fn cycle_bounds(today: NaiveDate, reset_day: u32) -> (NaiveDate, NaiveDate) {
    let this_start = month_anchor(today.year(), today.month(), reset_day);
    let start = if today >= this_start {
        this_start
    } else {
        let (y, m) = prev_month(today.year(), today.month());
        month_anchor(y, m, reset_day)
    };
    let (y, m) = next_month(start.year(), start.month());
    let end = month_anchor(y, m, reset_day);
    (start, end)
}

/// The reset date within a given year/month, clamped to the month's last day.
fn month_anchor(year: i32, month: u32, reset_day: u32) -> NaiveDate {
    let dim = days_in_month(year, month);
    NaiveDate::from_ymd_opt(year, month, reset_day.min(dim)).expect("clamped day is valid")
}

fn prev_month(year: i32, month: u32) -> (i32, u32) {
    if month == 1 {
        (year - 1, 12)
    } else {
        (year, month - 1)
    }
}

fn next_month(year: i32, month: u32) -> (i32, u32) {
    if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    }
}

fn days_in_month(year: i32, month: u32) -> u32 {
    let (y, m) = next_month(year, month);
    let first_of_next = NaiveDate::from_ymd_opt(y, m, 1).expect("first of next month is valid");
    first_of_next.pred_opt().expect("month has a last day").day()
}

fn fmt_day(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

// ─── Tier mapping ──────────────────────────────────────────────────────────

fn tier_for(percent: f64) -> u32 {
    if percent >= 100.0 {
        TIER_100
    } else if percent >= 80.0 {
        TIER_80
    } else if percent >= 50.0 {
        TIER_50
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::TempDir;

    use super::*;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s)
            .unwrap()
            .with_timezone(&Utc)
    }

    fn header(ts: &str) -> String {
        format!(r#"{{"type":"session","version":3,"id":"s","timestamp":"{ts}","cwd":"cwd"}}"#)
    }

    fn message(ts: &str, cost: f64) -> String {
        format!(
            r#"{{"type":"message","id":"m","parentId":null,"timestamp":"{ts}","message":{{"role":"assistant","model":"m","usage":{{"input":0,"output":0,"cacheRead":0,"cacheWrite":0,"totalTokens":1,"cost":{{"total":{cost}}}}}}}}}"#
        )
    }

    fn write(dir: &TempDir, name: &str, lines: &[String]) -> PathBuf {
        fs::create_dir_all(dir.path()).unwrap();
        let p = dir.path().join(name);
        fs::write(&p, lines.join("\n")).unwrap();
        p
    }

    // ── Cycle bounds ────────────────────────────────────────────────────

    #[test]
    fn cycle_starts_this_month_after_reset_day() {
        // reset day 5, today 15 → cycle [Feb 5, Mar 5).
        let (start, end) = cycle_bounds(NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(), 5);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 5).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 5).unwrap());
    }

    #[test]
    fn cycle_starts_last_month_before_reset_day() {
        // reset day 5, today 3 → cycle [Jan 5, Feb 5).
        let (start, end) = cycle_bounds(NaiveDate::from_ymd_opt(2026, 2, 3).unwrap(), 5);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 5).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 2, 5).unwrap());
    }

    #[test]
    fn reset_day_clamps_to_month_length() {
        // Day 31 in February → 28 (2026 is not a leap year).
        let (start, end) = cycle_bounds(NaiveDate::from_ymd_opt(2026, 2, 15).unwrap(), 31);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 1, 31).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        // On the clamped day itself the new cycle has started.
        let (start, end) = cycle_bounds(NaiveDate::from_ymd_opt(2026, 2, 28).unwrap(), 31);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 2, 28).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 3, 31).unwrap());
    }

    #[test]
    fn cycle_wraps_year_boundary() {
        // reset day 20, today Dec 10 → cycle [Nov 20, Dec 20).
        let (start, end) = cycle_bounds(NaiveDate::from_ymd_opt(2026, 12, 10).unwrap(), 20);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 11, 20).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2026, 12, 20).unwrap());
        // reset day 1, today Dec 31 → cycle [Dec 1, Jan 1 next year).
        let (start, end) = cycle_bounds(NaiveDate::from_ymd_opt(2026, 12, 31).unwrap(), 1);
        assert_eq!(start, NaiveDate::from_ymd_opt(2026, 12, 1).unwrap());
        assert_eq!(end, NaiveDate::from_ymd_opt(2027, 1, 1).unwrap());
    }

    // ── Tier mapping ────────────────────────────────────────────────────

    #[test]
    fn tier_boundaries() {
        assert_eq!(tier_for(0.0), 0);
        assert_eq!(tier_for(49.9), 0);
        assert_eq!(tier_for(50.0), TIER_50);
        assert_eq!(tier_for(79.9), TIER_50);
        assert_eq!(tier_for(80.0), TIER_80);
        assert_eq!(tier_for(99.9), TIER_80);
        assert_eq!(tier_for(100.0), TIER_100);
        assert_eq!(tier_for(150.0), TIER_100);
    }

    // ── Aggregation ─────────────────────────────────────────────────────

    #[test]
    fn sums_cost_within_cycle_only() {
        reset_cache();
        let dir = TempDir::new().unwrap();
        let files = vec![write(
            &dir,
            "a.jsonl",
            &[
                header("2026-02-10T00:00:00Z"),
                // In cycle (reset day 1): 1.50 = 150 cents.
                message("2026-02-10T00:00:01Z", 1.00),
                message("2026-02-11T00:00:01Z", 0.50),
                // Before the cycle start (January): must be excluded.
                message("2026-01-20T00:00:01Z", 2.00),
            ],
        )];

        let now = ts("2026-02-15T12:00:00Z");
        let status = budget_status_at(files, 10_000, 1, true, now).unwrap();
        assert_eq!(status.used, 150);
        assert_eq!(status.cycle_start, "2026-02-01");
        assert_eq!(status.cycle_end, "2026-03-01");
        assert_eq!(status.tier, 0); // 1.5% < 50%
    }

    #[test]
    fn tier_rises_and_used_is_percent_of_budget() {
        reset_cache();
        let dir = TempDir::new().unwrap();
        let files = vec![write(
            &dir,
            "a.jsonl",
            &[
                header("2026-02-10T00:00:00Z"),
                message("2026-02-10T00:00:01Z", 0.55),
            ],
        )];

        let now = ts("2026-02-15T12:00:00Z");
        let status = budget_status_at(files, 100, 1, true, now).unwrap();
        assert_eq!(status.used, 55);
        assert!((status.percent - 55.0).abs() < 1e-9);
        assert_eq!(status.tier, TIER_50);
    }

    #[test]
    fn disabled_or_zero_budget_returns_zero_percent() {
        reset_cache();
        let dir = TempDir::new().unwrap();
        let files = vec![write(
            &dir,
            "a.jsonl",
            &[header("2026-02-10T00:00:00Z"), message("2026-02-10T00:00:01Z", 9.99)],
        )];
        let now = ts("2026-02-15T12:00:00Z");

        let disabled = budget_status_at(files.clone(), 10_000, 1, false, now).unwrap();
        assert_eq!(disabled.used, 0);
        assert_eq!(disabled.percent, 0.0);
        assert_eq!(disabled.tier, 0);

        let zero = budget_status_at(files, 0, 1, true, now).unwrap();
        assert_eq!(zero.used, 0);
        assert_eq!(zero.percent, 0.0);
        assert_eq!(zero.tier, 0);
    }

    #[test]
    fn cache_returns_consistent_status() {
        reset_cache();
        let dir = TempDir::new().unwrap();
        let files = vec![write(
            &dir,
            "a.jsonl",
            &[header("2026-02-10T00:00:00Z"), message("2026-02-10T00:00:01Z", 1.00)],
        )];
        let now = ts("2026-02-15T12:00:00Z");
        let a = budget_status_at(files.clone(), 10_000, 1, true, now).unwrap();
        let b = budget_status_at(files, 10_000, 1, true, now).unwrap();
        assert_eq!(a.used, b.used);
        assert_eq!(a.cycle_start, b.cycle_start);
    }
}
