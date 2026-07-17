use std::path::Path;

use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::validate_user_note_relative_path;

use super::ToolDispatchContext;

pub(super) async fn read_note(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    ctx.ensure_retrieval_scope_allows_path(&state.db, path)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, path)?;
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
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let prefix = args["prefix"].as_str().unwrap_or("");
    let limit = (args["limit"].as_u64().unwrap_or(50) as usize).clamp(1, 100);
    let items = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT path, title FROM files
             WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
               AND path NOT LIKE '.iris/%'
               AND path <> '.classified'
               AND path NOT LIKE '.classified/%'
               AND (?1 = '' OR path LIKE ?2)
             ORDER BY path",
        )?;
        let pattern = format!("{prefix}%");
        let rows = stmt.query_map(rusqlite::params![prefix, pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;
        let mut items = Vec::new();
        for row in rows {
            let (path, title) = row?;
            if ctx.retrieval_scope.allows_path(conn, &path)? {
                items.push(serde_json::json!({ "path": path, "title": title }));
                if items.len() == limit {
                    break;
                }
            }
        }
        Ok(items)
    })?;
    Ok(serde_json::json!({ "files": items, "count": items.len() }))
}

pub(super) async fn get_outline(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    ctx.ensure_retrieval_scope_allows_path(&state.db, path)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, path)?;
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
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    ctx.ensure_retrieval_scope_allows_path(&state.db, path)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, path)?;
    let vault = state.vault_path()?;
    let _abs = validate_user_note_relative_path(&vault, path)?;
    let entries = state.db.with_read_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT f.path, f.title, l.context
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
               AND f.path <> '.classified'
               AND f.path NOT LIKE '.classified/%'
               AND t.path <> '.classified'
               AND t.path NOT LIKE '.classified/%'
             ORDER BY f.title",
        )?;
        let rows = stmt.query_map([path], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, Option<String>>(2)?,
            ))
        })?;
        let mut entries = Vec::new();
        for row in rows {
            let (source_path, source_title, context) = row?;
            if ctx.retrieval_scope.allows_path(conn, &source_path)? {
                entries.push(serde_json::json!({
                    "source_path": source_path,
                    "source_title": source_title,
                    "context": context,
                }));
            }
        }
        Ok(entries)
    })?;
    Ok(serde_json::json!({ "backlinks": entries, "count": entries.len() }))
}

pub(super) async fn get_block_links(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let note_path = args["note_path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing note_path"))?;
    ctx.ensure_retrieval_scope_allows_path(&state.db, note_path)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, note_path)?;
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
               AND (tf.path IS NULL OR (tf.path <> '.classified' AND tf.path NOT LIKE '.classified/%'))
             LIMIT 30",
        )?;
        let rows = stmt.query_map([fid], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, Option<String>>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, i64>(3)? != 0,
            ))
        })?;
        let mut links = Vec::new();
        for row in rows {
            let (id, target_path, link_type, is_confirmed) = row?;
            let Some(target_path) = target_path else {
                continue;
            };
            if !ctx
                .retrieval_scope
                .allows_path(conn, &target_path)
                .unwrap_or(false)
            {
                continue;
            }
            links.push(serde_json::json!({
                "id": id,
                "target_path": target_path,
                "link_type": link_type,
                "is_confirmed": is_confirmed,
            }));
        }
        Ok(links)
    })?;
    Ok(serde_json::json!({ "links": links }))
}
