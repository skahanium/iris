use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::validate_user_note_relative_path;

use super::ToolDispatchContext;

const DEFAULT_READ_NOTE_MAX_CHARS: usize = 12_000;
const MAX_READ_NOTE_CHARS: usize = 12_000;

fn ensure_note_model_read_allowed(ctx: &ToolDispatchContext<'_>, path: &str) -> AppResult<()> {
    use crate::ai_runtime::policy_decision_engine::DocumentCapability;

    for capability in [
        DocumentCapability::Discover,
        DocumentCapability::Read,
        DocumentCapability::SendToModel,
    ] {
        ctx.ensure_document_capability(path, capability)?;
    }
    Ok(())
}

pub(super) async fn read_note(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    ensure_note_model_read_allowed(ctx, path)?;
    ctx.ensure_retrieval_scope_allows_path(&state.db, path)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, path)?;
    let vault = state.vault_path()?;
    let abs = validate_user_note_relative_path(&vault, path)?;
    let content = std::fs::read_to_string(abs)?;
    let content_hash = crate::cas::hash::content_hash_str(&content);
    if args["content_hash"]
        .as_str()
        .is_some_and(|expected| expected != content_hash)
    {
        return Err(AppError::msg("read_note content hash mismatch"));
    }
    let start = args["start_byte"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(0);
    if start > content.len() || !content.is_char_boundary(start) {
        return Err(AppError::msg(
            "read_note start_byte must be a valid UTF-8 byte boundary",
        ));
    }
    let max_chars = args["max_chars"]
        .as_u64()
        .and_then(|value| usize::try_from(value).ok())
        .unwrap_or(DEFAULT_READ_NOTE_MAX_CHARS)
        .clamp(1, MAX_READ_NOTE_CHARS);
    let remaining = &content[start..];
    let relative_end = remaining
        .char_indices()
        .nth(max_chars)
        .map(|(index, _)| index)
        .unwrap_or(remaining.len());
    let end = start + relative_end;
    let truncated = end < content.len();
    let body = &content[start..end];
    Ok(serde_json::json!({
        "path": path,
        "content": body,
        "truncated": truncated,
        // Evidence registration must use the source that was actually read,
        // rather than treating the (possibly truncated) model payload as the
        // whole note. These fields remain internal tool-result metadata.
        "contentHash": content_hash,
        "sourceSpan": { "start": start, "end": end },
        "nextStartByte": truncated.then_some(end),
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
    ensure_note_model_read_allowed(ctx, path)?;
    ctx.ensure_retrieval_scope_allows_path(&state.db, path)?;
    ctx.ensure_active_skill_scope_allows_path(&state.db, path)?;
    let vault = state.vault_path()?;
    let abs = validate_user_note_relative_path(&vault, path)?;
    let content = std::fs::read_to_string(abs)?;
    let headings: Vec<serde_json::Value> = crate::indexer::chunker::markdown_headings(&content)
        .into_iter()
        .map(|heading| {
            serde_json::json!({
                "level": heading.level,
                "text": heading.text,
                "sourceSpan": { "start": heading.source_start, "end": heading.source_end },
            })
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
    ensure_note_model_read_allowed(ctx, path)?;
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
            if ensure_note_model_read_allowed(ctx, &source_path).is_ok()
                && ctx.retrieval_scope.allows_path(conn, &source_path)?
                && ctx
                    .ensure_active_skill_scope_allows_path(&state.db, &source_path)
                    .is_ok()
            {
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
