//! LAN auth REST handlers (0.2.0 P3).
//!
//! - `POST   /api/lan/auth`            body `{pin, next?}` → 校验 PIN → 签发 30 天设备 token（Set-Cookie）
//! - `GET    /api/lan/auth/config`     `{ success, enabled }`（不返回 PIN 明文）
//! - `PUT    /api/lan/auth/config`     body `{ enabled? }` 和/或 `{ regenerate: true }` → 重新生成时**仅此一次**返回新 PIN
//! - `GET    /api/lan/auth/devices`    `{ success, devices: [{ token, createdAt, expiresAt }] }`
//! - `DELETE /api/lan/auth/devices/:id` 撤销单个设备（该设备需重新输 PIN）
//! - `POST   /api/lan/auth/revoke`     清空所有已授权设备

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Json, Response};
use chrono::{Duration, Utc};
use serde_json::{json, Value};

use crate::gateway::lan_auth::{
    ct_eq, generate_pin, generate_salt, generate_token, hash_pin, is_loopback_ip,
    pin_rate_limit_remaining, record_pin_failure, record_pin_success, LAN_COOKIE,
    TOKEN_MAX_AGE_SECS,
};
use crate::gateway::GatewayState;

fn json_resp(status: StatusCode, body: Value) -> Response {
    (status, Json(body)).into_response()
}

/// 配置类变更（enable / regenerate / revoke）只允许本机（loopback）执行——
/// admin 面板本来就跑在 127.0.0.1；任何已授权 LAN 设备都不得关掉鉴权或
/// 读取新 PIN。只读查询（config/devices）保持 cookie 门禁。
fn require_loopback(remote: SocketAddr) -> Result<(), Response> {
    if is_loopback_ip(remote.ip()) {
        Ok(())
    } else {
        Err(json_resp(
            StatusCode::FORBIDDEN,
            json!({"success": false, "error": "lan_forbidden_local_only"}),
        ))
    }
}

// ─── PIN exchange ──────────────────────────────────────────────────────────

/// Validate the 6-digit PIN and issue a long-lived device token.
pub async fn lan_auth_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Response {
    let ip = remote.ip();

    // 爆破防护：该 IP 处于锁定窗口内 → 直接 429（见 lan_auth.rs 限速表）。
    if let Some(secs) = pin_rate_limit_remaining(ip) {
        return json_resp(
            StatusCode::TOO_MANY_REQUESTS,
            json!({"success": false, "error": "lan_auth_rate_limited", "retryAfter": secs}),
        );
    }

    let pin = body.get("pin").and_then(Value::as_str).unwrap_or("");

    let cfg = state.db.get_lan_auth_config();
    if !cfg.enabled {
        return json_resp(
            StatusCode::BAD_REQUEST,
            json!({"success": false, "error": "lan_auth_disabled"}),
        );
    }
    if cfg.pin_hash.is_empty() || cfg.pin_salt.is_empty() {
        return json_resp(
            StatusCode::BAD_REQUEST,
            json!({"success": false, "error": "lan_auth_not_configured"}),
        );
    }
    // 恒定时间比较（hex 折叠，非 memcmp 短路），与 lan_auth.rs 威胁模型一致。
    if !ct_eq(&hash_pin(pin, &cfg.pin_salt), &cfg.pin_hash) {
        record_pin_failure(ip);
        return json_resp(
            StatusCode::UNAUTHORIZED,
            json!({"success": false, "error": "lan_auth_bad_pin"}),
        );
    }
    record_pin_success(ip);

    let token = generate_token();
    let expires_at = Utc::now() + Duration::days(30);
    if let Err(e) = state
        .db
        .insert_lan_token(&token, &expires_at.to_rfc3339())
    {
        return json_resp(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"success": false, "error": e}),
        );
    }

    let cookie = format!(
        "{LAN_COOKIE}={token}; HttpOnly; Path=/; Max-Age={}; SameSite=Lax",
        TOKEN_MAX_AGE_SECS
    );
    let mut headers = HeaderMap::new();
    headers.insert(
        header::SET_COOKIE,
        cookie.parse().expect("valid set-cookie value"),
    );
    (
        StatusCode::OK,
        headers,
        Json(json!({"success": true, "expiresAt": expires_at.to_rfc3339()})),
    )
        .into_response()
}

// ─── Config (admin) ────────────────────────────────────────────────────────

pub async fn get_lan_auth_config_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let cfg = state.db.get_lan_auth_config();
    Json(json!({
        "success": true,
        "enabled": cfg.enabled,
        "pinSet": !cfg.pin_hash.is_empty(),
    }))
}

/// Toggle the switch and/or regenerate the PIN (new PIN returned once).
pub async fn put_lan_auth_config_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    axum::Json(body): axum::Json<HashMap<String, Value>>,
) -> Response {
    if let Err(resp) = require_loopback(remote) {
        return resp;
    }
    let mut cfg = state.db.get_lan_auth_config();
    let mut new_pin: Option<String> = None;

    if let Some(enabled) = body.get("enabled").and_then(Value::as_bool) {
        cfg.enabled = enabled;
    }
    if body.get("regenerate").and_then(Value::as_bool).unwrap_or(false) {
        let pin = generate_pin();
        let salt = generate_salt();
        cfg.pin_hash = hash_pin(&pin, &salt);
        cfg.pin_salt = salt;
        new_pin = Some(pin);
    }

    match state
        .db
        .set_lan_auth_config(cfg.enabled, &cfg.pin_hash, &cfg.pin_salt)
    {
        Ok(()) => {
            let mut payload = json!({
                "success": true,
                "enabled": cfg.enabled,
                "pinSet": !cfg.pin_hash.is_empty(),
            });
            if let Some(pin) = new_pin {
                payload["pin"] = Value::String(pin);
            }
            Json(payload).into_response()
        }
        Err(e) => json_resp(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"success": false, "error": e}),
        ),
    }
}

// ─── Devices (per-device management) ───────────────────────────────────────

pub async fn lan_auth_devices_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let devices = state.db.list_lan_tokens();
    Json(json!({"success": true, "devices": devices}))
}

pub async fn delete_lan_auth_device_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
    Path(token): Path<String>,
) -> Response {
    if let Err(resp) = require_loopback(remote) {
        return resp;
    }
    match state.db.delete_lan_token(&token) {
        Ok(true) => Json(json!({"success": true})).into_response(),
        Ok(false) => json_resp(
            StatusCode::NOT_FOUND,
            json!({"success": false, "error": "lan_token_not_found"}),
        ),
        Err(e) => json_resp(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"success": false, "error": e}),
        ),
    }
}

pub async fn lan_auth_revoke_handler(
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
    ConnectInfo(remote): ConnectInfo<SocketAddr>,
) -> Response {
    if let Err(resp) = require_loopback(remote) {
        return resp;
    }
    match state.db.clear_lan_tokens() {
        Ok(()) => Json(json!({"success": true})).into_response(),
        Err(e) => json_resp(
            StatusCode::INTERNAL_SERVER_ERROR,
            json!({"success": false, "error": e}),
        ),
    }
}
