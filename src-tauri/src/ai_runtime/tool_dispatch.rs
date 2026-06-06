//! Central tool execution for the harness agent loop.

/// Tools with real `dispatch_tool_inner` handlers (single source of truth with LLM exposure).
pub const DISPATCHABLE_TOOL_NAMES: &[&str] = &[
    "search_hybrid",
    "search_semantic",
    "search_keyword",
    "get_regulation",
    "get_context_packets",
    "web_search",
    "fetch_web_page",
    "read_note",
    "list_vault",
    "get_outline",
    "get_backlinks",
    "get_block_links",
];

/// Handled inside harness loop, not via `dispatch_tool`.
pub const HARNESS_ONLY_TOOL_NAMES: &[&str] = &["spawn_subagent", "conclude_reasoning"];

/// Whether the tool may be exposed to the model (has handler or harness branch).
pub fn is_exposable_tool(name: &str) -> bool {
    crate::ai_runtime::tool_catalog::catalog_find(name).is_some_and(|entry| {
        entry.implementation != crate::ai_runtime::tool_catalog::ToolImplementationStatus::Planned
    })
}

use std::path::Path;
use std::time::Instant;

use crate::ai_runtime::retrieval_broker::{RetrievalLayers, RetrievalRequest};
use crate::ai_runtime::retrieval_scope::RetrievalScope;
use crate::ai_runtime::{
    AiScene, ContextPacket, PatchApplyResult, PatchProposal, RiskLevel, SourceSpan, ToolCallResult,
};
use crate::app::AppState;
use crate::error::{AppError, AppResult};
use crate::storage::paths::{
    is_user_note_path, resolve_vault_path, validate_user_note_relative_path,
};

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
    (tool_name == "web_search" || tool_name == "fetch_web_page")
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
        "fetch_web_page" => fetch_web_page_tool(state, args, ctx).await,
        "read_note" => read_note(state, args).await,
        "list_vault" => list_vault(state, args).await,
        "get_outline" => get_outline(state, args).await,
        "get_backlinks" => get_backlinks(state, args).await,
        "get_block_links" => get_block_links(state, args).await,
        "insert_text_at_cursor" | "replace_selection" => {
            markdown_write_patch_apply(state, ctx, tool_name, args)
        }
        _ => Err(AppError::msg(format!("unknown tool: {tool_name}"))),
    }
}

fn markdown_write_patch_apply(
    state: &AppState,
    ctx: &ToolDispatchContext<'_>,
    tool_name: &str,
    args: &serde_json::Value,
) -> AppResult<serde_json::Value> {
    let Some(target_path) = args
        .get("target_path")
        .and_then(|v| v.as_str())
        .or(ctx.note_path)
        .map(str::to_string)
    else {
        return Ok(markdown_write_not_applied(
            tool_name,
            "missing target_path",
            args,
        ));
    };
    if !is_user_note_path(&target_path) {
        return Ok(markdown_write_not_applied(
            tool_name,
            "只能修改用户笔记",
            args,
        ));
    }
    let Some(base_content_hash) = args
        .get("base_content_hash")
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
    else {
        return Ok(markdown_write_not_applied(
            tool_name,
            "missing base_content_hash",
            args,
        ));
    };
    let Some(range) = parse_source_span(args.get("range")) else {
        return Ok(markdown_write_not_applied(tool_name, "missing range", args));
    };
    let replacement_key = if tool_name == "insert_text_at_cursor" {
        "text"
    } else {
        "replacement"
    };
    let replacement = args[replacement_key]
        .as_str()
        .ok_or_else(|| AppError::msg(format!("missing {replacement_key}")))?;
    let original_text = args
        .get("original_text")
        .and_then(|v| v.as_str())
        .or_else(|| args.get("selection").and_then(|v| v.as_str()))
        .unwrap_or("");
    let patch = PatchProposal {
        id: uuid::Uuid::new_v4().to_string(),
        target_path: target_path.clone(),
        base_content_hash: base_content_hash.to_string(),
        range,
        original_text: original_text.to_string(),
        replacement_text: replacement.to_string(),
        evidence_packet_ids: vec![],
        risk_level: parse_risk_level(args.get("risk_level")),
        warnings: vec![],
        created_at: chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S").to_string(),
    };
    let vault = state.vault_path()?;
    let abs = resolve_vault_path(&vault, &target_path)?;
    let current = std::fs::read_to_string(&abs)?;
    let applied = match crate::ai_runtime::writing_workflow::apply_patch(&patch, &current) {
        Ok(content) => content,
        Err(e) => {
            let result = PatchApplyResult {
                success: false,
                new_content_hash: None,
                error: Some(e.to_string()),
                warnings: vec![],
            };
            return Ok(serde_json::json!({
                "type": "patch_apply",
                "tool_name": tool_name,
                "target_path": target_path,
                "patch_id": patch.id,
                "result": result,
            }));
        }
    };
    let tmp = abs.with_extension("md.tmp");
    std::fs::write(&tmp, &applied)?;
    if let Err(e) = std::fs::rename(&tmp, &abs) {
        let _ = crate::security::secure_delete::secure_delete(&tmp);
        return Err(e.into());
    }
    let hash = crate::ai_runtime::writing_workflow::compute_content_hash(&applied);
    state.write_guard.mark(&target_path, &hash);
    let entry = state.db.with_conn(|conn| {
        crate::indexer::scan::index_file_from_content(conn, &vault, &abs, &applied, &hash, None)
    })?;
    let result = PatchApplyResult {
        success: true,
        new_content_hash: Some(hash),
        error: None,
        warnings: vec![format!(
            "已写入「{}」，共 {} 字",
            entry.title, entry.word_count
        )],
    };
    Ok(serde_json::json!({
        "type": "patch_apply",
        "tool_name": tool_name,
        "target_path": target_path,
        "patch_id": patch.id,
        "result": result,
    }))
}

