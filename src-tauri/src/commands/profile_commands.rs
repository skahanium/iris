//! Personalization IPC commands.
//!
//! Manages user_profile CRUD, knowledge_deposits inbox,
//! and user rule management (§8.2, §8.4, §E).

use crate::app::AppState;
use crate::error::AppResult;
use serde::{Deserialize, Serialize};
use tauri::State;

// ─── Profile Types ───────────────────────────────────────

/// A user profile entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProfileEntry {
    pub key: String,
    pub value: serde_json::Value,
    pub source: String,
    pub confidence: f64,
    pub is_active: bool,
    pub updated_at: String,
}

/// A knowledge deposit (inbox item).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeDeposit {
    pub id: i64,
    pub session_id: Option<i64>,
    pub source_note: Option<String>,
    pub deposit_type: String,
    pub content: String,
    pub status: String,
    pub target_path: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

// ─── Profile Commands ────────────────────────────────────

/// List all user profile entries.
#[tauri::command]
pub fn profile_list(
    state: State<'_, AppState>,
    include_inactive: Option<bool>,
) -> AppResult<Vec<ProfileEntry>> {
    let include = include_inactive.unwrap_or(false);

    state.db.with_conn(|conn| {
        let sql = if include {
            "SELECT key, value, source, confidence, is_active, updated_at FROM user_profile ORDER BY key"
        } else {
            "SELECT key, value, source, confidence, is_active, updated_at FROM user_profile WHERE is_active = 1 ORDER BY key"
        };

        let mut stmt = conn.prepare(sql)?;
        let rows = stmt.query_map([], |row| {
            let is_active_int: i64 = row.get(4)?;
            Ok(ProfileEntry {
                key: row.get(0)?,
                value: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                source: row.get(2)?,
                confidence: row.get(3)?,
                is_active: is_active_int != 0,
                updated_at: row.get(5)?,
            })
        })?;

        Ok(rows.flatten().collect())
    })
}

/// Get a single profile entry by key.
#[tauri::command]
pub fn profile_get(state: State<'_, AppState>, key: String) -> AppResult<Option<ProfileEntry>> {
    state.db.with_conn(|conn| {
        let result = conn.query_row(
            "SELECT key, value, source, confidence, is_active, updated_at FROM user_profile WHERE key = ?1",
            [&key],
            |row| {
                let is_active_int: i64 = row.get(4)?;
                Ok(ProfileEntry {
                    key: row.get(0)?,
                    value: serde_json::from_str(&row.get::<_, String>(1)?).unwrap_or_default(),
                    source: row.get(2)?,
                    confidence: row.get(3)?,
                    is_active: is_active_int != 0,
                    updated_at: row.get(5)?,
                })
            },
        );

        match result {
            Ok(entry) => Ok(Some(entry)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e.into()),
        }
    })
}

/// Set (upsert) a user profile entry.
#[tauri::command]
pub fn profile_set(
    state: State<'_, AppState>,
    key: String,
    value: serde_json::Value,
    source: String,
    confidence: Option<f64>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    let conf = confidence.unwrap_or(1.0);
    let json_str = serde_json::to_string(&value)?;

    state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO user_profile (key, value, source, confidence, is_active, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, ?5)
             ON CONFLICT(key) DO UPDATE SET
               value = excluded.value,
               source = excluded.source,
               confidence = excluded.confidence,
               is_active = 1,
               updated_at = excluded.updated_at",
            rusqlite::params![key, json_str, source, conf, now],
        )?;
        Ok(())
    })
}

/// Deactivate a profile entry (soft delete).
#[tauri::command]
pub fn profile_deactivate(state: State<'_, AppState>, key: String) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();
    state.db.with_conn(|conn| {
        conn.execute(
            "UPDATE user_profile SET is_active = 0, updated_at = ?1 WHERE key = ?2",
            rusqlite::params![now, key],
        )?;
        Ok(())
    })
}

/// Delete a profile entry permanently.
#[tauri::command]
pub fn profile_delete(state: State<'_, AppState>, key: String) -> AppResult<()> {
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM user_profile WHERE key = ?1", [&key])?;
        Ok(())
    })
}

// ─── Knowledge Inbox Commands ────────────────────────────

/// List knowledge deposits (inbox items).
#[tauri::command]
pub fn inbox_list(
    state: State<'_, AppState>,
    status: Option<String>,
) -> AppResult<Vec<KnowledgeDeposit>> {
    let status_filter = status.unwrap_or_else(|| "inbox".to_string());

    state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT id, session_id, source_note, deposit_type, content, status, target_path, created_at, updated_at
             FROM knowledge_deposits WHERE status = ?1 ORDER BY created_at DESC"
        )?;

        let rows = stmt.query_map([&status_filter], |row| {
            Ok(KnowledgeDeposit {
                id: row.get(0)?,
                session_id: row.get(1)?,
                source_note: row.get(2)?,
                deposit_type: row.get(3)?,
                content: row.get(4)?,
                status: row.get(5)?,
                target_path: row.get(6)?,
                created_at: row.get(7)?,
                updated_at: row.get(8)?,
            })
        })?;

        Ok(rows.flatten().collect())
    })
}

/// Create a new knowledge deposit (add to inbox).
#[tauri::command]
pub fn inbox_add(
    state: State<'_, AppState>,
    deposit_type: String,
    content: String,
    source_note: Option<String>,
    session_id: Option<i64>,
) -> AppResult<i64> {
    let now = chrono::Utc::now().to_rfc3339();

    state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO knowledge_deposits (session_id, source_note, deposit_type, content, status, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'inbox', ?5, ?5)",
            rusqlite::params![session_id, source_note, deposit_type, content, now],
        )?;
        Ok(conn.last_insert_rowid())
    })
}

/// Update deposit status (e.g., 'inbox' → 'archived' → 'written').
#[tauri::command]
pub fn inbox_update_status(
    state: State<'_, AppState>,
    deposit_id: i64,
    new_status: String,
    target_path: Option<String>,
) -> AppResult<()> {
    let now = chrono::Utc::now().to_rfc3339();

    state.db.with_conn(|conn| {
        if let Some(path) = target_path {
            conn.execute(
                "UPDATE knowledge_deposits SET status = ?1, target_path = ?2, updated_at = ?3 WHERE id = ?4",
                rusqlite::params![new_status, path, now, deposit_id],
            )?;
        } else {
            conn.execute(
                "UPDATE knowledge_deposits SET status = ?1, updated_at = ?2 WHERE id = ?3",
                rusqlite::params![new_status, now, deposit_id],
            )?;
        }
        Ok(())
    })
}

/// Delete a knowledge deposit.
#[tauri::command]
pub fn inbox_delete(state: State<'_, AppState>, deposit_id: i64) -> AppResult<()> {
    state.db.with_conn(|conn| {
        conn.execute("DELETE FROM knowledge_deposits WHERE id = ?1", [deposit_id])?;
        Ok(())
    })
}

/// Get inbox counts by status.
#[tauri::command]
pub fn inbox_counts(state: State<'_, AppState>) -> AppResult<serde_json::Value> {
    state.db.with_conn(|conn| {
        let inbox: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_deposits WHERE status = 'inbox'",
            [],
            |r| r.get(0),
        )?;
        let archived: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_deposits WHERE status = 'archived'",
            [],
            |r| r.get(0),
        )?;
        let written: i64 = conn.query_row(
            "SELECT COUNT(*) FROM knowledge_deposits WHERE status = 'written'",
            [],
            |r| r.get(0),
        )?;

        Ok(serde_json::json!({
            "inbox": inbox,
            "archived": archived,
            "written": written,
        }))
    })
}
