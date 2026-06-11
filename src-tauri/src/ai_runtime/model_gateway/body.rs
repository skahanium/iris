use serde::{Deserialize, Serialize};

use crate::ai_types::{LlmMessage, ProviderConfig};
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
    Ok(build_chat_completions_body_inner(&req))
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
