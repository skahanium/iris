//! Scene-free normal-domain Session persistence for the unified Run runtime.
//!
//! Sessions carry only opaque identity and user-visible metadata. They have no
//! routing, scene, or note-target binding.

use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

/// Opaque identity of one normal-domain conversation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalSession {
    /// SQLite primary key used by Run and message foreign keys.
    pub(crate) session_id: i64,
    /// Opaque client-facing key with no scene or document meaning.
    pub(crate) session_key: String,
}

/// Public-history projection for one normal-domain conversation.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalSessionSummary {
    pub(crate) session_key: String,
    pub(crate) title: String,
    pub(crate) message_count: u32,
    pub(crate) created_at: String,
    pub(crate) updated_at: String,
}

/// Public-history projection for one normal-domain message.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NormalSessionMessage {
    pub(crate) seq: i64,
    pub(crate) role: String,
    pub(crate) content: String,
    pub(crate) content_parts: Option<String>,
    pub(crate) tool_calls: Option<serde_json::Value>,
    pub(crate) context_scope: serde_json::Value,
    pub(crate) display_mentions: Vec<serde_json::Value>,
    pub(crate) created_at: String,
}

/// Repository for scene-free normal-domain sessions.
pub(crate) struct NormalSessionRepository;

impl NormalSessionRepository {
    /// Create one normal-domain session without an implicit scene or note target.
    pub(crate) fn create(db: &Database) -> AppResult<NormalSession> {
        let session_key = format!("run_session:{}", uuid::Uuid::new_v4());
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            conn.execute(
                "INSERT INTO sessions (session_key, created_at, updated_at)
                 VALUES (?1, ?2, ?2)",
                rusqlite::params![session_key, now],
            )?;
            Ok(NormalSession {
                session_id: conn.last_insert_rowid(),
                session_key,
            })
        })
    }

    /// Resolve an opaque normal-domain session key without reading legacy bindings.
    pub(crate) fn get(db: &Database, session_key: &str) -> AppResult<Option<NormalSession>> {
        db.with_read_conn(|conn| {
            let result = conn.query_row(
                "SELECT id, session_key FROM sessions WHERE session_key = ?1",
                [session_key],
                |row| {
                    Ok(NormalSession {
                        session_id: row.get(0)?,
                        session_key: row.get(1)?,
                    })
                },
            );
            match result {
                Ok(session) => Ok(Some(session)),
                Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
                Err(error) => Err(error.into()),
            }
        })
    }
    /// List conversations without reading legacy scene or document bindings.
    pub(crate) fn list(
        db: &Database,
        limit: u32,
        offset: u32,
    ) -> AppResult<Vec<NormalSessionSummary>> {
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT s.session_key, s.title, s.created_at, s.updated_at,
                        (SELECT COUNT(*) FROM session_messages m WHERE m.session_id = s.id),
                        (SELECT content FROM session_messages m
                         WHERE m.session_id = s.id AND m.role = 'user'
                         ORDER BY m.seq ASC LIMIT 1)
                 FROM sessions s
                 ORDER BY s.updated_at DESC
                 LIMIT ?1 OFFSET ?2",
            )?;
            let rows = statement.query_map(rusqlite::params![limit, offset], |row| {
                let stored_title: Option<String> = row.get(1)?;
                let first_user: Option<String> = row.get(5)?;
                Ok(NormalSessionSummary {
                    session_key: row.get(0)?,
                    title: stored_title.unwrap_or_else(|| derive_title(first_user.as_deref())),
                    message_count: row.get::<_, i64>(4)? as u32,
                    created_at: row.get(2)?,
                    updated_at: row.get(3)?,
                })
            })?;
            rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
        })
    }

    /// Load messages through the opaque session key.
    pub(crate) fn load_messages(
        db: &Database,
        session_key: &str,
        limit: u32,
    ) -> AppResult<Vec<NormalSessionMessage>> {
        let session = Self::get(db, session_key)?
            .ok_or_else(|| AppError::msg("assistant session not found"))?;
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT seq, role, content, content_parts, tool_calls, created_at,
                        context_scope_json, display_mentions_json
                 FROM session_messages
                 WHERE session_id = ?1
                 ORDER BY seq DESC
                 LIMIT ?2",
            )?;
            let rows =
                statement.query_map(rusqlite::params![session.session_id, limit], |row| {
                    Ok(NormalSessionMessage {
                        seq: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                        content_parts: row.get(3)?,
                        tool_calls: row
                            .get::<_, Option<String>>(4)?
                            .and_then(|value| serde_json::from_str(&value).ok()),
                        created_at: row.get(5)?,
                        context_scope: parse_json_value_or_empty_array(row.get(6)?),
                        display_mentions: parse_json_array_or_empty(row.get(7)?),
                    })
                })?;
            let mut messages = rows.collect::<Result<Vec<_>, _>>()?;
            messages.reverse();
            Ok(messages)
        })
    }

    /// Load recent messages for a Run-owned session id without scene routing.
    pub(crate) fn recent_messages(
        db: &Database,
        session_id: i64,
        limit: u32,
    ) -> AppResult<Vec<NormalSessionMessage>> {
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT seq, role, content, content_parts, tool_calls, created_at,
                        context_scope_json, display_mentions_json
                 FROM session_messages
                 WHERE session_id = ?1
                 ORDER BY seq DESC
                 LIMIT ?2",
            )?;
            let rows = statement.query_map(rusqlite::params![session_id, limit], |row| {
                Ok(NormalSessionMessage {
                    seq: row.get(0)?,
                    role: row.get(1)?,
                    content: row.get(2)?,
                    content_parts: row.get(3)?,
                    tool_calls: row
                        .get::<_, Option<String>>(4)?
                        .and_then(|value| serde_json::from_str(&value).ok()),
                    created_at: row.get(5)?,
                    context_scope: parse_json_value_or_empty_array(row.get(6)?),
                    display_mentions: parse_json_array_or_empty(row.get(7)?),
                })
            })?;
            let mut messages = rows.collect::<Result<Vec<_>, _>>()?;
            messages.reverse();
            Ok(messages)
        })
    }

    /// Load bounded conversation history strictly before one Run's current message.
    pub(crate) fn recent_messages_before(
        db: &Database,
        session_id: i64,
        before_seq: i64,
        limit: u32,
    ) -> AppResult<Vec<NormalSessionMessage>> {
        db.with_read_conn(|conn| {
            let mut statement = conn.prepare(
                "SELECT seq, role, content, content_parts, tool_calls, created_at,
                        context_scope_json, display_mentions_json
                 FROM session_messages
                 WHERE session_id = ?1 AND seq < ?2 AND role IN ('user', 'assistant')
                 ORDER BY seq DESC
                 LIMIT ?3",
            )?;
            let rows =
                statement.query_map(rusqlite::params![session_id, before_seq, limit], |row| {
                    Ok(NormalSessionMessage {
                        seq: row.get(0)?,
                        role: row.get(1)?,
                        content: row.get(2)?,
                        content_parts: row.get(3)?,
                        tool_calls: row
                            .get::<_, Option<String>>(4)?
                            .and_then(|value| serde_json::from_str(&value).ok()),
                        created_at: row.get(5)?,
                        context_scope: parse_json_value_or_empty_array(row.get(6)?),
                        display_mentions: parse_json_array_or_empty(row.get(7)?),
                    })
                })?;
            let mut messages = rows.collect::<Result<Vec<_>, _>>()?;
            messages.reverse();
            Ok(messages)
        })
    }
    /// Rename a conversation by opaque key.
    pub(crate) fn rename(db: &Database, session_key: &str, title: &str) -> AppResult<()> {
        let title = title.trim();
        if title.is_empty() {
            return Err(AppError::msg("assistant session title cannot be empty"));
        }
        let now = chrono::Utc::now().to_rfc3339();
        db.with_conn(|conn| {
            let updated = conn.execute(
                "UPDATE sessions SET title = ?1, updated_at = ?2 WHERE session_key = ?3",
                rusqlite::params![title, now, session_key],
            )?;
            if updated == 0 {
                return Err(AppError::msg("assistant session not found"));
            }
            Ok(())
        })
    }

    /// Delete a conversation by opaque key.
    pub(crate) fn delete(db: &Database, session_key: &str) -> AppResult<bool> {
        db.with_conn(|conn| {
            let deleted =
                conn.execute("DELETE FROM sessions WHERE session_key = ?1", [session_key])?;
            Ok(deleted > 0)
        })
    }

    /// Delete sessions whose last update is older than the configured retention window.
    pub(crate) fn purge_expired(db: &Database, retention_days: u32) -> AppResult<usize> {
        db.with_conn(|conn| {
            let deleted = conn.execute(
                "DELETE FROM sessions WHERE updated_at < datetime('now', ?1)",
                [format!("-{retention_days} days")],
            )?;
            Ok(deleted)
        })
    }
    /// Retract a message suffix by opaque key and retire associated normal evidence.
    pub(crate) fn retract(db: &Database, session_key: &str, from_seq: i64) -> AppResult<u32> {
        if from_seq <= 0 {
            return Err(AppError::msg("assistant session sequence must be positive"));
        }
        let session = Self::get(db, session_key)?
            .ok_or_else(|| AppError::msg("assistant session not found"))?;
        db.with_conn(|conn| {
            let deleted = conn.execute(
                "DELETE FROM session_messages WHERE session_id = ?1 AND seq >= ?2",
                rusqlite::params![session.session_id, from_seq],
            )?;
            if deleted > 0 {
                conn.execute(
                    "UPDATE session_evidence
                     SET retired_at = ?1
                     WHERE session_id = ?2 AND message_seq_first >= ?3 AND retired_at IS NULL",
                    rusqlite::params![
                        chrono::Utc::now().to_rfc3339(),
                        session.session_id,
                        from_seq,
                    ],
                )?;
                conn.execute(
                    "UPDATE sessions SET updated_at = ?1 WHERE id = ?2",
                    rusqlite::params![chrono::Utc::now().to_rfc3339(), session.session_id],
                )?;
            }
            Ok(deleted as u32)
        })
    }
}

fn derive_title(first_user_message: Option<&str>) -> String {
    let trimmed = first_user_message.unwrap_or("新对话").trim();
    if trimmed.is_empty() {
        return "新对话".into();
    }
    let title: String = trimmed.chars().take(40).collect();
    if trimmed.chars().count() > 40 {
        format!("{title}…")
    } else {
        title
    }
}

fn parse_json_value_or_empty_array(value: Option<String>) -> serde_json::Value {
    value
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_else(|| serde_json::json!([]))
}

fn parse_json_array_or_empty(value: Option<String>) -> Vec<serde_json::Value> {
    value
        .as_deref()
        .and_then(|json| serde_json::from_str(json).ok())
        .unwrap_or_default()
}
