//! Static model capability catalog (context window, output limits).

use serde::{Deserialize, Serialize};

use crate::ai_types::{EndpointFamily, ProbeStrategy};

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelCatalogEntry {
    pub id: &'static str,
    pub provider_id: &'static str,
    pub display_name: &'static str,
    pub context_window: u32,
    pub max_output: u32,
    pub supports_tools: bool,
    pub supports_thinking: bool,
    pub supports_vision: bool,
    pub supports_streaming: bool,
    pub cache_friendly: bool,
    pub endpoint_family: EndpointFamily,
    pub probe_strategy: ProbeStrategy,
}

const ONE_M: u32 = 1_048_576;

/// All known models (extensible without changing scene logic).
pub fn catalog() -> &'static [ModelCatalogEntry] {
    &[
        ModelCatalogEntry {
            id: "deepseek-v4-flash",
            provider_id: "deepseek",
            display_name: "DeepSeek V4 Flash",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: true,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "deepseek-v4-pro",
            provider_id: "deepseek",
            display_name: "DeepSeek V4 Pro",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: true,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: true,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "deepseek-chat",
            provider_id: "deepseek",
            display_name: "DeepSeek Chat (legacy → V4 Flash)",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: true,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "deepseek-reasoner",
            provider_id: "deepseek",
            display_name: "DeepSeek Reasoner (legacy → V4 Flash thinking)",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: true,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: true,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "gpt-4o-mini",
            provider_id: "openai",
            display_name: "GPT-4o mini",
            context_window: 128_000,
            max_output: 16_384,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: true,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "gpt-4o",
            provider_id: "openai",
            display_name: "GPT-4o",
            context_window: 128_000,
            max_output: 16_384,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: true,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "claude-3-5-haiku-20241022",
            provider_id: "anthropic",
            display_name: "Claude 3.5 Haiku",
            context_window: 200_000,
            max_output: 8_192,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: true,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::AnthropicMessages,
            probe_strategy: ProbeStrategy::AnthropicMessagesPing,
        },
        ModelCatalogEntry {
            id: "glm-4-flash",
            provider_id: "zhipu",
            display_name: "GLM-4 Flash",
            context_window: 128_000,
            max_output: 16_384,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "moonshot-v1-128k",
            provider_id: "kimi",
            display_name: "Kimi 128K",
            context_window: 128_000,
            max_output: 8_192,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "doubao-1-5-pro-256k",
            provider_id: "doubao",
            display_name: "Doubao 1.5 Pro 256K",
            context_window: 256_000,
            max_output: 12_288,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "llama3.2",
            provider_id: "ollama",
            display_name: "Llama 3.2",
            context_window: 128_000,
            max_output: 8_192,
            supports_tools: false,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OllamaChat,
            probe_strategy: ProbeStrategy::OllamaTagsThenChat,
        },
        ModelCatalogEntry {
            id: "default",
            provider_id: "custom",
            display_name: "Custom default",
            context_window: 128_000,
            max_output: 8_192,
            supports_tools: true,
            supports_thinking: false,
            supports_vision: false,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::OpenAiModelsThenChat,
        },
        ModelCatalogEntry {
            id: "mimo-vl-7b-experimental",
            provider_id: "mimo",
            display_name: "MiMo VL 7B Experimental",
            context_window: 64_000,
            max_output: 8_192,
            supports_tools: false,
            supports_thinking: false,
            supports_vision: true,
            supports_streaming: true,
            cache_friendly: false,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
            probe_strategy: ProbeStrategy::StaticOnly,
        },
    ]
}

pub fn find_model(model_id: &str) -> Option<&'static ModelCatalogEntry> {
    catalog().iter().find(|m| m.id == model_id)
}

pub fn models_for_provider(provider_id: &str) -> Vec<&'static ModelCatalogEntry> {
    catalog()
        .iter()
        .filter(|m| m.provider_id == provider_id)
        .collect()
}

pub fn fallback_model(provider_id: &str) -> &'static ModelCatalogEntry {
    if let Some(m) = models_for_provider(provider_id).first() {
        return m;
    }
    if crate::llm::providers::is_custom_provider(provider_id) {
        return find_model("default").expect("catalog has custom default");
    }
    find_model("deepseek-v4-flash").expect("catalog has deepseek-v4-flash")
}

/// 设置页使用的静态模型目录；自定义端点仍允许手填模型名。
pub fn catalog_for_settings() -> Vec<ModelCatalogEntry> {
    catalog().to_vec()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_v4_has_one_m_context() {
        let flash = find_model("deepseek-v4-flash").unwrap();
        assert_eq!(flash.context_window, 1_048_576);
    }

    #[test]
    fn phase3_catalog_exposes_default_provider_scope_and_capabilities() {
        for provider in [
            "deepseek",
            "openai",
            "anthropic",
            "zhipu",
            "kimi",
            "doubao",
            "ollama",
            "custom",
            "mimo",
        ] {
            assert!(
                catalog_for_settings()
                    .iter()
                    .any(|model| model.provider_id == provider),
                "missing provider {provider}"
            );
        }

        let gpt_4o = find_model("gpt-4o").expect("gpt-4o in catalog");
        assert!(gpt_4o.supports_vision);
        assert!(gpt_4o.supports_streaming);
        assert_eq!(
            gpt_4o.endpoint_family,
            crate::ai_types::EndpointFamily::OpenAiCompatibleChatCompletions
        );
    }

    #[test]
    fn custom_manual_model_fallback_has_capability_metadata() {
        let fallback = fallback_model("custom");
        assert_eq!(fallback.provider_id, "custom");
        assert!(fallback.supports_tools);
        assert_eq!(
            fallback.probe_strategy,
            crate::ai_types::ProbeStrategy::OpenAiModelsThenChat
        );
    }
}
