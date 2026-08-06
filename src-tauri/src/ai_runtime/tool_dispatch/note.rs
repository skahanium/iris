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
    ctx.ensure_document_capability(
        path,
        crate::ai_runtime::policy_decision_engine::DocumentCapability::Read,
    )?;
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
        // Evidence registration must use the source that was actually read,
        // rather than treating the (possibly truncated) model payload as the
        // whole note. These fields remain internal tool-result metadata.
        "contentHash": crate::cas::hash::content_hash_str(&content),
        "sourceSpan": { "start": 0, "end": content.len() },
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
            if ctx
                .ensure_document_capability(
                    &path,
                    crate::ai_runtime::policy_decision_engine::DocumentCapability::Discover,
                )
                .is_ok()
                && ctx.retrieval_scope.allows_path(conn, &path)?
            {
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
    ctx.ensure_document_capability(
        path,
        crate::ai_runtime::policy_decision_engine::DocumentCapability::Read,
    )?;
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
    ctx.ensure_document_capability(
        path,
        crate::ai_runtime::policy_decision_engine::DocumentCapability::Read,
    )?;
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
