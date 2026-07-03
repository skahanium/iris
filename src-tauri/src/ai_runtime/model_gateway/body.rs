use serde::{Deserialize, Serialize};

use crate::ai_types::{
    ContentPart, EndpointFamily, LlmMessage, MessageContent, MessageRole, ProviderConfig,
};
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
    pub input_token_budget: Option<u32>,
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
    enforce_input_token_budget(&messages, &request.tools, request.input_token_budget)?;
    let mut req = request.clone();
    req.messages = messages;
    Ok(match req.provider.endpoint_family {
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            build_chat_completions_body_inner(&req)
        }
        EndpointFamily::AnthropicMessages => build_anthropic_messages_body_inner(&req),
    })
}

fn enforce_input_token_budget(
    messages: &[LlmMessage],
    tools: &[LlmToolDef],
    input_token_budget: Option<u32>,
) -> AppResult<()> {
    let Some(limit) = input_token_budget else {
        return Ok(());
    };
    let estimated = estimate_gateway_input_tokens(messages, tools);
    if estimated > limit {
        return Err(AppError::msg(format!(
            "llm_input_context_overflow: estimated input tokens {estimated} exceed model input budget {limit}; reduce context or history before retrying"
        )));
    }
    Ok(())
}

fn estimate_gateway_input_tokens(messages: &[LlmMessage], tools: &[LlmToolDef]) -> u32 {
    let message_tokens = messages
        .iter()
        .map(estimate_message_input_tokens)
        .fold(0u32, u32::saturating_add);
    let tool_tokens = if tools.is_empty() {
        0
    } else {
        estimate_text_tokens(&serde_json::to_string(tools).unwrap_or_default())
    };
    message_tokens.saturating_add(tool_tokens)
}

fn estimate_message_input_tokens(message: &LlmMessage) -> u32 {
    let mut tokens = 4u32;
    tokens = tokens.saturating_add(estimate_text_tokens(&message.content.text_content()));
    if let Some(tool_call_id) = &message.tool_call_id {
        tokens = tokens.saturating_add(estimate_text_tokens(tool_call_id));
    }
    if let Some(tool_calls) = &message.tool_calls {
        tokens = tokens.saturating_add(estimate_text_tokens(
            &serde_json::to_string(tool_calls).unwrap_or_default(),
        ));
    }
    if let Some(reasoning) = &message.reasoning_content {
        tokens = tokens.saturating_add(estimate_text_tokens(reasoning));
    }
    tokens
}

