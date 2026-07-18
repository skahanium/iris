//! Shared compile-time manifests under `/config`.

use std::sync::OnceLock;

use serde::Deserialize;

use crate::error::{AppError, AppResult};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct BuiltinLlmProviderRow {
    id: String,
    name: String,
    default_model: String,
}

fn builtin_llm_rows() -> &'static [(String, String, String)] {
    static ROWS: OnceLock<Vec<(String, String, String)>> = OnceLock::new();
    ROWS.get_or_init(|| {
        let rows: Vec<BuiltinLlmProviderRow> =
            serde_json::from_str(include_str!("../../config/llm-builtin-providers.json"))
                .expect("config/llm-builtin-providers.json must parse");
        rows.into_iter()
            .map(|row| (row.id, row.name, row.default_model))
            .collect()
    })
    .as_slice()
}

/// Builtin LLM providers from `config/llm-builtin-providers.json`.
pub(crate) fn builtin_llm_providers() -> AppResult<&'static [(String, String, String)]> {
    let rows = builtin_llm_rows();
    if rows.is_empty() {
        return Err(AppError::msg("llm-builtin-providers.json is empty"));
    }
    Ok(rows)
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct McpSearchResultLimitHealTarget {
    pub preset_id: String,
    pub hosts: Vec<String>,
    pub provider_name: String,
    pub max_results_arg: String,
}

fn mcp_search_result_limit_targets() -> &'static [McpSearchResultLimitHealTarget] {
    static TARGETS: OnceLock<Vec<McpSearchResultLimitHealTarget>> = OnceLock::new();
    TARGETS
        .get_or_init(|| {
            serde_json::from_str(include_str!(
                "../../config/mcp-search-result-limit-manifest.json"
            ))
            .expect("config/mcp-search-result-limit-manifest.json must parse")
        })
        .as_slice()
}

fn transport_preset_id(transport_config_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(transport_config_json)
        .ok()
        .and_then(|value| {
            value
                .get("preset_id")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_owned)
        })
}

fn transport_url_host(transport_config_json: &str) -> Option<String> {
    serde_json::from_str::<serde_json::Value>(transport_config_json)
        .ok()
        .and_then(|value| {
            value
                .get("url")
                .and_then(serde_json::Value::as_str)
                .map(str::to_owned)
        })
        .and_then(|url| reqwest::Url::parse(&url).ok())
        .and_then(|url| url.host_str().map(str::to_owned))
}

/// Resolve the persisted `maxResultsArg` tool parameter for a provider.
pub(crate) fn resolve_mcp_search_result_limit_arg(
    transport_config_json: &str,
    provider_name: &str,
) -> Option<String> {
    let targets = mcp_search_result_limit_targets();
    if let Some(preset_id) = transport_preset_id(transport_config_json) {
        if let Some(target) = targets.iter().find(|item| item.preset_id == preset_id) {
            return Some(target.max_results_arg.clone());
        }
    }
    if let Some(host) = transport_url_host(transport_config_json) {
        if let Some(target) = targets.iter().find(|item| {
            item.hosts
                .iter()
                .any(|item_host| item_host.eq_ignore_ascii_case(&host))
        }) {
            return Some(target.max_results_arg.clone());
        }
    }
    let normalized_name = provider_name.trim().to_ascii_lowercase();
    targets
        .iter()
        .find(|item| {
            item.provider_name
                .trim()
                .eq_ignore_ascii_case(&normalized_name)
        })
        .map(|item| item.max_results_arg.clone())
}

/// Return whether a credential service is listed as optional in the shared manifest.
pub(crate) fn is_mcp_optional_credential_service(service: &str) -> bool {
    static SERVICES: OnceLock<Vec<String>> = OnceLock::new();
    let services = SERVICES.get_or_init(|| {
        serde_json::from_str::<Vec<String>>(include_str!(
            "../../config/mcp-optional-credential-services.json"
        ))
        .expect("config/mcp-optional-credential-services.json must parse")
    });
    services.iter().any(|item| item == service)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_llm_manifest_includes_deepseek_and_mimo() {
        let providers = builtin_llm_providers().expect("manifest");
        let ids: Vec<_> = providers.iter().map(|(id, _, _)| id.as_str()).collect();
        assert!(ids.contains(&"deepseek"));
        assert!(ids.contains(&"mimo"));
        assert!(!ids.contains(&"custom"));
    }

    #[test]
    fn mcp_optional_manifest_lists_anysearch_and_jina() {
        assert!(is_mcp_optional_credential_service("iris.mcp.anysearch"));
        assert!(is_mcp_optional_credential_service("iris.mcp.jina"));
        assert!(!is_mcp_optional_credential_service("iris.mcp.brave"));
    }

    #[test]
    fn mcp_search_result_limit_manifest_lists_anysearch_and_firecrawl() {
        assert_eq!(
            resolve_mcp_search_result_limit_arg(
                r#"{"preset_id":"anysearch","url":"https://api.anysearch.com/mcp"}"#,
                "AnySearch",
            )
            .as_deref(),
            Some("max_results")
        );
        assert_eq!(
            resolve_mcp_search_result_limit_arg(
                r#"{"preset_id":"firecrawl","url":"https://mcp.firecrawl.dev/v2/mcp"}"#,
                "Firecrawl",
            )
            .as_deref(),
            Some("limit")
        );
    }
}
