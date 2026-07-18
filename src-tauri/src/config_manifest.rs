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
}
