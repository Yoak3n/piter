//! 配置类表 CRUD：月度预算 + LAN 鉴权配置/令牌。
//!
//! 做什么：维护单行配置表 `budget_config`（预算分额/重置日/开关）与
//! `lan_auth_config`（启用/PIN 加盐哈希/更新时间），以及设备令牌表
//! `lan_tokens`（签发/校验/列表/撤销/清空）。
//! 不做什么：不计算周期/档位（budget.rs）；不生成或校验 PIN/令牌（lan_auth.rs）；
//! 不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite + serde；`Db` 定义于 db/mod.rs。

use rusqlite::params;
use serde::Serialize;

use super::Db;

/// User-configured monthly budget (single row, id = 1).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BudgetConfig {
    /// Budget cap in cents (0 = not set).
    pub budget_cents: i64,
    /// Day of month a new cycle starts (1..=31, clamped per month).
    pub reset_day: i64,
    /// Whether budget tracking is turned on.
    pub enabled: bool,
}

/// LAN auth config (single row, id = 1). The PIN is never stored in plaintext.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanAuthConfig {
    /// Whether LAN PIN auth is enforced for non-loopback requests.
    pub enabled: bool,
    /// Salted SHA-256 hash of the 6-digit PIN.
    pub pin_hash: String,
    /// Random salt used to derive `pin_hash`.
    pub pin_salt: String,
    /// Last config change time.
    pub updated_at: String,
}

/// One authorized LAN device (a random bearer token).
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LanToken {
    pub token: String,
    pub created_at: String,
    pub expires_at: String,
}

impl Db {
    // ── Monthly Budget ───────────────────────────────────────────────

    /// Read the single-row budget config (defaults when never configured).
    pub fn get_budget_config(&self) -> BudgetConfig {
        let conn = self.conn.lock().unwrap();
        let (budget, day, enabled) = conn
            .query_row(
                "SELECT monthly_budget_cents, reset_day, enabled FROM budget_config WHERE id = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, i64>(1)?, row.get::<_, i64>(2)?)),
            )
            .unwrap_or((0, 1, 0));
        BudgetConfig {
            budget_cents: budget,
            reset_day: day,
            enabled: enabled != 0,
        }
    }