fn markdown_write_not_applied(
    tool_name: &str,
    reason: &str,
    args: &serde_json::Value,
) -> serde_json::Value {
    let replacement_key = if tool_name == "insert_text_at_cursor" {
        "text"
    } else {
        "replacement"
    };
    let replacement_len = args
        .get(replacement_key)
        .and_then(|v| v.as_str())
        .map(|s| s.chars().count())
        .unwrap_or(0);
    serde_json::json!({
        "type": "patch_apply",
        "tool_name": tool_name,
        "replacement_len": replacement_len,
        "result": PatchApplyResult {
            success: false,
            new_content_hash: None,
            error: Some(reason.to_string()),
            warnings: vec![],
        },
    })
}

fn parse_source_span(value: Option<&serde_json::Value>) -> Option<SourceSpan> {
    let value = value?;
    Some(SourceSpan {
        start: value.get("start")?.as_u64()? as usize,
        end: value.get("end")?.as_u64()? as usize,
    })
}

fn parse_risk_level(value: Option<&serde_json::Value>) -> RiskLevel {
    match value.and_then(|v| v.as_str()) {
        Some("high") => RiskLevel::High,
        Some("medium") => RiskLevel::Medium,
        _ => RiskLevel::Low,
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
    let packets = state.db.with_read_conn(|conn| {
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
    let packets = state.db.with_read_conn(|conn| {
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

async fn fetch_web_page_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    if !ctx.web_search_enabled {
        return Err(AppError::msg("web fetch not enabled for this request"));
    }
    let url = args["url"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing url"))?;
    let max_chars = args["max_chars"]
        .as_u64()
        .map(|n| n as usize)
        .unwrap_or(crate::llm::fetch_web_page::DEFAULT_MAX_CHARS);
    let page = crate::llm::fetch_web_page::fetch_web_page(&state.db, url, max_chars).await?;
    let packets = crate::ai_runtime::evidence_mixer::web_packets_from_page_fetch(&page);
    Ok(serde_json::json!({
        "url": page.url,
        "title": page.title,
        "truncated": page.truncated,
        "from_cache": page.from_cache,
        "char_count": page.text.chars().count(),
        "results": packets,
        "count": packets.len(),
    }))
}

async fn read_note(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
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

async fn list_vault(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let prefix = args["prefix"].as_str().unwrap_or("");
    let limit = args["limit"].as_u64().unwrap_or(50) as usize;
    let items = state.db.with_read_conn(|conn| {
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

async fn get_backlinks(state: &AppState, args: &serde_json::Value) -> AppResult<serde_json::Value> {
    let path = args["path"]
        .as_str()
        .ok_or_else(|| AppError::msg("missing path"))?;
    let vault = state.vault_path()?;
    let _abs = crate::storage::paths::validate_user_note_relative_path(&vault, path)?;
    let entries = state.db.with_read_conn(|conn| {
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
    let vault: &Path = &state.vault_path()?;
    let _abs = crate::storage::paths::validate_user_note_relative_path(vault, note_path)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::AppState;
    use std::sync::Arc;

    fn test_state() -> (Arc<AppState>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let vault = dir.path().join("vault");
        std::fs::create_dir_all(&vault).unwrap();
        let notes = vault.join("notes");
        std::fs::create_dir_all(&notes).unwrap();
        std::fs::write(notes.join("test.md"), "# Test\nHello world").unwrap();
        let state = AppState::new(dir.path().to_path_buf()).unwrap();
        state.set_vault(vault).unwrap();
        (state, dir)
    }

    #[tokio::test]
    async fn read_note_rejects_parent_dir_traversal() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": "../../etc/passwd" });
        let result = read_note(&state, &args).await;
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("traversal") || err.contains("元数据"),
            "unexpected error: {err}"
        );
    }

    #[tokio::test]
    async fn read_note_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": ".iris/versions/1/test.md" });
        let result = read_note(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn read_note_accepts_valid_path() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": "notes/test.md" });
        let result = read_note(&state, &args).await;
        assert!(result.is_ok());
        let val = result.unwrap();
        assert_eq!(val["path"], "notes/test.md");
        assert_eq!(val["content"], "# Test\nHello world");
    }

    #[tokio::test]
    async fn get_outline_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": ".iris/skills/my-skill/SKILL.md" });
        let result = get_outline(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn get_backlinks_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": ".iris/versions/x.md" });
        let result = get_backlinks(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn get_backlinks_rejects_parent_dir() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": "../secret.md" });
        let result = get_backlinks(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[tokio::test]
    async fn get_block_links_rejects_parent_dir() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "note_path": "../note.md" });
        let result = get_block_links(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("traversal"));
    }

    #[tokio::test]
    async fn get_block_links_rejects_iris_metadata() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "note_path": ".iris/versions/x.md" });
        let result = get_block_links(&state, &args).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("元数据"));
    }

    #[tokio::test]
    async fn read_note_rejects_absolute_path() {
        let (state, _dir) = test_state();
        let args = serde_json::json!({ "path": "/etc/passwd" });
        let result = read_note(&state, &args).await;
        assert!(result.is_err());
    }

    #[test]
    fn write_tool_approval_applies_patch_with_cas() {
        let (state, _dir) = test_state();
        let base = "# Test\nHello world";
        let base_hash = crate::ai_runtime::writing_workflow::compute_content_hash(base);
        let ctx = ToolDispatchContext {
            scene: AiScene::DraftingAssist,
            note_path: Some("notes/test.md"),
            file_id: None,
            web_search_enabled: false,
            cold_start_packets: &[],
        };
        let result = markdown_write_patch_apply(
            &state,
            &ctx,
            "replace_selection",
            &serde_json::json!({
                "replacement": "Hi",
                "base_content_hash": base_hash,
                "range": {"start": 7, "end": 12},
                "original_text": "Hello",
                "risk_level": "low"
            }),
        )
        .unwrap();

        assert_eq!(result["type"], "patch_apply");
        assert_eq!(result["result"]["success"], true);
        let content =
            std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
        assert_eq!(content, "# Test\nHi world");
    }

    #[test]
    fn write_tool_approval_reports_hash_conflict_without_writing() {
        let (state, _dir) = test_state();
        let ctx = ToolDispatchContext {
            scene: AiScene::DraftingAssist,
            note_path: Some("notes/test.md"),
            file_id: None,
            web_search_enabled: false,
            cold_start_packets: &[],
        };
        let result = markdown_write_patch_apply(
            &state,
            &ctx,
            "replace_selection",
            &serde_json::json!({
                "replacement": "Hi",
                "base_content_hash": "stale",
                "range": {"start": 7, "end": 12},
                "original_text": "Hello",
            }),
        )
        .unwrap();

        assert_eq!(result["type"], "patch_apply");
        assert_eq!(result["result"]["success"], false);
        let error = result["result"]["error"].as_str().unwrap_or("");
        assert!(
            error.contains("hash") || error.contains("哈希"),
            "unexpected error: {error}"
        );
        let content =
            std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
        assert_eq!(content, "# Test\nHello world");
    }
}
