//! Static model capability catalog (context window, output limits).

use serde::{Deserialize, Serialize};

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
    pub cache_friendly: bool,
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
            cache_friendly: true,
        },
        ModelCatalogEntry {
            id: "deepseek-v4-pro",
            provider_id: "deepseek",
            display_name: "DeepSeek V4 Pro",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: true,
            cache_friendly: true,
        },
        ModelCatalogEntry {
            id: "deepseek-chat",
            provider_id: "deepseek",
            display_name: "DeepSeek Chat (legacy → V4 Flash)",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: false,
            cache_friendly: true,
        },
        ModelCatalogEntry {
            id: "deepseek-reasoner",
            provider_id: "deepseek",
            display_name: "DeepSeek Reasoner (legacy → V4 Flash thinking)",
            context_window: ONE_M,
            max_output: 384_000,
            supports_tools: true,
            supports_thinking: true,
            cache_friendly: true,
        },
        ModelCatalogEntry {
            id: "gpt-4o-mini",
            provider_id: "openai",
            display_name: "GPT-4o mini",
            context_window: 128_000,
            max_output: 16_384,
            supports_tools: true,
            supports_thinking: false,
            cache_friendly: false,
        },
        ModelCatalogEntry {
            id: "gpt-4o",
            provider_id: "openai",
            display_name: "GPT-4o",
            context_window: 128_000,
            max_output: 16_384,
            supports_tools: true,
            supports_thinking: false,
            cache_friendly: false,
        },
        ModelCatalogEntry {
            id: "claude-3-5-haiku-20241022",
            provider_id: "anthropic",
            display_name: "Claude 3.5 Haiku",
            context_window: 200_000,
            max_output: 8_192,
            supports_tools: true,
            supports_thinking: false,
            cache_friendly: false,
        },
        ModelCatalogEntry {
            id: "llama3.2",
            provider_id: "ollama",
            display_name: "Llama 3.2",
            context_window: 128_000,
            max_output: 8_192,
            supports_tools: false,
            supports_thinking: false,
            cache_friendly: false,
        },
        ModelCatalogEntry {
            id: "default",
            provider_id: "custom",
            display_name: "Custom default",
            context_window: 128_000,
            max_output: 8_192,
            supports_tools: true,
            supports_thinking: false,
            cache_friendly: false,
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

/// 设置页与场景下拉使用的模型目录（仅 DeepSeek；自定义端点用手填模型名）。
pub fn catalog_for_settings() -> Vec<ModelCatalogEntry> {
    catalog()
        .iter()
        .filter(|m| m.provider_id == "deepseek")
        .copied()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deepseek_v4_has_one_m_context() {
        let flash = find_model("deepseek-v4-flash").unwrap();
        assert_eq!(flash.context_window, 1_048_576);
    }
}
