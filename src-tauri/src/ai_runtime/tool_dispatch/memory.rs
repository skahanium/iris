use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

fn memory_session_scope(ctx: &ToolDispatchContext<'_>) -> String {
    let scene = ctx.scene.profile();
    match ctx.note_path {
        Some(path) if !path.is_empty() => format!("{scene}:{path}"),
        _ => format!("{scene}:__global__"),
    }
}

pub(super) async fn memory_read_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as i64;
    let session_scope = memory_session_scope(ctx);
    let items = state.db.with_read_conn(|conn| {
        if !key.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT key, content, scope, source, updated_at FROM ai_memories
                 WHERE key = ?1 AND (scope = 'global' OR scope = ?2)
                 LIMIT 1",
            )?;
            let rows = stmt.query_map(rusqlite::params![key, session_scope], |row| {
                Ok(serde_json::json!({
                    "key": row.get::<_, String>(0)?,
                    "content": row.get::<_, String>(1)?,
                    "scope": row.get::<_, String>(2)?,
                    "source": row.get::<_, String>(3)?,
                    "updated_at": row.get::<_, String>(4)?,
                }))
            })?;
            return Ok(rows.flatten().collect::<Vec<_>>());
        }
        let like = format!("%{query}%");
        let mut stmt = conn.prepare(
            "SELECT key, content, scope, source, updated_at
             FROM ai_memories
             WHERE (scope = 'global' OR scope = ?4)
               AND (?1 = '' OR key LIKE ?2 OR content LIKE ?2)
             ORDER BY updated_at DESC
             LIMIT ?3",
        )?;
        let rows = stmt.query_map(
            rusqlite::params![query, like, limit, session_scope],
            |row| {
                Ok(serde_json::json!({
                    "key": row.get::<_, String>(0)?,
                    "content": row.get::<_, String>(1)?,
                    "scope": row.get::<_, String>(2)?,
                    "source": row.get::<_, String>(3)?,
                    "updated_at": row.get::<_, String>(4)?,
                }))
            },
        )?;
        Ok(rows.flatten().collect::<Vec<_>>())
    })?;
    Ok(serde_json::json!({ "items": items, "count": items.len() }))
}

pub(super) async fn memory_write_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let key = args["key"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing key"))?
        .trim();
    let content = args["content"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing content"))?
        .trim();
    if key.is_empty() || content.is_empty() {
        return Err(AppError::msg("memory_write requires non-empty key/content"));
    }
    let explicit_scope = args.get("scope").and_then(|v| v.as_str()).unwrap_or("");
    let scope = if explicit_scope == "global" {
        "global".to_string()
    } else {
        memory_session_scope(ctx)
    };
    state.db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO ai_memories (key, content, scope, source, created_at, updated_at)
             VALUES (?1, ?2, ?3, 'user_confirmed', datetime('now'), datetime('now'))
             ON CONFLICT(key) DO UPDATE SET
               content = excluded.content,
               scope = excluded.scope,
               updated_at = datetime('now')",
            rusqlite::params![key, content, scope],
        )?;
        Ok(())
    })?;
    Ok(serde_json::json!({ "ok": true, "key": key }))
}
