//! LAN authentication middleware (0.2.0 P3).
//!
//! The gateway listens on `0.0.0.0`, so any device on the LAN can reach the
//! REST API, the WebSocket and the chat SPA. This module adds a light PIN gate
//! for non-loopback requests:
//!
//! ```text
//! request → loopback (127.0.0.1 / ::1)?       → allow (desktop unaffected)
//!         → auth disabled?                     → allow (restores open access)
//!         → valid `piter_lan_token` cookie?    → allow (30-day device token)
//!         → /api/lan/auth or /api/health?      → allow (auth + liveness)
//!         → otherwise:
//!             /api/* or WebSocket upgrade      → 401 JSON
//!             page request                     → inline PIN page (no SPA)
//! ```
//!
//! Security model (deliberately light): the PIN is a one-time onboarding step,
//! long-lived device tokens follow, and nothing here stops a determined
//! attacker who is already able to run arbitrary chats through pi — this only
//! prevents accidental / casual access to the LAN interface.
//!
//! Threat model note: the PIN is stored as a **salted SHA-256** (a fast hash)
//! because a 6-digit PIN has tiny entropy — if the local `piter.db` is stolen
//! (e.g. from the desktop machine), the PIN can be brute-forced in seconds.
//! SHA-256 was chosen over a slow KDF deliberately: the PIN is a low-value
//! "casual access" gate, and the constant-time comparison below prevents
//! timing side-channels over the LAN. Do not rely on it as a strong secret.

use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, StatusCode};
use axum::middleware::Next;
use axum::response::{Html, IntoResponse, Response};
use sha2::{Digest, Sha256};

use crate::gateway::GatewayState;

/// Cookie name carrying the per-device bearer token.
pub const LAN_COOKIE: &str = "piter_lan_token";
/// Device tokens live for 30 days (matches `Max-Age` on the cookie).
pub const TOKEN_MAX_AGE_SECS: i64 = 30 * 24 * 60 * 60;

// ─── Crypto helpers ────────────────────────────────────────────────────────

/// Salted SHA-256 of the PIN, hex-encoded. The salt is stored next to it.
pub fn hash_pin(pin: &str, salt: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(salt.as_bytes());
    hasher.update(b":");
    hasher.update(pin.as_bytes());
    to_hex(&hasher.finalize())
}

/// Constant-time equality for two hex digests (both 64 chars in practice).
/// `a != b`/memcmp would short-circuit on the first differing byte; folding
/// every byte with XOR keeps the loop fixed-length so the comparison cannot
/// leak which prefix matched (see the threat-model note in the module docs).
pub fn ct_eq(a: &str, b: &str) -> bool {
    let a = a.as_bytes();
    let b = b.as_bytes();
    if a.len() != b.len() {
        return false; // SHA-256 hex is always 64 chars — no length signal
    }
    let mut diff: u8 = 0;
    for i in 0..a.len() {
        diff |= a[i] ^ b[i];
    }
    diff == 0
}

/// Fresh random salt (32 hex chars from a UUID v4).
pub fn generate_salt() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Random bearer token for one authorized device (32 hex chars).
pub fn generate_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// A fresh 6-digit numeric PIN.
pub fn generate_pin() -> String {
    uuid::Uuid::new_v4()
        .as_bytes()
        .iter()
        .take(6)
        .map(|b| char::from_digit((*b % 10) as u32, 10).expect("digit is valid"))
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

/// Extract the LAN token value from a `Cookie` header.
pub fn extract_lan_token(cookie_header: Option<&str>) -> Option<String> {
    for part in cookie_header?.split(';') {
        let part = part.trim();
        if let Some((k, v)) = part.split_once('=') {
            if k.trim() == LAN_COOKIE {
                let v = v.trim();
                if !v.is_empty() {
                    return Some(v.to_string());
                }
            }
        }
    }
    None
}

fn is_websocket_upgrade(req: &Request) -> bool {
    req.headers()
        .get(header::UPGRADE)
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| v.eq_ignore_ascii_case("websocket"))
}

