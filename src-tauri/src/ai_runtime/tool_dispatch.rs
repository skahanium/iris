//! Central tool execution for the harness agent loop.

use std::path::Path;
use std::time::Instant;

use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::{AiScene, ContextPacket, ToolCallResult};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::resolve_vault_path;

/// Context passed into tool dispatch.
pub struct ToolDispatchContext<'a> {
    pub scene: AiScene,
    pub note_path: Option<&'a str>,
    pub file_id: Option<i64>,
    pub web_search_enabled: bool,
    pub cold_start_packets: &'a [ContextPacket],
}

fn is_retryable_tool_error(tool_name: &str, result: &ToolCallResult) -> bool {
    if result.success {
        return false;
    }
    let err = result.error.as_deref().unwrap_or("");
    tool_name == "web_search"
        && (err.contains("timeout") || err.contains("network") || err.contains("连接"))
}

/// Execute with one retry for transient failures; hybrid search falls back to keyword.
pub async fn dispatch_tool_with_retry(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolCallResult {
    let mut result = dispatch_tool(state, ctx, tool_name, args).await;
    if is_retryable_tool_error(tool_name, &result) {
        tokio::time::sleep(std::time::Duration::from_millis(400)).await;
        result = dispatch_tool(state, ctx, tool_name, args).await;
    }
    if !result.success && tool_name == "search_hybrid" {
        return dispatch_tool(state, ctx, "search_keyword", args).await;
    }
    result
}

/// Execute a tool by name and return JSON output for the LLM tool message.
pub async fn dispatch_tool(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> ToolCallResult {
    let start = Instant::now();
    let result = dispatch_tool_inner(state, ctx, tool_name, args).await;
    let duration_ms = start.elapsed().as_millis() as u64;
    match result {
        Ok(output) => ToolCallResult {
            tool_name: tool_name.to_string(),
            success: true,
            output,
            duration_ms,
            tokens_used: None,
            error: None,
        },
        Err(e) => ToolCallResult {
            tool_name: tool_name.to_string(),
            success: false,
            output: serde_json::Value::Null,
            duration_ms,
            tokens_used: None,
            error: Some(e.to_string()),
        },
    }
}

async fn dispatch_tool_inner(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    match tool_name {
        "search_hybrid" | "search_semantic" | "search_keyword" => {
            hybrid_search(state, tool_name, args, ctx).await
        }
        "get_regulation" => regulation_lookup(state, args).await,
        "get_context_packets" => Ok(serde_json::json!({
            "packets": ctx.cold_start_packets,
            "count": ctx.cold_start_packets.len(),
        })),
        "web_search" => web_search_tool(state, args, ctx).await,
        "read_note" => read_note(state, args).await,
        "list_vault" => list_vault(state, args).await,
        "get_outline" => get_outline(state, args).await,
        "get_backlinks" => get_backlinks(state, args).await,
        "get_block_links" => get_block_links(state, args).await,
        "get_genre_template" | "get_model_essays" => Err(AppError::msg(format!(
            "tool {tool_name} not yet wired in harness dispatch"
        ))),
        _ => Err(AppError::msg(format!("unknown tool: {tool_name}"))),
    }
}

async fn hybrid_search(
    state: &AppState,
    tool_name: &str,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    let query = args["query"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing query"))?;
    let limit = args["limit"].as_u64().unwrap_or(10) as usize;
    let layers = match tool_name {
        "search_keyword" => RetrievalLayers {
            fts: true,
            vector: false,
            graph: false,
            exact: false,
            template: false,
        },
        "search_semantic" => RetrievalLayers {
            fts: false,
            vector: true,
            graph: false,
            exact: false,
            template: false,
        },
        _ => RetrievalLayers {
            fts: true,
            vector: true,
            graph: ctx.note_path.is_some(),
            exact: false,
            template: false,
        },
    };
    let packets = state.db.with_conn(|conn| {
        let request = RetrievalRequest {
            query: query.to_string(),
            max_results: limit,
            layers,
            note_context: ctx.note_path.map(|s| s.to_string()),
            file_id_context: ctx.file_id,
            scope: RetrievalScope::default(),
        };
        crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
    })?;
    Ok(serde_json::json!({ "results": packets, "count": packets.len() }))
}

async fn regulation_lookup(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let regulation_name = args["regulation_name"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing regulation_name"))?;
    let article = args["article"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing article"))?;
    let query = format!("《{regulation_name}》{article}");
    let packets = state.db.with_conn(|conn| {
        let request = RetrievalRequest {
            query,
            max_results: 3,
            layers: RetrievalLayers {
                fts: false,
                vector: false,
                graph: false,
                exact: true,
                template: false,
            },
            note_context: None,
            file_id_context: None,
            scope: RetrievalScope::default(),
        };
        crate::ai_runtime::retrieval_broker::hybrid_retrieve(conn, &request)
    })?;
    Ok(serde_json::json!({
        "regulation": packets.first(),
        "found": !packets.is_empty(),
    }))
}

async fn web_search_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    if !ctx.web_search_enabled {
        return Err(AppError::msg("web search not enabled for this request"));
    }
    let query = args["query"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing query"))?;
    let result = crate::llm::search_web::fetch_search_context_for_db(&state.db, query).await?;
    let packets = crate::ai_runtime::evidence_mixer::web_packets_from_fetch(&result, query, None);
    Ok(serde_json::json!({
        "context": result.body,
        "backend": format!("{:?}", result.backend),
        "results": packets,
        "count": packets.len(),
    }))
}

async fn read_note(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
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

async fn list_vault(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let prefix = args["prefix"].as_str().unwrap_or("");
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;
    let items = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT path, title FROM files
             WHERE id IN (SELECT MAX(id) FROM files GROUP BY path)
               AND path NOT LIKE '.iris/%'
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

async fn get_outline(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, path)?;
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

async fn get_backlinks(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let entries = state.db.with_conn(|conn| {
        let mut stmt = conn.prepare(
            "SELECT f.path, f.title, l.context
             FROM links l
             JOIN files f ON f.id = l.source_id
             JOIN files t ON t.id = l.target_id
             WHERE t.path = ?1
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

async fn get_block_links(
    state: &AppState,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let note_path = args["note_path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing note_path"))?;
    let _vault: &Path = &state.vault_path()?;
    let links = state.db.with_conn(|conn| {
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
