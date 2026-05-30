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
    pub title: Option<String>,
    pub retention_policy: String,
    pub created_at: String,
    pub updated_at: String,
}

/// 会话列表项（供历史下拉使用）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionSummary {
    pub id: i64,
    pub title: String,
    pub scene: String,
    pub note_path: Option<String>,
    pub message_count: u32,
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
    /// 创建新的独立会话（同 scene + 笔记路径下开启新对话线程，不续接旧消息）。
    pub fn create_fresh(db: &Database, scene: AiScene, note_path: Option<&str>) -> AppResult<i64> {
        let key = format!(
            "{}#{}",
            session_key(scene, note_path),
            uuid::Uuid::new_v4()
        );
        let now = chrono::Utc::now().to_rfc3339();

        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions (session_key, scene, note_path, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?4)",
                rusqlite::params![key, scene.profile(), note_path, now],
            )?;
            Ok(conn.last_insert_rowid())
        })
    }

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
                "SELECT id, session_key, scene, note_path, title, retention_policy, created_at, updated_at
                 FROM sessions WHERE id = ?1",
                [session_id],
                |row| {
                    Ok(Session {
                        id: row.get(0)?,
                        session_key: row.get(1)?,
                        scene: row.get(2)?,
                        note_path: row.get(3)?,
                        title: row.get(4)?,
                        retention_policy: row.get(5)?,
                        created_at: row.get(6)?,
                        updated_at: row.get(7)?,
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

    /// 列出会话（按 updated_at 降序）。
    pub fn list_sessions(
        db: &Database,
        scene: Option<&str>,
        note_path: Option<&str>,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<SessionSummary>> {
        db.with_conn(|conn| {
            let mut sql = String::from(
                "SELECT s.id, s.scene, s.note_path, s.title, s.created_at, s.updated_at,
                        (SELECT COUNT(*) FROM session_messages m WHERE m.session_id = s.id) AS message_count,
                        (SELECT content FROM session_messages m
                         WHERE m.session_id = s.id AND m.role = 'user'
                         ORDER BY m.seq ASC LIMIT 1) AS first_user
                 FROM sessions s
                 WHERE 1=1",
            );
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(sc) = scene {
                sql.push_str(" AND s.scene = ?");
                params.push(Box::new(sc.to_string()));
            }
            if let Some(np) = note_path {
                sql.push_str(" AND s.note_path = ?");
                params.push(Box::new(np.to_string()));
            }
            sql.push_str(" ORDER BY s.updated_at DESC LIMIT ? OFFSET ?");
            params.push(Box::new(limit));
            params.push(Box::new(offset));

            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let mut stmt = conn.prepare(&sql)?;
            let rows = stmt.query_map(param_refs.as_slice(), |row| {
                let stored_title: Option<String> = row.get(3)?;
                let first_user: Option<String> = row.get(7)?;
                let title = stored_title.unwrap_or_else(|| {
                    derive_session_title(first_user.as_deref().unwrap_or("新对话"))
                });
                Ok(SessionSummary {
                    id: row.get(0)?,
                    title,
                    scene: row.get(1)?,
                    note_path: row.get(2)?,
                    message_count: row.get::<_, i64>(6)? as u32,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                })
            })?;
            let mut out = Vec::new();
            for row in rows {
                out.push(row?);
            }
            Ok(out)
        })
    }

    /// 按 ID 删除会话（级联删除消息）。
    pub fn delete_session(db: &Database, session_id: i64) -> AppResult<bool> {
        db.with_conn(|conn| {
            let count = conn.execute("DELETE FROM sessions WHERE id = ?1", [session_id])?;
            Ok(count > 0)
        })
    }

    /// 删除当前筛选条件下的全部会话（级联消息）。
    pub fn delete_all_filtered(
        db: &Database,
        scene: Option<&str>,
        note_path: Option<&str>,
    ) -> AppResult<u32> {
        db.with_conn(|conn| {
            let mut sql = String::from("DELETE FROM sessions WHERE 1=1");
            let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
            if let Some(sc) = scene {
                sql.push_str(" AND scene = ?");
                params.push(Box::new(sc.to_string()));
            }
            if let Some(np) = note_path {
                sql.push_str(" AND note_path = ?");
                params.push(Box::new(np.to_string()));
            }
            let param_refs: Vec<&dyn rusqlite::ToSql> =
                params.iter().map(|p| p.as_ref()).collect();
            let count = conn.execute(&sql, param_refs.as_slice())?;
            Ok(count as u32)
        })
    }

    /// 重命名会话标题。
    pub fn rename_session(db: &Database, session_id: i64, new_title: &str) -> AppResult<()> {
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_title, now, session_id],
            )?;
            Ok(())
        })
    }
}

fn derive_session_title(first_user_message: &str) -> String {
    let trimmed = first_user_message.trim();
    if trimmed.is_empty() {
        return "新对话".to_string();
    }
    let chars: String = trimmed.chars().take(40).collect();
    if trimmed.chars().count() > 40 {
        format!("{chars}…")
    } else {
        chars
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
    fn list_rename_and_delete_session() {
        let db = setup_db();
        let sid = SessionManager::create_fresh(&db, AiScene::KnowledgeLookup, None).unwrap();
        SessionManager::append_message(&db, sid, "user", "第一条用户消息用于标题", None).unwrap();
        let list = SessionManager::list_sessions(&db, Some("knowledge_lookup"), None, 10, 0)
            .unwrap();
        assert!(!list.is_empty());
        assert!(list[0].title.contains("第一条"));

        SessionManager::rename_session(&db, sid, "自定义标题").unwrap();
        let list2 = SessionManager::list_sessions(&db, None, None, 10, 0).unwrap();
        assert!(list2.iter().any(|s| s.title == "自定义标题"));

        assert!(SessionManager::delete_session(&db, sid).unwrap());
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
