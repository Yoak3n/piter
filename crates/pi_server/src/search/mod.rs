//! Cross-session full-text search — index pi session files into the gateway DB.
//!
//! Index strategy (mirrors `stats` file discovery): only DB-registered session
//! files are touched (`sessions.session_path`), never a directory scan. Every
//! session records the file mtime it was indexed at; `index_if_stale` stats the
//! files before a search and re-indexes (DELETE + full re-insert, JSONL is
//! append-only) any session whose mtime changed. The first search therefore
//! lazily builds the whole index.
//!
//! Only `user`/`assistant` message text (string content or `text` blocks) is
//! indexed — no thinking, no tool details.

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::time::UNIX_EPOCH;

use serde_json::Value;

use crate::gateway::db::{Db, IndexedMessage};

// ─── Entry points ───────────────────────────────────────────────────────────

/// Ensure every DB-registered session is indexed up to its current file mtime.
/// Cheap when nothing changed (only stats the files). Call before searching.
pub fn index_if_stale(db: &Db) -> Result<(), String> {
    for s in db.all_sessions() {
        let Some(path) = s.session_path else { continue };
        match file_mtime_ms(Path::new(&path)) {
            None => {
                // File gone: drop any stale index for this session.
                if db.search_index_mtime(&s.instance_id).is_some() {
                    if let Err(e) = db.delete_session_fts(&s.instance_id) {
                        log::warn!("[search] drop missing session {}: {}", s.instance_id, e);
                    }
                }
            }
            Some(mt) => {
                if db.search_index_mtime(&s.instance_id).as_deref() != Some(mt.as_str()) {
                    if let Err(e) = index_session(db, &s.instance_id, Path::new(&path), &mt) {
                        log::warn!("[search] skip indexing {}: {}", path, e);
                    }
                }
            }
        }
    }
    Ok(())
}

/// Rebuild the whole index from scratch (manual refresh / recovery).
pub fn reindex_all(db: &Db) -> Result<(), String> {
    db.clear_search_index()?;
    index_if_stale(db)
}

// ─── Indexing ───────────────────────────────────────────────────────────────

fn index_session(db: &Db, session_id: &str, path: &Path, mtime: &str) -> Result<(), String> {
    let entries = parse_session_entries(path)?;
    db.index_session_messages(session_id, &entries, mtime)
}