    /// Persist the budget config. `reset_day` is clamped to 1..=31; shorter
    /// months clamp further at cycle computation time (e.g. 31 → Feb 28/29).
    pub fn set_budget_config(
        &self,
        budget_cents: i64,
        reset_day: i64,
        enabled: bool,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO budget_config (id, monthly_budget_cents, reset_day, enabled)
             VALUES (1, ?1, ?2, ?3)
             ON CONFLICT(id) DO UPDATE SET
                 monthly_budget_cents = excluded.monthly_budget_cents,
                 reset_day = excluded.reset_day,
                 enabled = excluded.enabled",
            params![budget_cents, reset_day.clamp(1, 31), enabled as i32],
        )
        .map_err(|e| format!("set_budget_config: {}", e))?;
        Ok(())
    }

    // ── LAN Auth ───────────────────────────────────────────────────────

    /// Read the single-row LAN auth config (defaults when never configured).
    pub fn get_lan_auth_config(&self) -> LanAuthConfig {
        let conn = self.conn.lock().unwrap();
        let (enabled, pin_hash, pin_salt, updated_at) = conn
            .query_row(
                "SELECT enabled, pin_hash, pin_salt, updated_at FROM lan_auth_config WHERE id = 1",
                [],
                |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?)),
            )
            .unwrap_or((0, String::new(), String::new(), String::new()));
        LanAuthConfig {
            enabled: enabled != 0,
            pin_hash,
            pin_salt,
            updated_at,
        }
    }

    /// Persist the LAN auth config (PIN stored as salted hash).
    pub fn set_lan_auth_config(
        &self,
        enabled: bool,
        pin_hash: &str,
        pin_salt: &str,
    ) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO lan_auth_config (id, enabled, pin_hash, pin_salt, updated_at)
             VALUES (1, ?1, ?2, ?3, ?4)
             ON CONFLICT(id) DO UPDATE SET
                 enabled = excluded.enabled,
                 pin_hash = excluded.pin_hash,
                 pin_salt = excluded.pin_salt,
                 updated_at = excluded.updated_at",
            params![
                enabled as i32,
                pin_hash,
                pin_salt,
                chrono::Utc::now().to_rfc3339()
            ],
        )
        .map_err(|e| format!("set_lan_auth_config: {}", e))?;
        Ok(())
    }

    /// Record a newly issued device token.
    pub fn insert_lan_token(&self, token: &str, expires_at: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO lan_tokens (token, created_at, expires_at) VALUES (?1, ?2, ?3)",
            params![token, chrono::Utc::now().to_rfc3339(), expires_at],
        )
        .map_err(|e| format!("insert_lan_token: {}", e))?;
        Ok(())
    }

    /// Whether the token exists and has not expired (RFC3339 compare).
    pub fn lan_token_valid(&self, token: &str) -> bool {
        let conn = self.conn.lock().unwrap();
        let Some(expires_at) = conn
            .query_row(
                "SELECT expires_at FROM lan_tokens WHERE token = ?1",
                params![token],
                |row| row.get::<_, String>(0),
            )
            .ok()
        else {
            return false;
        };
        chrono::DateTime::parse_from_rfc3339(&expires_at)
            .map(|t| t > chrono::Utc::now())
            .unwrap_or(false)
    }

    /// List all authorized devices (for per-device management).
    pub fn list_lan_tokens(&self) -> Vec<LanToken> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare("SELECT token, created_at, expires_at FROM lan_tokens ORDER BY created_at")
            .unwrap();
        stmt.query_map([], |row| {
            Ok(LanToken {
                token: row.get(0)?,
                created_at: row.get(1)?,
                expires_at: row.get(2)?,
            })
        })
        .unwrap()
        .filter_map(|r| r.ok())
        .collect()
    }

    /// Revoke a single device token. Returns false when the token is unknown.
    pub fn delete_lan_token(&self, token: &str) -> Result<bool, String> {
        let conn = self.conn.lock().unwrap();
        let rows = conn
            .execute("DELETE FROM lan_tokens WHERE token = ?1", params![token])
            .map_err(|e| format!("delete_lan_token: {}", e))?;
        Ok(rows > 0)
    }

    /// Revoke every authorized device (pinch-to-reset).
    pub fn clear_lan_tokens(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM lan_tokens", [])
            .map_err(|e| format!("clear_lan_tokens: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn budget_config_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();

        // Defaults when never configured.
        let cfg = db.get_budget_config();
        assert_eq!(cfg.budget_cents, 0);
        assert_eq!(cfg.reset_day, 1);
        assert!(!cfg.enabled);

        // Update + read back.
        db.set_budget_config(50_000, 15, true).unwrap();
        let cfg = db.get_budget_config();
        assert_eq!(cfg.budget_cents, 50_000);
        assert_eq!(cfg.reset_day, 15);
        assert!(cfg.enabled);

        // reset_day is clamped to 1..=31 at write time.
        db.set_budget_config(100, 0, true).unwrap();
        assert_eq!(db.get_budget_config().reset_day, 1);
        db.set_budget_config(100, 99, true).unwrap();
        assert_eq!(db.get_budget_config().reset_day, 31);

        // Turning tracking off is persisted.
        db.set_budget_config(50_000, 15, false).unwrap();
        assert!(!db.get_budget_config().enabled);
    }

    #[test]
    fn lan_auth_config_and_tokens_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();

        // Defaults: disabled, no PIN configured.
        let cfg = db.get_lan_auth_config();
        assert!(!cfg.enabled);
        assert!(cfg.pin_hash.is_empty());

        // Persist enabled + salted hash.
        db.set_lan_auth_config(true, "deadbeef", "salt1").unwrap();
        let cfg = db.get_lan_auth_config();
        assert!(cfg.enabled);
        assert_eq!(cfg.pin_hash, "deadbeef");
        assert_eq!(cfg.pin_salt, "salt1");

        // Toggling off keeps the stored hash.
        db.set_lan_auth_config(false, "deadbeef", "salt1").unwrap();
        assert!(!db.get_lan_auth_config().enabled);
        assert_eq!(db.get_lan_auth_config().pin_hash, "deadbeef");

        // Tokens: valid while unexpired, invalid when expired/unknown.
        let future = (chrono::Utc::now() + chrono::Duration::days(30)).to_rfc3339();
        let past = (chrono::Utc::now() - chrono::Duration::days(1)).to_rfc3339();
        db.insert_lan_token("t1", &future).unwrap();
        db.insert_lan_token("t2", &past).unwrap();
        assert!(db.lan_token_valid("t1"));
        assert!(!db.lan_token_valid("t2"));
        assert!(!db.lan_token_valid("missing"));

        let devices = db.list_lan_tokens();
        assert_eq!(devices.len(), 2);

        // Per-device revoke; unknown token → false.
        assert!(db.delete_lan_token("t1").unwrap());
        assert!(!db.delete_lan_token("t1").unwrap());
        assert_eq!(db.list_lan_tokens().len(), 1);

        // Clear everything.
        db.clear_lan_tokens().unwrap();
        assert!(db.list_lan_tokens().is_empty());
    }
}