pub(crate) fn is_loopback_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_loopback(),
        IpAddr::V6(v6) => {
            v6.is_loopback()
                || v6.to_ipv4_mapped().is_some_and(|v4| v4.is_loopback())
        }
    }
}

// ─── PIN brute-force guard ──────────────────────────────────────────────────
//
// `POST /api/lan/auth` is reachable by any LAN device, and a 6-digit PIN has
// only 10^6 combinations — without throttling a LAN attacker could brute-force
// it in hours. Keep it light (the feature is "prevent casual access", not a
// fortress): per-IP failure counter, lock after 5 consecutive failures for 60s
// (in-memory only; a restart resets it). Any correct PIN (or lock expiry)
// clears the counter.

const MAX_PIN_FAILURES: u32 = 5;
const PIN_LOCK_DURATION: Duration = Duration::from_secs(60);

#[derive(Clone, Copy)]
struct FailEntry {
    count: u32,
    locked_until: Instant,
}

static PIN_FAILURES: LazyLock<Mutex<HashMap<IpAddr, FailEntry>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// `None` = allowed to try; `Some(remaining_secs)` = currently locked out.
pub fn pin_rate_limit_remaining(ip: IpAddr) -> Option<u64> {
    let mut m = PIN_FAILURES.lock().unwrap();
    if let Some(e) = m.get(&ip) {
        if e.count >= MAX_PIN_FAILURES {
            let now = Instant::now();
            if now < e.locked_until {
                return Some(e.locked_until.duration_since(now).as_secs().max(1));
            }
            // Lock expired → forget and allow a fresh attempt.
            m.remove(&ip);
        }
    }
    None
}

pub fn record_pin_failure(ip: IpAddr) {
    let mut m = PIN_FAILURES.lock().unwrap();
    let e = m
        .entry(ip)
        .or_insert_with(|| FailEntry { count: 0, locked_until: Instant::now() });
    e.count += 1;
    if e.count >= MAX_PIN_FAILURES {
        e.locked_until = Instant::now() + PIN_LOCK_DURATION;
    }
}

pub fn record_pin_success(ip: IpAddr) {
    PIN_FAILURES.lock().unwrap().remove(&ip);
}

// ─── Inline PIN page (no SPA exposure) ────────────────────────────────────
//
// Unauthorized LAN *page* requests get this zero-dependency page instead of
// the chat SPA, so an unauthenticated device never even fetches the app code
// or the API contract. The PIN is typed here, exchanged for a cookie via
// `POST /api/lan/auth`, then the original URL is re-visited.

