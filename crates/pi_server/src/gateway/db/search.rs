//! 跨会话搜索索引：FTS 写入/删除/清空/mtime 查询 + 搜索 + 摘要工具。
//!
//! 做什么：维护 `session_messages`（索引行）+ `search_index_state`（每会话
//! 文件 mtime）+ FTS5 虚表（trigram，触发器同步）；`search_messages` 在
//! FTS5 可用且查询 ≥3 字符时走短语查询，否则 LIKE 兜底；`make_snippet`/
//! `truncate_chars` 生成展示摘要。
//! 不做什么：不扫描磁盘/解析会话文件（search/ 模块）；不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite；`Db` 定义于 db/mod.rs。

use std::sync::atomic::Ordering;

use rusqlite::params;
use serde::Serialize;

use super::Db;

/// A message ready to be inserted into the search index (search module output).
#[derive(Debug, Clone)]
pub struct IndexedMessage {
    pub role: String,
    pub content: String,
    pub entry_id: Option<String>,
    /// Epoch milliseconds (matches the frontend Message.timestamp).
    pub timestamp: Option<i64>,
}

/// One cross-session search hit.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SearchHit {
    pub session_id: String,
    pub project_name: Option<String>,
    pub label: Option<String>,
    pub role: String,
    /// Context window around the first match, ready for display.
    pub snippet: String,
    pub entry_id: Option<String>,
    /// Epoch milliseconds; used by the frontend to scroll to the message.
    pub timestamp: Option<i64>,
}

impl Db {
    // ── Cross-session Search ────────────────────────────────────────────

    /// Whether FTS5 (trigram tokenizer) is available; false → LIKE fallback.
    pub fn fts_available(&self) -> bool {
        self.fts_available.load(Ordering::SeqCst)
    }

    /// Replace a session's indexed messages (full re-index after mtime change).
    /// The FTS index stays in sync via triggers on `session_messages`.
    pub fn index_session_messages(
        &self,
        session_id: &str,
        entries: &[IndexedMessage],
        file_mtime: &str,
    ) -> Result<(), String> {
        let mut conn = self.conn.lock().unwrap();
        let tx = conn.transaction().map_err(|e| format!("index tx: {e}"))?;
        tx.execute(
            "DELETE FROM session_messages WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("index delete: {e}"))?;
        {
            let mut stmt = tx
                .prepare(
                    "INSERT INTO session_messages (session_id, entry_id, role, content, timestamp, file_mtime)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                )
                .map_err(|e| format!("index insert prepare: {e}"))?;
            for m in entries {
                stmt.execute(params![
                    session_id,
                    m.entry_id,
                    m.role,
                    m.content,
                    m.timestamp,
                    file_mtime
                ])
                .map_err(|e| format!("index insert: {e}"))?;
            }
        }
        tx.execute(
            "INSERT INTO search_index_state (session_id, file_mtime, indexed_at) VALUES (?1, ?2, ?3)
             ON CONFLICT(session_id) DO UPDATE SET file_mtime = excluded.file_mtime, indexed_at = excluded.indexed_at",
            params![session_id, file_mtime, chrono::Utc::now().to_rfc3339()],
        )
        .map_err(|e| format!("index state: {e}"))?;
        tx.commit().map_err(|e| format!("index commit: {e}"))?;
        Ok(())
    }

    /// Drop a session's indexed messages (and FTS rows via trigger).
    pub fn delete_session_fts(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM session_messages WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete fts: {e}"))?;
        conn.execute(
            "DELETE FROM search_index_state WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete fts state: {e}"))?;
        Ok(())
    }

    /// Wipe the whole search index (reindex_all).
    pub fn clear_search_index(&self) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute("DELETE FROM session_messages", [])
            .map_err(|e| format!("clear search: {e}"))?;
        conn.execute("DELETE FROM search_index_state", [])
            .map_err(|e| format!("clear search state: {e}"))?;
        Ok(())
    }

