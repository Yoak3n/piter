//! Cross-session message search REST handler.
//!
//! `GET /api/search?q=<query>&limit=50` → `{ "results": [SearchHit...] }`.
//! The index is refreshed lazily (per-file mtime) before each search, so the
//! first request builds it; empty/too-short queries return no results.

use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::Query;
use axum::response::Json;
use serde_json::Value;

use crate::gateway::GatewayState;
use crate::search::index_if_stale;

pub async fn search_handler(
    Query(params): Query<HashMap<String, String>>,
    axum::extract::State(state): axum::extract::State<Arc<GatewayState>>,
) -> Json<Value> {
    let q = params.get("q").map(String::as_str).unwrap_or("").trim().to_string();
    if q.chars().count() < 2 {
        return Json(serde_json::json!({ "results": [] }));
    }
    let limit: u32 = params
        .get("limit")
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    // Refresh the index before searching (mtime-based incremental update).
    if let Err(e) = index_if_stale(&state.db) {
        log::warn!("[search] index_if_stale failed: {}", e);
    }

    match state.db.search_messages(&q, limit) {
        Ok(hits) => Json(serde_json::json!({ "results": hits })),
        Err(e) => Json(serde_json::json!({ "results": [], "error": e })),
    }
}
