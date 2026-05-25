use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LlmProviderInfo {
    pub id: String,
    pub name: String,
    pub default_model: String,
}

pub fn list_providers() -> Vec<LlmProviderInfo> {
    vec![
        LlmProviderInfo {
            id: "openai".into(),
            name: "OpenAI".into(),
            default_model: "gpt-4o-mini".into(),
        },
        LlmProviderInfo {
            id: "anthropic".into(),
            name: "Anthropic Claude".into(),
            default_model: "claude-3-5-haiku-20241022".into(),
        },
        LlmProviderInfo {
            id: "ollama".into(),
            name: "Ollama".into(),
            default_model: "llama3.2".into(),
        },
        LlmProviderInfo {
            id: "custom".into(),
            name: "Custom OpenAI-compatible".into(),
            default_model: "default".into(),
        },
    ]
}

pub fn credential_service(provider: &str) -> String {
    format!("iris/llm/{provider}")
}

pub fn api_base(provider: &str, custom_base: Option<&str>) -> String {
    match provider {
        "openai" => "https://api.openai.com/v1".into(),
        "anthropic" => "https://api.anthropic.com/v1".into(),
        "ollama" => custom_base
            .unwrap_or("http://127.0.0.1:11434/v1")
            .to_string(),
        _ => custom_base
            .unwrap_or("https://api.openai.com/v1")
            .to_string(),
    }
}

/// Anthropic Messages API（非 OpenAI 兼容 `/chat/completions`）。
pub fn uses_anthropic_messages_api(provider: &str) -> bool {
    provider == "anthropic"
}

pub const ANTHROPIC_API_VERSION: &str = "2023-06-01";

pub const ANTHROPIC_DEFAULT_MAX_TOKENS: u32 = 8192;
