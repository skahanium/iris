use serde::{Deserialize, Serialize};

use crate::ai_types::{EndpointFamily, LlmMessage, MessageRole, ProviderConfig};
use crate::error::{AppError, AppResult};

use super::{messages_for_api, prepare_tool_api_messages, tool_api_message_chain_valid};

/// Tool definition for LLM function-calling format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmToolDef {
    #[serde(rename = "type")]
    pub tool_type: String,
    pub function: LlmFunctionDef,
}

/// Function definition for LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmFunctionDef {
    pub name: String,
    pub description: String,
    pub parameters: serde_json::Value,
}

/// Gateway request configuration.
#[derive(Debug, Clone)]
pub struct GatewayRequest {
    pub provider: ProviderConfig,
    pub messages: Vec<LlmMessage>,
    pub tools: Vec<LlmToolDef>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f64>,
    pub stream: bool,
    /// When true, send provider thinking-mode parameters (DeepSeek-compatible).
    pub thinking: bool,
    /// Tool call IDs still awaiting user confirmation - must not receive error stubs.
    pub skip_stub_ids: Vec<String>,
}

fn messages_need_tool_prep(messages: &[LlmMessage], tools: &[LlmToolDef]) -> bool {
    !tools.is_empty()
        || messages
            .iter()
            .any(|m| matches!(m.role, crate::ai_types::MessageRole::Tool))
}

/// Build OpenAI-compatible chat-completions JSON body (tests + checkpoint validation).
/// Honors `skip_stub_ids` - use only in tests; live sends go through `build_llm_api_body`.
pub fn build_chat_completions_body(request: &GatewayRequest) -> serde_json::Value {
    let mut messages = request.messages.clone();
    if messages_need_tool_prep(&messages, &request.tools) {
        prepare_tool_api_messages(&mut messages, &request.skip_stub_ids);
    }
    let mut req = request.clone();
    req.messages = messages;
    build_chat_completions_body_inner(&req)
}

/// Build API body for a live LLM request - never leaves pending-confirm tool gaps unstubbed.
pub(super) fn build_llm_api_body(request: &GatewayRequest) -> AppResult<serde_json::Value> {
    let mut messages = request.messages.clone();
    if messages_need_tool_prep(&messages, &request.tools) {
        prepare_tool_api_messages(&mut messages, &[]);
        if !tool_api_message_chain_valid(&messages) {
            return Err(AppError::msg(
                "工具续聊消息序列无效（tool 行缺少对应的 assistant tool_calls）",
            ));
        }
    }
    let mut req = request.clone();
    req.messages = messages;
    Ok(match req.provider.endpoint_family {
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            build_chat_completions_body_inner(&req)
        }
        EndpointFamily::AnthropicMessages => build_anthropic_messages_body_inner(&req),
        EndpointFamily::OllamaChat => build_ollama_chat_body_inner(&req),
    })
}

fn build_chat_completions_body_inner(request: &GatewayRequest) -> serde_json::Value {
    let messages = &request.messages;

    let mut body = serde_json::json!({
        "model": request.provider.model,
        "messages": messages_for_api(messages),
    });

    if !request.tools.is_empty() {
        body["tools"] = serde_json::to_value(&request.tools).unwrap_or_default();
    }

    if let Some(max_tokens) = request.max_tokens {
        body["max_tokens"] = serde_json::json!(max_tokens);
    }

    if let Some(temperature) = request.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }

    apply_thinking_body(&mut body, request.thinking);
    body
}

fn apply_thinking_body(body: &mut serde_json::Value, thinking: bool) {
    if thinking {
        body["thinking"] = serde_json::json!({ "type": "enabled" });
    }
}

fn build_anthropic_messages_body_inner(request: &GatewayRequest) -> serde_json::Value {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    for message in &request.messages {
        match message.role {
            MessageRole::System => system_parts.push(message.content.clone()),
            MessageRole::Assistant => messages.push(serde_json::json!({
                "role": "assistant",
                "content": message.content,
            })),
            MessageRole::User | MessageRole::Tool => messages.push(serde_json::json!({
                "role": "user",
                "content": message.content,
            })),
        }
    }

    let mut body = serde_json::json!({
        "model": request.provider.model,
        "max_tokens": request.max_tokens.unwrap_or(crate::llm::providers::ANTHROPIC_DEFAULT_MAX_TOKENS),
        "messages": messages,
    });
    if !system_parts.is_empty() {
        body["system"] = serde_json::json!(system_parts.join("\n\n"));
    } else {
        body["system"] = serde_json::json!("");
    }
    if !request.tools.is_empty() {
        body["tools"] = serde_json::Value::Array(
            request
                .tools
                .iter()
                .map(|tool| {
                    serde_json::json!({
                        "name": tool.function.name,
                        "description": tool.function.description,
                        "input_schema": tool.function.parameters,
                    })
                })
                .collect(),
        );
    }
    if let Some(temperature) = request.temperature {
        body["temperature"] = serde_json::json!(temperature);
    }
    body
}

fn build_ollama_chat_body_inner(request: &GatewayRequest) -> serde_json::Value {
    let mut body = serde_json::json!({
        "model": request.provider.model,
        "messages": messages_for_api(&request.messages),
        "stream": request.stream,
    });
    if !request.tools.is_empty() {
        body["tools"] = serde_json::to_value(&request.tools).unwrap_or_default();
    }
    if let Some(temperature) = request.temperature {
        body["options"] = serde_json::json!({ "temperature": temperature });
    }
    body
}

#[cfg(test)]
mod phase3_adapter_contract_tests {
    use super::*;
    use crate::ai_types::{CapabilitySlot, EndpointFamily, MessageRole};

    fn request_for(endpoint_family: EndpointFamily) -> GatewayRequest {
        GatewayRequest {
            provider: ProviderConfig {
                name: "test".into(),
                base_url: "https://api.example.com".into(),
                api_key: Some("secret".into()),
                model: "model-a".into(),
                slot: CapabilitySlot::Fast,
                endpoint_family,
            },
            messages: vec![LlmMessage {
                role: MessageRole::User,
                content: "ping".into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            }],
            tools: vec![LlmToolDef {
                tool_type: "function".into(),
                function: LlmFunctionDef {
                    name: "search_hybrid".into(),
                    description: "Search notes".into(),
                    parameters: serde_json::json!({"type": "object"}),
                },
            }],
            max_tokens: Some(8),
            temperature: Some(0.2),
            stream: false,
            thinking: false,
            skip_stub_ids: vec![],
        }
    }

    #[test]
    fn builds_anthropic_messages_body_from_unified_request() {
        let body = build_llm_api_body(&request_for(EndpointFamily::AnthropicMessages)).unwrap();

        assert_eq!(body["model"], "model-a");
        assert_eq!(body["max_tokens"], 8);
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["tools"][0]["name"], "search_hybrid");
        assert!(body.get("system").is_some());
        assert!(body.get("stream").is_none());
    }

    #[test]
    fn builds_ollama_chat_body_from_unified_request() {
        let body = build_llm_api_body(&request_for(EndpointFamily::OllamaChat)).unwrap();

        assert_eq!(body["model"], "model-a");
        assert_eq!(body["messages"][0]["role"], "user");
        assert_eq!(body["stream"], false);
        assert_eq!(body["tools"][0]["function"]["name"], "search_hybrid");
    }
}
