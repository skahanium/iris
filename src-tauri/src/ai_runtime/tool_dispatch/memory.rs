use crate::app::AppState;
use crate::error::{AppError, AppResult};
use std::collections::HashSet;

use super::ToolDispatchContext;

const MAX_MEMORY_KEY_CHARS: usize = 200;
const MAX_MEMORY_CONTENT_CHARS: usize = 2_000;
const MAX_MEMORY_READ_LIMIT: i64 = 50;

/// Delete all memories under one exact scope. This is the primitive used for
/// scope cleanup; it never touches other scopes or the global scope.
fn clear_memory_scope(db: &crate::storage::db::Database, scope: &str) -> AppResult<usize> {
    db.with_conn(|conn| Ok(conn.execute("DELETE FROM ai_memories WHERE scope = ?1", [scope])?))
}

fn active_vault_scope(state: &AppState) -> AppResult<(String, String)> {
    let vault = state
        .vault_path()
        .map_err(|_| AppError::msg("memory_vault_scope_unavailable"))?;
    let vault_id = crate::cas::hash::content_hash_str(&vault.to_string_lossy());
    Ok((format!("vault:{vault_id}"), vault_id))
}

fn requested_scope(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<(String, Option<String>)> {
    let scope = match args.get("scope") {
        None => "global",
        Some(value) => value
            .as_str()
            .ok_or_else(|| AppError::msg("memory_scope_invalid"))?,
    };
    match scope {
        "global" => Ok(("global".to_string(), None)),
        "vault" => active_vault_scope(state).map(|(scope, vault_id)| (scope, Some(vault_id))),
        _ => Err(AppError::msg("memory_scope_invalid")),
    }
}

pub(super) async fn memory_read_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    ctx.ensure_run_active()?;
    let key = args.get("key").and_then(|v| v.as_str()).unwrap_or("");
    let query = args.get("query").and_then(|v| v.as_str()).unwrap_or("");
    let limit = (args.get("limit").and_then(|v| v.as_u64()).unwrap_or(20) as i64)
        .clamp(1, MAX_MEMORY_READ_LIMIT);
    let vault_scope = state.vault_path().ok().map(|vault| {
        let vault_id = crate::cas::hash::content_hash_str(&vault.to_string_lossy());
        format!("vault:{vault_id}")
    });
    let items = state.db.with_read_conn(|conn| {
        if !key.is_empty() {
            let mut stmt = conn.prepare(
                "SELECT key, content, scope, source, updated_at FROM ai_memories
                 WHERE key = ?1 AND (scope = 'global' OR scope = ?2)
                 ORDER BY CASE WHEN scope = ?2 THEN 0 ELSE 1 END
                 LIMIT 1",
            )?;
            let rows = stmt.query_map(rusqlite::params![key, vault_scope], |row| {
                let stored_scope = row.get::<_, String>(2)?;
                Ok(serde_json::json!({
                    "key": row.get::<_, String>(0)?,
                    "content": row.get::<_, String>(1)?,
                    "scope": if stored_scope == "global" { "global" } else { "vault" },
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
             ORDER BY CASE WHEN scope = ?4 THEN 0 ELSE 1 END, updated_at DESC",
        )?;
        let rows = stmt.query_map(rusqlite::params![query, like, limit, vault_scope], |row| {
            let stored_scope = row.get::<_, String>(2)?;
            Ok(serde_json::json!({
                "key": row.get::<_, String>(0)?,
                "content": row.get::<_, String>(1)?,
                "scope": if stored_scope == "global" { "global" } else { "vault" },
                "source": row.get::<_, String>(3)?,
                "updated_at": row.get::<_, String>(4)?,
            }))
        })?;
        let mut seen = HashSet::new();
        Ok(rows
            .flatten()
            .filter(|item| {
                item.get("key")
                    .and_then(serde_json::Value::as_str)
                    .is_some_and(|key| seen.insert(key.to_string()))
            })
            .take(limit as usize)
            .collect::<Vec<_>>())
    })?;
    Ok(serde_json::json!({ "items": items, "count": items.len() }))
}

pub(super) async fn memory_write_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    ctx.ensure_run_active()?;
    let operation = match args.get("operation") {
        None => "upsert",
        Some(value) => value
            .as_str()
            .ok_or_else(|| AppError::msg("memory_operation_invalid"))?,
    };
    let has_key = args.get("key").is_some();
    let has_content = args.get("content").is_some();
    let key = args
        .get("key")
        .and_then(serde_json::Value::as_str)
        .map(str::trim);
    let content = args
        .get("content")
        .and_then(serde_json::Value::as_str)
        .map(str::trim);
    if key.is_some_and(|value| value.chars().count() > MAX_MEMORY_KEY_CHARS)
        || content.is_some_and(|value| value.chars().count() > MAX_MEMORY_CONTENT_CHARS)
    {
        return Err(AppError::msg("memory_write_exceeds_budget"));
    }
    let (scope, vault_id) = requested_scope(state, args)?;
    let affected_count = match operation {
        "upsert" => {
            let key = key
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::msg("missing key"))?;
            let content = content
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::msg("missing content"))?;
            state.db.with_conn(|conn| {
                conn.execute(
                    "INSERT INTO ai_memories
                     (key, content, scope, source, vault_id, created_at, updated_at)
                     VALUES (?1, ?2, ?3, 'user_confirmed', ?4, datetime('now'), datetime('now'))
                     ON CONFLICT(scope, key) DO UPDATE SET
                       content = excluded.content,
                       vault_id = excluded.vault_id,
                       updated_at = datetime('now')",
                    rusqlite::params![key, content, scope, vault_id],
                )?;
                Ok(1_usize)
            })?
        }
        "delete_key" => {
            if has_content {
                return Err(AppError::msg("memory_delete_key_rejects_content"));
            }
            let key = key
                .filter(|value| !value.is_empty())
                .ok_or_else(|| AppError::msg("missing key"))?;
            state.db.with_conn(|conn| {
                Ok(conn.execute(
                    "DELETE FROM ai_memories WHERE scope = ?1 AND key = ?2",
                    rusqlite::params![scope, key],
                )?)
            })?
        }
        "clear_scope" => {
            if has_key || has_content {
                return Err(AppError::msg("memory_clear_scope_rejects_key_content"));
            }
            clear_memory_scope(&state.db, &scope)?
        }
        _ => return Err(AppError::msg("memory_operation_invalid")),
    };
    Ok(serde_json::json!({
        "ok": true,
        "operation": operation,
        "scope": if scope == "global" { "global" } else { "vault" },
        "affectedCount": affected_count,
    }))
}

#[cfg(test)]
mod tests {
    use super::{memory_read_tool, memory_write_tool};
    use crate::ai_runtime::retrieval_scope::RetrievalScope;
    use crate::ai_runtime::tool_dispatch::ToolDispatchContext;
    use crate::app::AppState;

    fn context<'a>(retrieval_scope: &'a RetrievalScope) -> ToolDispatchContext<'a> {
        ToolDispatchContext {
            note_path: None,
            file_id: None,
            run_id: None,
            write_target_path: None,
            document_policy: None,
            web_search_enabled: false,
            available_tool_names: &[],
            max_web_fetches: 0,
            cold_start_packets: &[],
            retrieval_scope,
            runtime_documents: &[],
            app_handle: None,
            attachment_count: 0,
            skill_activation_plan: None,
        }
    }

    fn test_state() -> (std::sync::Arc<AppState>, tempfile::TempDir, String) {
        let directory = tempfile::tempdir().expect("temporary directory");
        let vault = directory.path().join("vault");
        std::fs::create_dir_all(&vault).expect("vault directory");
        let state = AppState::new(directory.path().join("data")).expect("state");
        state.set_vault(vault.clone()).expect("active vault");
        let active_vault = state.vault_path().expect("resolved active vault");
        let vault_id = crate::cas::hash::content_hash_str(&active_vault.to_string_lossy());
        (state, directory, vault_id)
    }

    #[tokio::test]
    async fn memory_scope_precedence_is_vault_then_global() {
        let (state, _directory, vault_id) = test_state();
        let vault_scope = format!("vault:{vault_id}");
        state
            .db
            .with_conn(|conn| {
                Ok(conn.execute_batch(&format!(
                    "INSERT INTO ai_memories (key, content, scope, vault_id)
                     VALUES ('shared-key', 'global-value', 'global', NULL),
                            ('shared-key', 'vault-value', '{vault_scope}', '{vault_id}')"
                ))?)
            })
            .expect("seed memories");
        let retrieval_scope = RetrievalScope::default();
        let ctx = context(&retrieval_scope);

        let exact = memory_read_tool(&state, &serde_json::json!({"key": "shared-key"}), &ctx)
            .await
            .expect("exact memory read");
        assert_eq!(exact["items"][0]["content"], "vault-value");
        assert_eq!(exact["items"][0]["scope"], "vault");

        let listed = memory_read_tool(&state, &serde_json::json!({}), &ctx)
            .await
            .expect("list memory read");
        assert_eq!(listed["count"], 1);
        assert_eq!(listed["items"][0]["content"], "vault-value");
    }

    #[tokio::test]
    async fn confirmed_memory_clear_is_scope_local() {
        let (state, _directory, vault_id) = test_state();
        let vault_scope = format!("vault:{vault_id}");
        state
            .db
            .with_conn(|conn| {
                Ok(conn.execute_batch(&format!(
                    "INSERT INTO ai_memories (key, content, scope, vault_id)
                 VALUES ('global-key', 'global', 'global', NULL),
                        ('vault-key', 'vault', '{vault_scope}', '{vault_id}')"
                ))?)
            })
            .expect("seed memories");
        let retrieval_scope = RetrievalScope::default();
        let ctx = context(&retrieval_scope);

        let result = memory_write_tool(
            &state,
            &serde_json::json!({"operation": "clear_scope", "scope": "vault"}),
            &ctx,
        )
        .await
        .expect("confirmed clear dispatch");
        assert_eq!(result["affectedCount"], 1);

        let counts: (i64, i64) = state
            .db
            .with_read_conn(|conn| {
                Ok((
                    conn.query_row(
                        "SELECT COUNT(*) FROM ai_memories WHERE scope = 'global'",
                        [],
                        |row| row.get(0),
                    )?,
                    conn.query_row(
                        "SELECT COUNT(*) FROM ai_memories WHERE scope = ?1",
                        [&vault_scope],
                        |row| row.get(0),
                    )?,
                ))
            })
            .expect("scope counts");
        assert_eq!(counts, (1, 0));
    }
}
