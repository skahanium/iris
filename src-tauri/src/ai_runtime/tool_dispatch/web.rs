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
            let failure_hint = if reason.starts_with("mcp_search_parse_empty") {
                "联网搜索链路返回不可解析结果"
            } else {
                "联网搜索失败"
            };
            let instruction = if reason.starts_with("mcp_search_parse_empty") {
                "这是 MCP 返回结构/解析链路问题，不代表外部没有相关信息，也不要改写成搜索服务暂时不可用。"
            } else {
                "请告知用户搜索未成功，不要基于记忆或猜测给出结论。"
            };
            return Err(AppError::msg(format!(
                "{failure_hint}：{reason}。没有可核验的网页证据；{instruction}"
            )));
        }
    }

    Ok(serde_json::json!({
        "broker": "网络证据代理",
        "results": packets,
        "count": packets.len(),
        "rawEvidenceCount": evidence.len(),
        "resultBudget": {
            "format": "context_packets_only",
            "rawEvidenceOmitted": true,
        },
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

    fn failed_provider_output(reason: &str) -> WebEvidenceBrokerOutput {
        WebEvidenceBrokerOutput {
            items: vec![WebEvidenceItem {
                url: String::new(),
                canonical_url: String::new(),
                title: "网络证据代理".into(),
                domain: String::new(),
                snippet: String::new(),
                fetched_excerpt: None,
                provider_id: "web.provider".into(),
                provider_kind: "mcp".into(),
                cost_class: "free".into(),
                raw_result_hash: "hash".into(),
                extraction_method: "none".into(),
                trust_level: "external_untrusted".into(),
                retrieval_reason: "search".into(),
                search_backend: WebSearchBackend::Provider,
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

    fn successful_provider_item(index: usize, fetched_excerpt: String) -> WebEvidenceItem {
        WebEvidenceItem {
            url: format!("https://example.com/{index}"),
            canonical_url: format!("https://example.com/{index}"),
            title: format!("来源 {index}"),
            domain: "example.com".into(),
            snippet: "摘要".into(),
            fetched_excerpt: Some(fetched_excerpt),
            provider_id: "web.provider".into(),
            provider_kind: "mcp".into(),
            cost_class: "free".into(),
            raw_result_hash: format!("hash-{index}"),
            extraction_method: "search_result".into(),
            trust_level: "external_untrusted".into(),
            retrieval_reason: "search".into(),
            search_backend: WebSearchBackend::Provider,
            source_rank: WebSourceRank::Unknown,
            freshness_label: None,
            failure_reason: None,
            conflict_group: None,
            conflict_note: None,
        }
    }

    #[test]
    fn failed_only_web_evidence_is_returned_as_tool_error() {
        let err = web_search_tool_response(
            "武亮 结婚",
            failed_provider_output("web_search_failed: HTTP error"),
        )
        .unwrap_err();

        assert!(err.to_string().contains("联网搜索失败"));
        assert!(err.to_string().contains("web_search_failed"));
    }

    #[test]
    fn parse_empty_web_evidence_is_reported_as_unparseable_search_chain() {
        let err = web_search_tool_response(
            "高市早苗 最近 动向",
            failed_provider_output("mcp_search_parse_empty:text_without_url"),
        )
        .unwrap_err();

        assert!(err.to_string().contains("联网搜索链路返回不可解析结果"));
        assert!(err
            .to_string()
            .contains("mcp_search_parse_empty:text_without_url"));
    }

    #[test]
    fn successful_web_response_is_packetized_without_raw_evidence_blob() {
        let output = WebEvidenceBrokerOutput {
            items: (0..8)
                .map(|index| successful_provider_item(index, "长正文".repeat(4_000)))
                .collect(),
            usage: WebEvidenceUsage {
                successful_search_requests: WebEvidenceSearchRequestUsage { mcp: 1 },
                providers: Vec::new(),
            },
        };

        let response = web_search_tool_response("近期世界杯战况", output).unwrap();
        let encoded = serde_json::to_string(&response).unwrap();

        assert!(response.get("evidence").is_none());
        assert_eq!(response["count"], serde_json::json!(8));
        assert_eq!(response["results"].as_array().unwrap().len(), 8);
        assert!(encoded.chars().count() < 50_000);
    }
}