pub fn pin_page_html(next: &str) -> String {
    // JSON-quote `next` so arbitrary paths can't break out of the script.
    let next_json = serde_json::to_string(next).unwrap_or_else(|_| "\"/chat\"".to_string());
    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>Piter</title>
<style>
  :root {{ color-scheme: light dark; }}
  body {{ margin:0; min-height:100vh; display:flex; align-items:center; justify-content:center;
         font-family:-apple-system,"Segoe UI",Roboto,sans-serif; background:#f5f5f7; color:#1d1d1f; }}
  .card {{ background:#fff; border:1px solid #e4e4e2; border-radius:14px; padding:32px;
           width:min(92vw,340px); box-shadow:0 8px 24px rgba(0,0,0,.06); }}
  h1 {{ font-size:18px; margin:0 0 6px; }}
  p {{ font-size:13px; color:#6e6e73; margin:0 0 18px; line-height:1.5; }}
  input {{ width:100%; box-sizing:border-box; height:46px; font-size:22px; letter-spacing:.45em;
          text-align:center; border:1px solid #d2d2d7; border-radius:10px; margin-bottom:12px;
          outline:none; font-variant-numeric:tabular-nums; }}
  input:focus {{ border-color:#2f6fed; }}
  button {{ width:100%; height:42px; border:0; border-radius:10px; background:#2f6fed; color:#fff;
           font-size:15px; font-weight:600; cursor:pointer; }}
  button:disabled {{ opacity:.5; cursor:default; }}
  .err {{ color:#d70015; font-size:13px; margin-top:12px; min-height:18px; text-align:center; }}
  @media (prefers-color-scheme: dark) {{
    body {{ background:#1e1e1e; color:#f5f5f5; }}
    .card {{ background:#2a2a2a; border-color:#3a3a3a; }}
    p {{ color:#a1a1a6; }}
    input {{ background:#1e1e1e; border-color:#484848; color:#f5f5f5; }}
  }}
</style>
</head>
<body>
<div class="card">
  <h1>Piter</h1>
  <p>This device hasn't been authorized yet. Enter the 6-digit PIN shown in Piter&rsquo;s Share &amp; Connect tab.</p>
  <input id="pin" inputmode="numeric" pattern="[0-9]*" maxlength="6" placeholder="&bull;&bull;&bull;&bull;&bull;&bull;" autocomplete="one-time-code">
  <button id="go">Unlock</button>
  <div class="err" id="err"></div>
</div>
<script>
  var next = {next_json};
  var pin = document.getElementById('pin');
  var go = document.getElementById('go');
  var err = document.getElementById('err');
  function submit() {{
    if (!/^\d{{6}}$/.test(pin.value)) {{ err.textContent = 'PIN must be 6 digits.'; return; }}
    go.disabled = true;
    fetch('/api/lan/auth', {{
      method: 'POST',
      headers: {{ 'Content-Type': 'application/json' }},
      body: JSON.stringify({{ pin: pin.value }})
    }}).then(function (res) {{
      return res.json().then(function (data) {{
        if (res.ok && data.success) {{ window.location.href = next; return; }}
        err.textContent = data.error === 'lan_auth_bad_pin'
          ? 'Wrong PIN, try again.'
          : (data.error || 'Failed to unlock.');
        go.disabled = false;
      }});
    }}).catch(function () {{
      err.textContent = 'Network error — try again.';
      go.disabled = false;
    }});
  }}
  go.addEventListener('click', submit);
  pin.addEventListener('keydown', function (e) {{ if (e.key === 'Enter') submit(); }});
  pin.focus();
</script>
</body>
</html>"#
    )
}

// ─── Middleware ────────────────────────────────────────────────────────────

pub async fn lan_auth_middleware(
    State(state): State<Arc<GatewayState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    req: Request,
    next: Next,
) -> Response {
    // 1. Loopback (desktop admin/chat, local tools) is always exempt. IPv4
    //    loopback may arrive v4-mapped (`::ffff:127.0.0.1`) — unwrap it.
    if is_loopback_ip(remote.ip()) {
        return next.run(req).await;
    }

    // 2. Auth disabled → restore open access.
    if !state.db.get_lan_auth_config().enabled {
        return next.run(req).await;
    }

    let path = req.uri().path().to_string();

    // 3. The PIN exchange endpoint and the (non-sensitive) liveness probe
    //    must stay reachable before a device is authorized.
    if path == "/api/lan/auth" || path == "/api/health" {
        return next.run(req).await;
    }

    // 4. Valid per-device cookie → pass.
    let cookie = req
        .headers()
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok());
    if let Some(token) = extract_lan_token(cookie) {
        if state.db.lan_token_valid(&token) {
            return next.run(req).await;
        }
    }

    // 5. Unauthorized: API + WebSocket upgrades get 401 JSON; page requests
    //    get the inline PIN page (the SPA is never served to this device).
    if path.starts_with("/api/") || is_websocket_upgrade(&req) {
        return (
            StatusCode::UNAUTHORIZED,
            axum::Json(serde_json::json!({
                "success": false,
                "error": "lan_auth_required"
            })),
        )
            .into_response();
    }

    let next_path = match req.uri().query() {
        Some(q) => format!("{path}?{q}"),
        None => path,
    };
    (StatusCode::OK, Html(pin_page_html(&next_path))).into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pin_hash_is_salted_and_deterministic() {
        let salt = generate_salt();
        let a = hash_pin("123456", &salt);
        let b = hash_pin("123456", &salt);
        assert_eq!(a, b);
        assert_eq!(a.len(), 64, "sha256 hex");
        // Same PIN, different salt → different hash.
        assert_ne!(a, hash_pin("123456", &generate_salt()));
        // Different PIN, same salt → different hash.
        assert_ne!(a, hash_pin("654321", &salt));
    }

    #[test]
    fn constant_time_eq_matches_and_mismatches() {
        let h = hash_pin("123456", "salt");
        assert!(ct_eq(&h, &h), "identical digest");
        assert!(ct_eq(&h, &h.clone()), "clone identical");
        assert!(!ct_eq(&h, &hash_pin("123456", "salt2")), "same pin, other salt");
        assert!(!ct_eq(&h, &hash_pin("654321", "salt")), "other pin, same salt");
        assert!(!ct_eq("abc", "abcd"), "length mismatch is unequal");
        // 仅首字节不同也判不等（非前缀短路逻辑正确性）
        let mut other = h.clone();
        other.replace_range(0..1, if other.starts_with('a') { "b" } else { "a" });
        assert!(!ct_eq(&h, &other));
    }

    #[test]
    fn generated_pin_is_six_digits() {
        for _ in 0..50 {
            let pin = generate_pin();
            assert_eq!(pin.len(), 6);
            assert!(pin.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn token_and_salt_are_random_hex() {
        assert_ne!(generate_token(), generate_token());
        assert_ne!(generate_salt(), generate_salt());
        assert_eq!(generate_token().len(), 32);
    }

    #[test]
    fn extracts_lan_token_from_cookie() {
        let cookie = "theme=dark; piter_lan_token=abc123; other=1";
        assert_eq!(
            extract_lan_token(Some(cookie)),
            Some("abc123".to_string())
        );
        assert_eq!(extract_lan_token(Some("piter_lan_token=xyz")), Some("xyz".to_string()));
        assert_eq!(extract_lan_token(Some("theme=dark")), None);
        assert_eq!(extract_lan_token(None), None);
        assert_eq!(extract_lan_token(Some("piter_lan_token=")), None);
    }

    #[test]
    fn loopback_detection_covers_v4_mapped() {
        assert!(is_loopback_ip("127.0.0.1".parse().unwrap()));
        assert!(is_loopback_ip("::1".parse().unwrap()));
        assert!(is_loopback_ip("::ffff:127.0.0.1".parse().unwrap()));
        assert!(!is_loopback_ip("192.168.1.10".parse().unwrap()));
        assert!(!is_loopback_ip("::ffff:192.168.1.10".parse().unwrap()));
    }

    #[test]
    fn pin_rate_limit_locks_after_failures_and_clears_on_success() {
        // 注：不 reset 静态表——测试并行运行，reset 会清掉别的测试记录；
        // 本测试用固定 IP，进程内仅此一处使用，天然隔离。
        let ip: IpAddr = "192.168.1.10".parse().unwrap();
        assert!(pin_rate_limit_remaining(ip).is_none());

        // Under the threshold → still allowed.
        for _ in 0..(MAX_PIN_FAILURES - 1) {
            record_pin_failure(ip);
            assert!(pin_rate_limit_remaining(ip).is_none());
        }
        // The final failure triggers the lockout.
        record_pin_failure(ip);
        assert!(pin_rate_limit_remaining(ip).is_some());

        // A correct PIN clears the counter.
        record_pin_success(ip);
        assert!(pin_rate_limit_remaining(ip).is_none());
    }

    #[test]
    fn pin_rate_limit_is_per_ip() {
        let bad: IpAddr = "192.168.1.20".parse().unwrap();
        let other: IpAddr = "192.168.1.11".parse().unwrap();
        for _ in 0..MAX_PIN_FAILURES {
            record_pin_failure(bad);
        }
        assert!(pin_rate_limit_remaining(bad).is_some());
        assert!(pin_rate_limit_remaining(other).is_none(), "other IP unaffected");
    }
}