/// Parse a session `.jsonl` into indexable messages (tolerant of bad lines).
fn parse_session_entries(path: &Path) -> Result<Vec<IndexedMessage>, String> {
    let file = File::open(path).map_err(|e| format!("open: {e}"))?;
    let reader = BufReader::new(file);
    let mut out = Vec::new();
    for line in reader.lines() {
        let line = line.map_err(|e| format!("read line: {e}"))?;
        let Ok(value) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if value.get("type").and_then(|t| t.as_str()) != Some("message") {
            continue;
        }
        let Some(msg) = value.get("message") else {
            continue;
        };
        let role = msg.get("role").and_then(|r| r.as_str()).unwrap_or("");
        if role != "user" && role != "assistant" {
            continue;
        }
        let text = extract_text(msg);
        if text.trim().is_empty() {
            continue;
        }
        let entry_id = value.get("id").and_then(|v| v.as_str()).map(String::from);
        // Prefer the message's own ms timestamp (what the frontend sees in the
        // snapshot); fall back to the event-line RFC3339 timestamp.
        let timestamp = msg
            .get("timestamp")
            .and_then(|t| t.as_i64())
            .or_else(|| {
                value
                    .get("timestamp")
                    .and_then(|t| t.as_str())
                    .and_then(rfc3339_to_ms)
            });
        out.push(IndexedMessage {
            role: role.to_string(),
            content: text,
            entry_id,
            timestamp,
        });
    }
    Ok(out)
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

// ─── Time helpers ───────────────────────────────────────────────────────────

/// RFC3339 (event-line timestamps) → epoch milliseconds.
fn rfc3339_to_ms(s: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

/// File modification time as a millisecond string (stale detection marker).
fn file_mtime_ms(path: &Path) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let mt = meta.modified().ok()?;
    let ms = mt.duration_since(UNIX_EPOCH).ok()?.as_millis();
    Some(ms.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_file(dir: &Path) -> std::path::PathBuf {
        let path = dir.join("sess-1.jsonl");
        let mut f = std::fs::File::create(&path).unwrap();
        writeln!(f, r#"{{"type":"session","timestamp":"2026-08-09T10:00:00Z"}}"#).unwrap();
        writeln!(
            f,
            r#"{{"type":"message","id":"m1","timestamp":"2026-08-09T10:00:01Z","message":{{"role":"user","content":"帮我实现撤回功能","timestamp":1780000001000}}}}"#
        )
        .unwrap();
        writeln!(
            f,
            r#"{{"type":"message","id":"m2","timestamp":"2026-08-09T10:00:02Z","message":{{"role":"assistant","content":[{{"type":"text","text":"好的，我来实现撤回功能"}}],"timestamp":1780000002000}}}}"#
        )
        .unwrap();
        drop(f);
        path
    }

    /// End-to-end: DB-registered session file → index_if_stale → FTS/LIKE search.
    #[test]
    fn indexes_and_searches_chinese() {
        let dir = std::env::temp_dir().join(format!("piter-search-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::gateway::db::Db::open(&dir).unwrap();
        db.register_session("sess-1", "/tmp/proj", None).unwrap();
        let path = sample_file(&dir);
        db.complete_session("sess-1", &path.to_string_lossy()).unwrap();

        index_if_stale(&db).unwrap();
        assert!(db.search_index_mtime("sess-1").is_some(), "session indexed");

        let hits = db.search_messages("撤回", 20).unwrap();
        assert_eq!(hits.len(), 2, "both user+assistant messages match");
        for h in &hits {
            assert!(h.snippet.contains("撤回"), "snippet keeps the hit: {}", h.snippet);
            assert_eq!(h.session_id, "sess-1");
        }
        assert_eq!(hits[0].timestamp, Some(1780000002000));

        // Second run is a no-op (mtime unchanged) but still returns hits.
        index_if_stale(&db).unwrap();
        assert_eq!(db.search_messages("撤回", 20).unwrap().len(), 2);

        // Short query (trigram limit) still matches via LIKE fallback.
        assert_eq!(db.search_messages("功能", 20).unwrap().len(), 2);

        // Deletion clears the session's index.
        db.delete_session_fts("sess-1").unwrap();
        assert!(db.search_messages("撤回", 20).unwrap().is_empty());
    }

    /// Session file mtime change triggers a full re-index of that session.
    #[test]
    fn reindexes_on_mtime_change() {
        let dir = std::env::temp_dir().join(format!("piter-search-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let db = crate::gateway::db::Db::open(&dir).unwrap();
        db.register_session("sess-2", "/tmp/proj", None).unwrap();
        let path = sample_file(&dir);
        db.complete_session("sess-2", &path.to_string_lossy()).unwrap();

        index_if_stale(&db).unwrap();
        assert_eq!(db.search_messages("撤回", 20).unwrap().len(), 2);

        // Append a new message, then force an mtime change (filesystem ms precision).
        std::thread::sleep(std::time::Duration::from_millis(5));
        let mut f = std::fs::OpenOptions::new().append(true).open(&path).unwrap();
        writeln!(
            f,
            r#"{{"type":"message","id":"m3","timestamp":"2026-08-09T10:00:03Z","message":{{"role":"user","content":"再搜索一次","timestamp":1780000003000}}}}"#
        )
        .unwrap();
        drop(f);

        index_if_stale(&db).unwrap();
        let hits = db.search_messages("再搜索", 20).unwrap();
        assert_eq!(hits.len(), 1, "new message indexed after mtime change");
        assert_eq!(hits[0].entry_id.as_deref(), Some("m3"));
    }
}
