//! Session and session_messages management.
//!
//! Sessions are identified by `session_key = scene + ":" + (note_path || "__global__")`.

use crate::ai_runtime::AiScene;
use crate::error::AppResult;
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};

/// Session 元数据。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Session {
    pub id: i64,
    pub session_key: String,
    pub scene: String,
    pub note_path: Option<String>,
    pub retention_policy: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Session 消息记录。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: i64,
    pub session_id: i64,
    pub seq: i64,
    pub role: String,
    pub content: String,
    pub tool_calls: Option<serde_json::Value>,
    pub content_hash: Option<String>,
    pub created_at: String,
}

/// 构建 session_key。
pub fn session_key(scene: AiScene, note_path: Option<&str>) -> String {
    let scene_str = scene.profile();
    match note_path {
        Some(path) if !path.is_empty() => format!("{scene_str}:{path}"),
        _ => format!("{scene_str}:__global__"),
    }
}

pub struct SessionManager;

impl SessionManager {
    /// 获取或创建 session。返回 session id。
    pub fn ensure(db: &Database, scene: AiScene, note_path: Option<&str>) -> AppResult<i64> {
        let key = session_key(scene, note_path);
        let now = chrono::Utc::now().to_rfc3339();

        db.with_conn(|conn| {
            // Try insert; if conflict, update updated_at and return existing id
            conn.execute(
                "INSERT INTO sessions (session_key, scene, note_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)
                 ON CONFLICT(session_key) DO UPDATE SET updated_at = excluded.updated_at",
                rusqlite::params![key, scene.profile(), note_path, now],
            )?;

            let id: i64 = conn.query_row(
                "SELECT id FROM sessions WHERE session_key = ?1",
                [&key],
                |row| row.get(0),
            )?;
            Ok(id)
        })
    }

    /// 向 session 追加一条消息。
    pub fn append_message(
        db: &Database,
        session_id: i64,
        role: &str,
        content: &str,
        tool_calls: Option<&serde_json::Value>,
    ) -> AppResult<i64> {
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            // Get next seq
            let seq: i64 = conn
                .query_row(
                    "SELECT COALESCE(MAX(seq), 0) + 1 FROM session_messages WHERE session_id = ?1",
                    [session_id],
                    |row| row.get(0),
                )
                .unwrap_or(1);

            let tool_json = tool_calls.map(|t| t.to_string());
            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, tool_calls, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![session_id, seq, role, content, tool_json, now],
            )?;

            // Update session updated_at
            conn.execute(
                "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                rusqlite::params![now, session_id],
            )?;

            Ok(conn.last_insert_rowid())
        })
    }

    /// 获取 session 最近 N 条消息。
    pub fn recent_messages(
        db: &Database,
        session_id: i64,
        limit: u32,
    ) -> AppResult<Vec<SessionMessage>> {
        db.with_conn(|conn| {
            let mut stmt = conn.prepare(
                "SELECT id, session_id, seq, role, content, tool_calls, content_hash, created_at
                 FROM session_messages
                 WHERE session_id = ?1
                 ORDER BY seq DESC
                 LIMIT ?2",
            )?;
            let rows = stmt.query_map(rusqlite::params![session_id, limit], |row| {
                Ok(SessionMessage {
                    id: row.get(0)?,
                    session_id: row.get(1)?,
                    seq: row.get(2)?,
                    role: row.get(3)?,
                    content: row.get(4)?,
                    tool_calls: row
                        .get::<_, Option<String>>(5)?
                        .and_then(|s| serde_json::from_str(&s).ok()),
                    content_hash: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?;
            let mut msgs = Vec::new();
            for row in rows {
                msgs.push(row?);
            }
            msgs.reverse(); // chronological order
            Ok(msgs)
        })
    }

    /// 按 session_key 删除整个 session（级联删除消息）。
    pub fn delete_by_key(db: &Database, key: &str) -> AppResult<bool> {
        db.with_conn(|conn| {
            let count = conn.execute("DELETE FROM sessions WHERE session_key = ?1", [key])?;
            Ok(count > 0)
        })
    }

    /// 清空所有会话（保留表结构）。
    pub fn clear_all(db: &Database) -> AppResult<usize> {
        db.with_conn(|conn| {
            let msg_count = conn.execute("DELETE FROM session_messages", [])?;
            let sess_count = conn.execute("DELETE FROM sessions", [])?;
            Ok(msg_count + sess_count)
        })
    }

    /// 获取某个 session 的摘要信息。
    pub fn get_session(db: &Database, session_id: i64) -> AppResult<Option<Session>> {
        db.with_conn(|conn| {
            let result = conn.query_row(
                "SELECT id, session_key, scene, note_path, retention_policy, created_at, updated_at
                 FROM sessions WHERE id = ?1",
                [session_id],
                |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        session_key: row.get(1)?,
                        scene: row.get(2)?,
                        note_path: row.get(3)?,
                        retention_policy: row.get(4)?,
                        created_at: row.get(5)?,
                        updated_at: row.get(6)?,
                    })
                },
            );
            match result {
                Ok(s) => Ok(Some(s)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(e) => Err(e.into()),
            }
        })
    }
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::storage::db::Database;

    fn setup_db() -> Database {
        Database::open_in_memory().unwrap()
    }

    #[test]
    fn session_key_generation() {
        let key = session_key(AiScene::KnowledgeLookup, None);
        assert_eq!(key, "knowledge_lookup:__global__");

        let key = session_key(AiScene::DraftingAssist, Some("/notes/report.md"));
        assert_eq!(key, "drafting_assist:/notes/report.md");
    }

    #[test]
    fn ensure_session_creates_and_reuses() {
        let db = setup_db();
        let id1 = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
        let id2 = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
        assert_eq!(id1, id2, "same session_key should return same id");
    }

    #[test]
    fn different_scenes_different_sessions() {
        let db = setup_db();
        let id1 = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
        let id2 = SessionManager::ensure(&db, AiScene::DraftingAssist, None).unwrap();
        assert_ne!(id1, id2);
    }

    #[test]
    fn append_and_retrieve_messages() {
        let db = setup_db();
        let sid = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();

        SessionManager::append_message(&db, sid, "user", "hello", None).unwrap();
        SessionManager::append_message(&db, sid, "assistant", "hi there", None).unwrap();

        let msgs = SessionManager::recent_messages(&db, sid, 10).unwrap();
        assert_eq!(msgs.len(), 2);
        assert_eq!(msgs[0].role, "user");
        assert_eq!(msgs[0].content, "hello");
        assert_eq!(msgs[1].role, "assistant");
        assert_eq!(msgs[1].content, "hi there");
    }

    #[test]
    fn delete_session_cascades_messages() {
        let db = setup_db();
        let sid = SessionManager::ensure(&db, AiScene::ExemplarLearning, Some("/notes/fanwen.md"))
            .unwrap();
        SessionManager::append_message(&db, sid, "user", "test", None).unwrap();

        let key = session_key(AiScene::ExemplarLearning, Some("/notes/fanwen.md"));
        let deleted = SessionManager::delete_by_key(&db, &key).unwrap();
        assert!(deleted);

        let msgs = SessionManager::recent_messages(&db, sid, 10).unwrap();
        assert!(msgs.is_empty());
    }

    #[test]
    fn clear_all_sessions() {
        let db = setup_db();
        SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
        SessionManager::ensure(&db, AiScene::DraftingAssist, Some("/notes/draft.md")).unwrap();

        let count = SessionManager::clear_all(&db).unwrap();
        assert!(count > 0);

        // New ensure should create fresh sessions
        let new_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
        assert!(new_id > 0);
    }
}
