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
    let urls = args["urls"]
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let output = crate::ai_runtime::web_evidence_broker::collect_web_evidence_with_usage(
        &state.db,
        crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerInput {
            query: query.to_string(),
            urls,
            enabled: ctx.web_search_enabled,
            max_search_results: 8,
            max_fetches: ctx.max_web_fetches,
        },
    )
    .await?;
    let evidence = output.items;
    let packets =
        crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets(query, &evidence);
    Ok(serde_json::json!({
        "broker": "网络证据代理",
        "evidence": evidence,
        "results": packets,
        "count": packets.len(),
        "webUsage": output.usage,
    }))
}
