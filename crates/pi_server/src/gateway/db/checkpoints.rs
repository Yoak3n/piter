//! 文件回滚 checkpoint 表 CRUD（消息撤回 0.2.0 P3）。
//!
//! 做什么：维护 `checkpoints` 表——每轮 agent 完成（git 仓库）时写入一行
//! （turn_seq / git stash create ref / untracked 清单 manifest），按
//! session_id 查询"某时间点之前最近的 checkpoint"、按保留数裁剪。
//! 不做什么：不执行 git 命令（gateway/checkpoint.rs）；不含建表迁移（db/mod.rs）。
//! 依赖：rusqlite；`Db` 定义于 db/mod.rs。

use rusqlite::params;

use super::Db;

/// One checkpoint row: a git snapshot of the workspace taken at `agent_end`.
#[derive(Debug, Clone)]
pub struct CheckpointRow {
    pub id: i64,
    /// instance_id of the session this checkpoint belongs to.
    pub session_id: String,
    /// Completed turn count at creation time (agent_end of that turn).
    pub turn_seq: i32,
    /// `git stash create` commit-ish, usable with `git restore --source=<ref>`.
    pub git_ref: String,
    /// JSON array of `{"path": <rel>, "snapshot": <abs>}` for untracked files.
    pub manifest: String,
    /// Epoch ms when the checkpoint was created.
    pub created_at_ms: i64,
}

impl Db {
    /// Insert a checkpoint row and return its autoincrement id (the id doubles
    /// as the snapshot directory name under `<data_dir>/checkpoints/<id>/`).
    pub fn insert_checkpoint(
        &self,
        session_id: &str,
        turn_seq: i32,
        git_ref: &str,
        manifest: &str,
    ) -> Result<i64, String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "INSERT INTO checkpoints (session_id, turn_seq, git_ref, manifest, created_at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                session_id,
                turn_seq,
                git_ref,
                manifest,
                crate::gateway::now_epoch_ms()
            ],
        )
        .map_err(|e| format!("insert_checkpoint: {}", e))?;
        Ok(conn.last_insert_rowid())
    }

    /// Rewrite the manifest after untracked snapshots are copied to disk.
    pub fn update_checkpoint_manifest(&self, id: i64, manifest: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "UPDATE checkpoints SET manifest = ?1 WHERE id = ?2",
            params![manifest, id],
        )
        .map_err(|e| format!("update_checkpoint_manifest: {}", e))?;
        Ok(())
    }

    /// The most recent checkpoint created strictly before `created_at_ms`
    /// (i.e. the state right before the recalled turn). None when the session
    /// is not a git repo or nothing was change-worthy.
    pub fn latest_checkpoint_before(
        &self,
        session_id: &str,
        created_at_ms: i64,
    ) -> Option<CheckpointRow> {
        let conn = self.conn.lock().unwrap();
        conn.query_row(
            "SELECT id, session_id, turn_seq, git_ref, manifest, created_at_ms
             FROM checkpoints
             WHERE session_id = ?1 AND created_at_ms < ?2
             ORDER BY created_at_ms DESC
             LIMIT 1",
            params![session_id, created_at_ms],
            |row| {
                Ok(CheckpointRow {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    turn_seq: row.get(2)?,
                    git_ref: row.get(3)?,
                    manifest: row.get(4)?,
                    created_at_ms: row.get(5)?,
                })
            },
        )
        .ok()
    }

    /// Keep only the newest `keep` rows per session. Returns the ids of the
    /// pruned rows — their snapshot dirs live at `<data_dir>/checkpoints/<id>/`
    /// and are cleaned up by the caller (gateway/checkpoint.rs).
    pub fn prune_checkpoints(&self, session_id: &str, keep: usize) -> Result<Vec<i64>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT id FROM checkpoints
                 WHERE session_id = ?1
                 ORDER BY created_at_ms DESC
                 LIMIT -1 OFFSET ?2",
            )
            .map_err(|e| format!("prune prepare: {}", e))?;
        let pruned_ids: Vec<i64> = stmt
            .query_map(params![session_id, keep as i64], |row| row.get::<_, i64>(0))
            .map_err(|e| format!("prune query: {}", e))?
            .filter_map(|r| r.ok())
            .collect();
        for id in &pruned_ids {
            conn.execute("DELETE FROM checkpoints WHERE id = ?1", params![id])
                .map_err(|e| format!("prune delete: {}", e))?;
        }
        Ok(pruned_ids)
    }

    /// Delete all checkpoints of a session (used when the session is deleted).
    pub fn delete_checkpoints(&self, session_id: &str) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        conn.execute(
            "DELETE FROM checkpoints WHERE session_id = ?1",
            params![session_id],
        )
        .map_err(|e| format!("delete_checkpoints: {}", e))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkpoint_roundtrip_and_lookup() {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(dir.path()).unwrap();

        assert!(db.latest_checkpoint_before("s1", i64::MAX).is_none());

        let t0 = crate::gateway::now_epoch_ms();
        let id1 = db.insert_checkpoint("s1", 1, "ref1", "[]").unwrap();
        db.update_checkpoint_manifest(id1, r#"[{"path":"a.txt","snapshot":"/tmp/cp"}]"#).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(5));
        let t1 = crate::gateway::now_epoch_ms();
        db.insert_checkpoint("s1", 2, "ref2", "[]").unwrap();

        // 在两次插入之间取"之前最近" → id1（created_at_ms 严格小于 t1）。
        let row = db.latest_checkpoint_before("s1", t1).unwrap();
        assert_eq!(row.id, id1);
        assert_eq!(row.git_ref, "ref1");
        assert!(row.manifest.contains("a.txt"));

        // 早于第一条 → none；其它 session → none。
        assert!(db.latest_checkpoint_before("s1", t0).is_none());
        assert!(db.latest_checkpoint_before("s2", i64::MAX).is_none());

        // Prune keeps newest `keep` and reports the pruned id.
        let pruned = db.prune_checkpoints("s1", 1).unwrap();
        assert_eq!(pruned, vec![id1]);
        assert!(db.latest_checkpoint_before("s1", i64::MAX).is_some());

        // Delete clears everything.
        db.delete_checkpoints("s1").unwrap();
        assert!(db.latest_checkpoint_before("s1", i64::MAX).is_none());
    }
}
