//! Provider-owned wire-contract facts for the built-in model catalog.
//!
//! "OpenAI compatible" describes a broad envelope, not a guarantee that
//! reasoning, streaming, or a tool continuation has identical semantics. This
//! table is deliberately small and explicit so routing can safely decline an
//! unverified Agent capability instead of guessing at provider-private fields.

/// How a provider expects an assistant tool turn to be resumed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ToolContinuationMode {
    /// The model is chat-only or its tool continuation has not been verified.
    Disabled,
    /// Standard OpenAI Chat Completions `assistant.tool_calls` -> `tool` chain.
    OpenAiChatCompletions,
    /// Anthropic `tool_use` / `tool_result` content blocks.
    AnthropicMessages,
    /// OpenAI Responses `previous_response_id` continuation.
    OpenAiResponses,
    /// MiniMax M3 preserves `reasoning_details` alongside `tool_calls`.
    MiniMaxReasoningDetails,
}

/// Static capability facts that have a protocol fixture in the gateway suite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderProtocolContract {
    pub chat: bool,
    pub streaming: bool,
    pub tools: bool,
    pub parallel_tools: bool,
    pub tool_continuation: ToolContinuationMode,
}

impl ProviderProtocolContract {
    pub const fn chat_only() -> Self {
        Self {
            chat: true,
            streaming: true,
            tools: false,
            parallel_tools: false,
            tool_continuation: ToolContinuationMode::Disabled,
        }
    }

    const fn openai_compatible() -> Self {
        Self {
            chat: true,
            streaming: true,
            tools: true,
            parallel_tools: true,
            tool_continuation: ToolContinuationMode::OpenAiChatCompletions,
        }
    }
}

/// Return the contract for a configured provider/model pair.
///
/// Built-in models use a fixture-backed profile. Custom endpoints intentionally
/// remain chat-only until a future explicit live capability probe establishes a
/// provider-specific contract.
pub fn provider_protocol_contract(provider_id: &str, model_id: &str) -> ProviderProtocolContract {
    match provider_id {
        provider if provider == "custom" || provider.starts_with("custom_") => {
            ProviderProtocolContract::chat_only()
        }
        "anthropic" => ProviderProtocolContract {
            tool_continuation: ToolContinuationMode::AnthropicMessages,
            ..ProviderProtocolContract::openai_compatible()
        },
        "openai"
            if model_id.starts_with("o1")
                || model_id.starts_with("o3")
                || model_id.starts_with("o4")
                || model_id.starts_with("gpt-5") =>
        {
            ProviderProtocolContract {
                tool_continuation: ToolContinuationMode::OpenAiResponses,
                ..ProviderProtocolContract::openai_compatible()
            }
        }
        "openai" => ProviderProtocolContract::openai_compatible(),
        "minimax" => ProviderProtocolContract {
            tool_continuation: ToolContinuationMode::MiniMaxReasoningDetails,
            ..ProviderProtocolContract::openai_compatible()
        },
        // Each of these built-in endpoints exposes the documented OpenAI-style
        // function-call envelope. Their request/response extensions are kept
        // in the reasoning and streaming adapters rather than guessed here.
        "deepseek" | "google" | "qwen" | "zhipu" | "kimi" | "doubao" | "hunyuan" | "ernie"
        | "mimo" | "ollama" => ProviderProtocolContract::openai_compatible(),
        _ => ProviderProtocolContract::chat_only(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_provider_has_an_explicit_contract() {
        for provider in [
            "deepseek",
            "openai",
            "anthropic",
            "google",
            "qwen",
            "zhipu",
            "kimi",
            "doubao",
            "minimax",
            "hunyuan",
            "ernie",
            "mimo",
            "ollama",
        ] {
            let contract = provider_protocol_contract(provider, "default-model");
            assert!(contract.chat, "{provider} must support chat");
            assert!(contract.streaming, "{provider} must support streaming");
            assert!(
                contract.tools,
                "{provider} default chat model must support tools"
            );
            assert!(
                contract.parallel_tools,
                "{provider} must declare parallel tool behavior"
            );
            assert_ne!(
                contract.tool_continuation,
                ToolContinuationMode::Disabled,
                "{provider} must declare its tool continuation"
            );
        }
    }

    #[test]
    fn custom_endpoints_are_chat_only_until_explicitly_verified() {
        let contract = provider_protocol_contract("custom_openrouter", "unknown");
        assert!(!contract.tools);
        assert_eq!(contract.tool_continuation, ToolContinuationMode::Disabled);
    }

    #[test]
    fn minimax_uses_its_own_tool_continuation_contract() {
        assert_eq!(
            provider_protocol_contract("minimax", "MiniMax-M3").tool_continuation,
            ToolContinuationMode::MiniMaxReasoningDetails
        );
    }
}
