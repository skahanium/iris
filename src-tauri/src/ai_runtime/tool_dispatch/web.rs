use crate::app::AppState;
use crate::error::{AppError, AppResult};

use super::ToolDispatchContext;

fn web_search_tool_response(
    query: &str,
    output: crate::ai_runtime::web_evidence_broker::WebEvidenceBrokerOutput,
) -> AppResult<serde_json::Value> {
    let evidence = output.items;
    let packets =
        crate::ai_runtime::web_evidence_broker::web_evidence_items_to_packets(query, &evidence);
    if packets.is_empty() {
        if let Some(reason) = evidence
            .iter()
            .find_map(|item| item.failure_reason.as_deref())
        {
            return Err(AppError::msg(format!(
                "联网搜索失败：{reason}。没有可核验的网页证据；请告知用户搜索未成功，不要基于记忆或猜测给出结论。"
            )));
        }
    }

    Ok(serde_json::json!({
        "broker": "网络证据代理",
        "evidence": evidence,
        "results": packets,
        "count": packets.len(),
        "webUsage": output.usage,
    }))
}

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
    web_search_tool_response(query, output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::web_evidence_broker::{
        WebEvidenceBrokerOutput, WebEvidenceItem, WebEvidenceSearchRequestUsage, WebEvidenceUsage,
    };
    use crate::ai_runtime::{WebSearchBackend, WebSourceRank};

    fn failed_ddg_output(reason: &str) -> WebEvidenceBrokerOutput {
        WebEvidenceBrokerOutput {
            items: vec![WebEvidenceItem {
                url: String::new(),
                canonical_url: String::new(),
                title: "网络证据代理".into(),
                domain: String::new(),
                snippet: String::new(),
                fetched_excerpt: None,
                provider_id: "web.provider".into(),
                provider_kind: "native".into(),
                cost_class: "free".into(),
                raw_result_hash: "hash".into(),
                extraction_method: "none".into(),
                trust_level: "external_untrusted".into(),
                retrieval_reason: "search".into(),
                search_backend: WebSearchBackend::Duckduckgo,
                source_rank: WebSourceRank::Unknown,
                freshness_label: None,
                failure_reason: Some(reason.into()),
                conflict_group: None,
                conflict_note: None,
            }],
            usage: WebEvidenceUsage {
                successful_search_requests: WebEvidenceSearchRequestUsage::default(),
                providers: Vec::new(),
            },
        }
    }

    #[test]
    fn failed_only_web_evidence_is_returned_as_tool_error() {
        let err = web_search_tool_response(
            "武亮 结婚",
            failed_ddg_output("web_search_failed: HTTP error"),
        )
        .unwrap_err();

        assert!(err.to_string().contains("联网搜索失败"));
        assert!(err.to_string().contains("web_search_failed"));
    }
}
