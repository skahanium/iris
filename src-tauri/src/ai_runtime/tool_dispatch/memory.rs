use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

const MAX_MEMORY_KEY_CHARS: usize = 200;
const MAX_MEMORY_CONTENT_CHARS: usize = 2_000;
const MAX_MEMORY_READ_LIMIT: i64 = 50;

/// Delete all memories under one exact scope. This is the primitive used for
/// scope cleanup; it never touches other scopes or the global scope.
#[cfg(test)]
pub(crate) fn clear_memory_scope(
    db: &crate::storage::db::Database,
    scope: &str,
) -> AppResult<usize> {
    db.with_conn(|conn| Ok(conn.execute("DELETE FROM ai_memories WHERE scope = ?1", [scope])?))
}

fn memory_session_scope(ctx: &ToolDispatchContext<'_>) -> String {
    match ctx.note_path {
        Some(path) if !path.is_empty() => format!("run:{path}"),
        _ => "run:__global__".to_string(),
    }
}

pub(super) async fn memory_read_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = (args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as i64)
        .clamp(1, MAX_MEMORY_READ_LIMIT);
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
    if key.chars().count() > MAX_MEMORY_KEY_CHARS
        || content.chars().count() > MAX_MEMORY_CONTENT_CHARS
    {
        return Err(AppError::msg("memory_write_exceeds_budget"));
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
             ON CONFLICT(scope, key) DO UPDATE SET
               content = excluded.content,
               scope = excluded.scope,
               updated_at = datetime('now')",
            rusqlite::params![key, content, scope],
        )?;
        Ok(())
    })?;
    Ok(serde_json::json!({ "ok": true, "key": key }))
}

#[cfg(test)]
mod tests {
    use super::clear_memory_scope;
    use crate::storage::db::Database;

    #[test]
    fn same_key_can_exist_in_different_scopes_and_clear_preserves_other_scope() {
        let db = Database::open_in_memory().expect("database");
        db.with_conn(|conn| {
            Ok(conn.execute_batch(
                "INSERT INTO ai_memories (key, content, scope)
                 VALUES ('shared-key', 'global', 'global'),
                        ('shared-key', 'vault', 'run:note')",
            )?)
        })
        .expect("seed memories");

        let cleared = clear_memory_scope(&db, "run:note").expect("clear one scope");
        assert_eq!(cleared, 1);

        let global_count: i64 = db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT COUNT(*) FROM ai_memories WHERE scope = 'global'",
                    [],
                    |row| row.get(0),
                )?)
            })
            .expect("global scope count");
        assert_eq!(
            global_count, 1,
            "clearing one scope must preserve the global scope"
        );
    }
}
