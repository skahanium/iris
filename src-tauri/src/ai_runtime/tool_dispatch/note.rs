use std::path::Path;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::validate_user_note_relative_path;

pub(super) async fn read_note(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let abs = validate_user_note_relative_path(&vault, path)?;
    let content = std::fs::read_to_string(abs)?;
    let max_chars = args["max_chars"].as_u64().unwrap_or(12_000) as usize;
    let truncated = content.chars().count() > max_chars;
    let body: String = content.chars().take(max_chars).collect();
    Ok(serde_json::json!({
        "path": path,
        "content": body,
        "truncated": truncated,
    }))
}

pub(super) async fn list_vault(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let prefix = args["prefix"].as_str().unwrap_or("");
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;
    let items = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT path, title FROM files
             WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
               AND path NOT LIKE '.iris/%'
               AND path NOT LIKE '.classified/%'
               AND (?1 = '' OR path LIKE ?2)
             ORDER BY path
             LIMIT ?3",
        )?;
        let pattern = format!("{prefix}%");
        let rows = stmt.query_map(rusqlite::params![prefix, pattern, limit as i64], |row| {
            Ok(serde_json::json!({
                "path": row.get::<_, String>(0)?,
                "title": row.get::<_, String>(1)?,
            }))
        })?;
        Ok(rows.flatten().collect::<Vec<_>>())
    })?;
    Ok(serde_json::json!({ "files": items, "count": items.len() }))
}

pub(super) async fn get_outline(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let abs = validate_user_note_relative_path(&vault, path)?;
    let content = std::fs::read_to_string(abs)?;
    let headings: Vec<serde_json::Value> = content
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim();
            if !trimmed.starts_with('#') {
                return None;
            }
            let level = trimmed.chars().take_while(|c| *c == '#').count();
            let text = trimmed.trim_start_matches('#').trim();
            if text.is_empty() {
                return None;
            }
            Some(serde_json::json!({ "level": level, "text": text }))
        })
        .collect();
    Ok(serde_json::json!({ "path": path, "headings": headings }))
}

pub(super) async fn get_backlinks(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let _abs = validate_user_note_relative_path(&vault, path)?;
    let entries = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT f.path, f.title, l.context
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
               AND f.path NOT LIKE '.classified/%'
             ORDER BY f.title",
        )?;
        let rows = stmt.query_map([path], |row| {
            Ok(serde_json::json!({
                "source_path": row.get::<_, String>(0)?,
                "source_title": row.get::<_, String>(1)?,
                "context": row.get::<_, Option<String>>(2)?,
            }))
        })?;
        Ok(rows.flatten().collect::<Vec<_>>())
    })?;
    Ok(serde_json::json!({ "backlinks": entries, "count": entries.len() }))
}

pub(super) async fn get_block_links(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let note_path = args["note_path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing note_path"))?;
    let vault: &Path = &state.vault_path()?;
    let _abs = validate_user_note_relative_path(vault, note_path)?;
    let links = state.db.with_read_conn(|conn| {
        let file_id: Option<i64> = conn
            .query_row("SELECT id FROM files WHERE path = ?1", [note_path], |r| {
                r.get(0)
            })
            .ok();
        let Some(fid) = file_id else {
            return Ok(vec![]);
        };
        let mut stmt = conn.prepare(
            "SELECT bl.id, tf.path, bl.link_type, bl.is_confirmed
             FROM block_links bl
             LEFT JOIN files tf ON tf.id = bl.target_file_id
             WHERE bl.source_file_id = ?1
             LIMIT 30",
        )?;
        let rows = stmt.query_map([fid], |row| {
            Ok(serde_json::json!({
                "id": row.get::<_, i64>(0)?,
                "target_path": row.get::<_, Option<String>>(1)?,
                "link_type": row.get::<_, String>(2)?,
                "is_confirmed": row.get::<_, i64>(3)? != 0,
            }))
        })?;
        Ok(rows.flatten().collect::<Vec<_>>())
    })?;
    Ok(serde_json::json!({ "links": links }))
}