    /// Last indexed file-mtime for a session (None = never indexed).
    pub fn search_index_mtime(&self, session_id: &str) -> Option<String> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT file_mtime FROM search_index_state WHERE session_id = ?1",
            params![session_id],
            |row| row.get(0),
        )
        .ok()
    }

    /// Full-text search across all indexed session messages, newest first.
    /// Uses FTS5 (trigram) for queries ≥ 3 chars when available, else LIKE.
    pub fn search_messages(&self, q: &str, limit: u32) -> Result<Vec<SearchHit>, String> {
        let q = q.trim();
        let qlen = q.chars().count();
        if q.is_empty() || qlen < 2 {
            return Ok(Vec::new());
        }
        let limit = limit.clamp(1, 200);
        let conn = self.conn.lock().unwrap();

        // (content, role, session_id, entry_id, timestamp)
        let rows: Vec<(String, String, String, Option<String>, Option<i64>)> =
            if self.fts_available() && qlen >= 3 {
                // trigram tokenizer: query must be a quoted phrase (≥3 chars)
                let fts_q = format!("\"{}\"", q.replace('"', "\"\""));
                let mut stmt = conn
                    .prepare(
                        "SELECT f.content, f.role, f.session_id, f.entry_id, f.timestamp
                         FROM session_messages_fts f
                         WHERE session_messages_fts MATCH ?1
                         ORDER BY f.timestamp IS NULL, f.timestamp DESC LIMIT ?2",
                    )
                    .map_err(|e| format!("search prepare: {e}"))?;
                stmt.query_map(params![fts_q, limit], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })
                .map_err(|e| format!("search query: {e}"))?
                .filter_map(|r| r.ok())
                .collect()
            } else {
                let like = format!(
                    "%{}%",
                    q.replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_")
                );
                let mut stmt = conn
                    .prepare(
                        "SELECT content, role, session_id, entry_id, timestamp
                         FROM session_messages
                         WHERE content LIKE ?1 ESCAPE '\\'
                         ORDER BY timestamp IS NULL, timestamp DESC LIMIT ?2",
                    )
                    .map_err(|e| format!("search prepare: {e}"))?;
                stmt.query_map(params![like, limit], |row| {
                    Ok((
                        row.get(0)?,
                        row.get(1)?,
                        row.get(2)?,
                        row.get(3)?,
                        row.get(4)?,
                    ))
                })
                .map_err(|e| format!("search query: {e}"))?
                .filter_map(|r| r.ok())
                .collect()
            };

        let mut out = Vec::with_capacity(rows.len());
        for (content, role, session_id, entry_id, ts) in rows {
            let (label, project_name) = conn
                .query_row(
                    "SELECT s.name, p.name FROM sessions s
                     LEFT JOIN projects p ON p.id = s.project_id
                     WHERE s.instance_id = ?1",
                    params![session_id],
                    |row| Ok((row.get::<_, Option<String>>(0)?, row.get::<_, Option<String>>(1)?)),
                )
                .unwrap_or((None, None));
            out.push(SearchHit {
                session_id,
                project_name,
                label,
                role,
                snippet: make_snippet(&content, q),
                entry_id,
                timestamp: ts,
            });
        }
        Ok(out)
    }
}

// ─── Helpers ────────────────────────────────────────────────────────────────

/// Char-safe context window around the first case-insensitive match.
/// `find` on a lowercased copy yields byte offsets that may split UTF-8
/// (case folding changes byte length), so we convert to a char offset first.
fn make_snippet(content: &str, q: &str) -> String {
    const RADIUS: usize = 60;
    let c_lower = content.to_lowercase();
    let Some(rel) = c_lower.find(&q.to_lowercase()) else {
        return truncate_chars(content, 160);
    };
    let char_pos = c_lower[..rel].chars().count();
    let total_chars = content.chars().count();
    let start_char = char_pos.saturating_sub(RADIUS);
    let end_char = (char_pos + q.chars().count() + RADIUS).min(total_chars);
    let mut out: String = content
        .chars()
        .skip(start_char)
        .take(end_char - start_char)
        .collect();
    if start_char > 0 {
        out.insert_str(0, "…");
    }
    if end_char < total_chars {
        out.push('…');
    }
    out
}

fn truncate_chars(s: &str, n: usize) -> String {
    let mut out: String = s.chars().take(n).collect();
    if s.chars().count() > n {
        out.push('…');
    }
    out
}
