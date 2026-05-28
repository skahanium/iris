use serde::Serialize;

use crate::llm::config::{LlmRoutingConfig, ProviderOverride};

#[derive(Debug, Clone, Serialize)]
pub struct LlmProviderInfo {
    pub id: String,
    pub name: String,
    pub default_model: String,
}

/// 设置页允许的厂商：DeepSeek + 任意自定义 OpenAI 兼容端点。
pub fn is_custom_provider(provider_id: &str) -> bool {
    provider_id == "custom" || provider_id.starts_with("custom_")
}

pub fn is_allowed_provider(provider_id: &str) -> bool {
    provider_id == "deepseek" || is_custom_provider(provider_id)
}

pub fn requires_api_key(provider_id: &str) -> bool {
    is_allowed_provider(provider_id)
}

/// 仅 DeepSeek（兼容旧调用；设置页请用 [`list_providers_from_routing`]）。
pub fn list_providers() -> Vec<LlmProviderInfo> {
    vec![deepseek_info()]
}

pub fn list_providers_from_routing(routing: &LlmRoutingConfig) -> Vec<LlmProviderInfo> {
    let mut out = vec![deepseek_info()];
    let mut custom_ids: Vec<String> = routing
        .providers
        .keys()
        .filter(|id| is_custom_provider(id))
        .cloned()
        .collect();
    custom_ids.sort();
    for id in custom_ids {
        let row = routing.providers.get(&id).cloned().unwrap_or_default();
        out.push(provider_info_from_override(&id, &row));
    }
    out
}

fn deepseek_info() -> LlmProviderInfo {
    LlmProviderInfo {
        id: "deepseek".into(),
        name: "DeepSeek".into(),
        default_model: "deepseek-v4-flash".into(),
    }
}

fn provider_info_from_override(id: &str, row: &ProviderOverride) -> LlmProviderInfo {
    LlmProviderInfo {
        id: id.to_string(),
        name: row
            .label
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| default_custom_label(id)),
        default_model: row
            .default_model
            .clone()
            .filter(|s| !s.trim().is_empty())
            .unwrap_or_else(|| "default".into()),
    }
}

fn default_custom_label(id: &str) -> String {
    if id == "custom" {
        "Custom OpenAI-compatible".into()
    } else {
        format!("Custom ({id})")
    }
}

pub fn credential_service(provider: &str) -> String {
    crate::credentials::llm_credential_service(provider)
}

pub fn api_base(provider: &str, custom_base: Option<&str>) -> String {
    match provider {
        "deepseek" => custom_base
            .unwrap_or("https://api.deepseek.com")
            .to_string(),
        id if is_custom_provider(id) => custom_base
            .unwrap_or("https://api.openai.com/v1")
            .to_string(),
        _ => custom_base
            .unwrap_or("https://api.openai.com/v1")
            .to_string(),
    }
}

/// Anthropic Messages API（保留供内联/旧路径；设置页已不暴露）。
pub fn uses_anthropic_messages_api(provider: &str) -> bool {
    provider == "anthropic"
}

pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

pub const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 8192;

/// OpenAI-compatible `POST …/chat/completions`（base 可带或不带 `/v1`）。
pub fn chat_completions_url(base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    if base.ends_with("/v1") {
        format!("{base}/chat/completions")
    } else {
        format!("{base}/v1/chat/completions")
    }
}

/// GET URL for connectivity probe (provider-specific; DeepSeek uses `/models` without `/v1`).
pub fn models_probe_url(provider: &str, base_url: &str) -> String {
    let base = base_url.trim_end_matches('/');
    match provider {
        "deepseek" => {
            let root = base.strip_suffix("/v1").unwrap_or(base);
            format!("{root}/models")
        }
        _ => format!("{base}/models"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_probe_url_uses_root_models_endpoint() {
        assert_eq!(
            models_probe_url("deepseek", "https://api.deepseek.com"),
            "https://api.deepseek.com/models"
        );
        assert_eq!(
            models_probe_url("deepseek", "https://api.deepseek.com/v1"),
            "https://api.deepseek.com/models"
        );
    }

    #[test]
    fn deepseek_chat_url_adds_v1_when_base_has_no_suffix() {
        assert_eq!(
            chat_completions_url("https://api.deepseek.com"),
            "https://api.deepseek.com/v1/chat/completions"
        );
    }

    #[test]
    fn custom_provider_ids() {
        assert!(is_custom_provider("custom"));
        assert!(is_custom_provider("custom_local"));
        assert!(!is_custom_provider("deepseek"));
    }

    #[test]
    fn list_from_routing_includes_custom_entries() {
        let mut routing = crate::llm::config::deepseek_defaults();
        routing.providers.insert(
            "custom_groq".into(),
            ProviderOverride {
                base_url: Some("https://api.groq.com/openai/v1".into()),
                label: Some("Groq".into()),
                default_model: Some("llama-3.1-8b-instant".into()),
            },
        );
        let ids: Vec<_> = list_providers_from_routing(&routing)
            .into_iter()
            .map(|p| p.id)
            .collect();
        assert_eq!(ids, vec!["deepseek", "custom_groq"]);
    }
}
