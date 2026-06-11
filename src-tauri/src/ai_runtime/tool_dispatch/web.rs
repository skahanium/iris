use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

pub(super) async fn web_search_tool(
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

pub(super) async fn fetch_web_page_tool(
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

pub(super) async fn readability_fetch_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
    rendered: bool,
) -> AppResult<serde_json::Value> {
    let mut out = fetch_web_page_tool(state, args, ctx).await?;
    if let Some(obj) = out.as_object_mut() {
        obj.insert("rendered".into(), serde_json::json!(false));
        if rendered {
            obj.insert(
                "warning".into(),
                serde_json::json!(
                    "rendered_fetch currently uses the safe HTTPS text extraction path; JavaScript rendering is not enabled"
                ),
            );
        }
    }
    Ok(out)
}

pub(super) async fn web_fetch_batch_tool(
    state: &AppState,
    args: &serde_json::Value,
    ctx: &ToolDispatchContext<'_>,
) -> AppResult<serde_json::Value> {
    if !ctx.web_search_enabled {
        return Err(AppError::msg("web fetch not enabled for this request"));
    }
    let urls = args["urls"]
        .as_array()
        .ok_or_else(|| AppError::msg("missing urls"))?;
    let max_chars = args["max_chars"].as_u64().unwrap_or(12_000) as usize;
    let mut pages = Vec::new();
    let mut all_packets = Vec::new();
    for url in urls.iter().filter_map(|v| v.as_str()).take(5) {
        let page = crate::llm::fetch_web_page::fetch_web_page(&state.db, url, max_chars).await?;
        let packets = crate::ai_runtime::evidence_mixer::web_packets_from_page_fetch(&page);
        all_packets.extend(packets);
        pages.push(serde_json::json!({
            "url": page.url,
            "title": page.title,
            "truncated": page.truncated,
            "from_cache": page.from_cache,
            "char_count": page.text.chars().count(),
        }));
    }
    Ok(serde_json::json!({
        "pages": pages,
        "results": all_packets,
        "count": all_packets.len(),
    }))
}