fn estimate_text_tokens(text: &str) -> u32 {
    if text.is_empty() {
        return 0;
    }
    let chars = text.chars().count() as u32;
    let cjk = text
        .chars()
        .filter(|ch| matches!(*ch as u32, 0x4E00..=0x9FFF | 0x3400..=0x4DBF | 0x3040..=0x30FF | 0xAC00..=0xD7AF))
        .count() as u32;
    let non_cjk = chars.saturating_sub(cjk);
    cjk.saturating_add(non_cjk.div_ceil(4)).max(1)
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

/// Convert `MessageContent` to Anthropic-compatible JSON.
///
/// - `Text` → plain string (Anthropic accepts string content)
/// - `Parts` → array of content blocks with `ImageUrl` converted to Anthropic image format
fn content_to_anthropic_json(content: &MessageContent) -> serde_json::Value {
    match content {
        MessageContent::Text(s) => serde_json::Value::String(s.clone()),
        MessageContent::Parts(parts) => {
            let blocks: Vec<serde_json::Value> = parts
                .iter()
                .map(|part| match part {
                    ContentPart::Text { text } => {
                        serde_json::json!({ "type": "text", "text": text })
                    }
                    ContentPart::ImageUrl { image_url } => {
                        let (media_type, data) = parse_data_url(&image_url.url);
                        serde_json::json!({
                            "type": "image",
                            "source": {
                                "type": "base64",
                                "media_type": media_type,
                                "data": data,
                            }
                        })
                    }
                })
                .collect();
            serde_json::Value::Array(blocks)
        }
    }
}

/// Parse `data:image/png;base64,xxxxx` into `(media_type, base64_data)`.
fn parse_data_url(url: &str) -> (&str, &str) {
    let after_data = url.strip_prefix("data:").unwrap_or(url);
    let comma_pos = after_data.find(',').unwrap_or(after_data.len());
    let media_type_end = after_data[..comma_pos]
        .rfind(";base64")
        .unwrap_or(comma_pos);
    (&after_data[..media_type_end], &after_data[comma_pos + 1..])
}

fn build_anthropic_messages_body_inner(request: &GatewayRequest) -> serde_json::Value {
    let mut system_parts = Vec::new();
    let mut messages = Vec::new();
    for message in &request.messages {
        match message.role {
            MessageRole::System => system_parts.push(message.content.text_content()),
            MessageRole::Assistant => messages.push(serde_json::json!({
                "role": "assistant",
                "content": content_to_anthropic_json(&message.content),
            })),
            MessageRole::User | MessageRole::Tool => messages.push(serde_json::json!({
                "role": "user",
                "content": content_to_anthropic_json(&message.content),
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
            input_token_budget: None,
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
    fn parse_data_url_extracts_media_type_and_data() {
        let (mt, data) = super::parse_data_url("data:image/png;base64,iVBORw0KGgo=");
        assert_eq!(mt, "image/png");
        assert_eq!(data, "iVBORw0KGgo=");
    }

    #[test]
    fn parse_data_url_handles_jpeg() {
        let (mt, data) = super::parse_data_url("data:image/jpeg;base64,/9j/4AAQ");
        assert_eq!(mt, "image/jpeg");
        assert_eq!(data, "/9j/4AAQ");
    }

    #[test]
    fn build_llm_api_body_rejects_input_budget_overflow_before_provider_call() {
        let mut request = request_for(EndpointFamily::OpenAiCompatibleChatCompletions);
        request.input_token_budget = Some(8);
        request.messages[0].content = "x".repeat(200).into();

        let err = build_llm_api_body(&request).unwrap_err().to_string();

        assert!(err.contains("llm_input_context_overflow"));
        assert!(!err.contains(&"x".repeat(32)));
    }

    #[test]
    fn anthropic_body_converts_image_url_to_image_source_block() {
        use crate::ai_types::{ContentPart, ImageUrlPayload, MessageContent};

        let request = GatewayRequest {
            provider: ProviderConfig {
                name: "anthropic".into(),
                base_url: "https://api.anthropic.com".into(),
                api_key: Some("key".into()),
                model: "claude-3-5-haiku".into(),
                slot: CapabilitySlot::Vision,
                endpoint_family: EndpointFamily::AnthropicMessages,
            },
            messages: vec![LlmMessage {
                role: MessageRole::User,
                content: MessageContent::Parts(vec![
                    ContentPart::Text {
                        text: "describe this".into(),
                    },
                    ContentPart::ImageUrl {
                        image_url: ImageUrlPayload {
                            url: "data:image/png;base64,abc123".into(),
                            detail: None,
                        },
                    },
                ]),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            }],
            tools: vec![],
            max_tokens: Some(100),
            input_token_budget: None,
            temperature: None,
            stream: false,
            thinking: false,
            skip_stub_ids: vec![],
        };

        let body = build_llm_api_body(&request).unwrap();
        let content = &body["messages"][0]["content"];

        // Should be an array of content blocks
        assert!(content.is_array());
        let arr = content.as_array().unwrap();
        assert_eq!(arr.len(), 2);

        // First block: text
        assert_eq!(arr[0]["type"], "text");
        assert_eq!(arr[0]["text"], "describe this");

        // Second block: image (Anthropic format)
        assert_eq!(arr[1]["type"], "image");
        assert_eq!(arr[1]["source"]["type"], "base64");
        assert_eq!(arr[1]["source"]["media_type"], "image/png");
        assert_eq!(arr[1]["source"]["data"], "abc123");
    }
}
