//! Model Gateway — LLM provider abstraction with streaming and tool-calling.
//!
//! Handles:
//! 1. Selecting provider/model by capability slot
//! 2. Building messages with system prompt + context packets
//! 3. Streaming responses via Tauri events
//! 4. Processing tool calls from LLM and routing to ToolExecutor

use crate::ai_runtime::{AiScene, ContextPacket, ToolSpec};
use crate::error::{AppError, AppResult};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::{AppHandle, Emitter};

// ─── Provider Types ──────────────────────────────────────

/// LLM provider configuration (from settings or registry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub slot: CapabilitySlot,
}

/// Capability slots for model selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySlot {
    Fast,
    Writer,
    Reasoner,
    LongContext,
    Embedding,
    Reranker,
    LocalPrivate,
}

/// Message role for LLM conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
}

/// Tool call from LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    pub function: FunctionCall,
}

/// Function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

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
}

/// Gateway response (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: String,
}

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

/// Streaming event emitted to frontend.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StreamEvent {
    pub request_id: String,
    pub event_type: StreamEventType,
    pub data: StreamEventData,
}

/// Stream event types.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StreamEventType {
    Token,
    ToolCall,
    Done,
    Error,
}

/// Stream event data payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StreamEventData {
    Token { token: String },
    ToolCall { tool_call: ToolCall },
    Done { usage: Option<TokenUsage> },
    Error { message: String },
}

// ─── Model Gateway ───────────────────────────────────────

/// Model Gateway: handles LLM provider communication.
pub struct ModelGateway {
    app_handle: AppHandle,
    providers: HashMap<CapabilitySlot, ProviderConfig>,
}

impl ModelGateway {
    /// Create a new gateway with provider configurations.
    pub fn new(app_handle: AppHandle, providers: Vec<ProviderConfig>) -> Self {
        let mut provider_map = HashMap::new();
        for p in providers {
            provider_map.insert(p.slot, p);
        }
        Self {
            app_handle,
            providers: provider_map,
        }
    }

    /// Get provider for a capability slot.
    pub fn get_provider(&self, slot: CapabilitySlot) -> Option<&ProviderConfig> {
        self.providers.get(&slot)
    }

    /// Select appropriate capability slot for scene.
    pub fn slot_for_scene(scene: AiScene) -> CapabilitySlot {
        match scene {
            AiScene::KnowledgeLookup => CapabilitySlot::Fast,
            AiScene::ExemplarLearning => CapabilitySlot::Writer,
            AiScene::DraftingAssist => CapabilitySlot::Writer,
            AiScene::ResearchSynthesis => CapabilitySlot::Reasoner,
        }
    }

    /// Build system prompt for a scene with context packets.
    pub fn build_system_prompt(
        scene: AiScene,
        packets: &[ContextPacket],
        user_rules: &[String],
    ) -> String {
        let mut prompt = String::new();

        // Scene-specific persona
        match scene {
            AiScene::KnowledgeLookup => {
                prompt.push_str("你是「知识管家」，帮助用户在本地知识库中查找、解释、引用材料。\n");
                prompt
                    .push_str("你的回答必须基于提供的证据包，引用时使用 [citation_label] 格式。\n");
                prompt.push_str("如果证据不足，直接说明缺少材料，不要编造。\n");
            }
            AiScene::ExemplarLearning => {
                prompt.push_str("你是「学习伴侣」，帮助用户分析范文结构、表达方式和写作技巧。\n");
                prompt.push_str("分析时要指出具体的结构特征、常用句式和法规引用方式。\n");
                prompt.push_str("可以建议可复用的模板，但必须经过用户确认才能保存。\n");
            }
            AiScene::DraftingAssist => {
                prompt.push_str("你是「写作伴侣」，帮助用户在文稿创作中提供低干扰写作辅助。\n");
                prompt.push_str("提供结构建议、段落生成、改写润色和法规引用建议。\n");
                prompt.push_str("写入操作（插入文本、替换选区）必须经过用户确认。\n");
                prompt.push_str("反抄袭保护：不直接注入范文长段原文。\n");
            }
            AiScene::ResearchSynthesis => {
                prompt.push_str("你是「研究助理」，帮助用户对多材料进行论证组织和证据缺口分析。\n");
                prompt.push_str("支持子命题拆解、证据矩阵构建、论证链检测和缺口识别。\n");
                prompt.push_str("联网研究必须经过用户授权。\n");
            }
        }

        // Inject context packets as evidence
        if !packets.is_empty() {
            prompt.push_str("\n## 证据包\n\n");
            prompt.push_str("以下是检索到的证据材料，回答时必须引用来源：\n\n");
            for packet in packets {
                prompt.push_str(&format!(
                    "### {} ({})\n",
                    packet.citation_label, packet.title
                ));
                if let Some(path) = &packet.source_path {
                    prompt.push_str(&format!("来源: {}\n", path));
                }
                if let Some(heading) = &packet.heading_path {
                    prompt.push_str(&format!("章节: {}\n", heading));
                }
                prompt.push_str(&format!("相关度: {:.0}%\n", packet.score * 100.0));
                prompt.push_str(&format!("{}\n\n", packet.excerpt));
            }
        }

        // Inject user rules
        if !user_rules.is_empty() {
            prompt.push_str("\n## 用户规则\n\n");
            for rule in user_rules {
                prompt.push_str(&format!("- {}\n", rule));
            }
        }

        prompt
    }

    /// Convert ToolSpec to LLM tool definition format.
    pub fn tools_to_llm_format(tools: &[ToolSpec]) -> Vec<LlmToolDef> {
        tools
            .iter()
            .map(|t| LlmToolDef {
                tool_type: "function".into(),
                function: LlmFunctionDef {
                    name: t.name.clone(),
                    description: t.description.clone(),
                    parameters: t.input_schema.clone(),
                },
            })
            .collect()
    }

    /// Send a request to the LLM provider (non-streaming).
    pub async fn send_request(&self, request: GatewayRequest) -> AppResult<GatewayResponse> {
        let url = format!("{}/v1/chat/completions", request.provider.base_url);

        let mut body = serde_json::json!({
            "model": request.provider.model,
            "messages": request.messages,
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

        let client = reqwest::Client::new();
        let mut req_builder = client.post(&url).header("Content-Type", "application/json");

        if let Some(api_key) = &request.provider.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::msg(format!("LLM request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::msg(format!(
                "LLM request failed with status {}: {}",
                status, text
            )));
        }

        let json: serde_json::Value = response
            .json()
            .await
            .map_err(|e| AppError::msg(format!("Failed to parse LLM response: {}", e)))?;

        let content = json["choices"][0]["message"]["content"]
            .as_str()
            .map(|s| s.to_string());

        let tool_calls = json["choices"][0]["message"]["tool_calls"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|tc| {
                        Some(ToolCall {
                            id: tc["id"].as_str()?.to_string(),
                            function: FunctionCall {
                                name: tc["function"]["name"].as_str()?.to_string(),
                                arguments: tc["function"]["arguments"]
                                    .as_str()
                                    .unwrap_or("{}")
                                    .to_string(),
                            },
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let usage = TokenUsage {
            prompt_tokens: json["usage"]["prompt_tokens"].as_u64().unwrap_or(0) as u32,
            completion_tokens: json["usage"]["completion_tokens"].as_u64().unwrap_or(0) as u32,
            total_tokens: json["usage"]["total_tokens"].as_u64().unwrap_or(0) as u32,
        };

        let finish_reason = json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string();

        Ok(GatewayResponse {
            content,
            tool_calls,
            usage,
            finish_reason,
        })
    }

    /// Send a streaming request and emit events to frontend.
    pub async fn send_streaming_request(
        &self,
        request_id: &str,
        request: GatewayRequest,
    ) -> AppResult<GatewayResponse> {
        let url = format!("{}/v1/chat/completions", request.provider.base_url);

        let mut body = serde_json::json!({
            "model": request.provider.model,
            "messages": request.messages,
            "stream": true,
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

        let client = reqwest::Client::new();
        let mut req_builder = client.post(&url).header("Content-Type", "application/json");

        if let Some(api_key) = &request.provider.api_key {
            req_builder = req_builder.header("Authorization", format!("Bearer {}", api_key));
        }

        let response = req_builder
            .json(&body)
            .send()
            .await
            .map_err(|e| AppError::msg(format!("LLM streaming request failed: {}", e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let text = response.text().await.unwrap_or_default();
            return Err(AppError::msg(format!(
                "LLM streaming request failed with status {}: {}",
                status, text
            )));
        }

        let mut full_content = String::new();
        let mut tool_calls: Vec<ToolCall> = Vec::new();
        let mut usage = TokenUsage::default();

        // Process SSE stream
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;

        while let Some(chunk_result) = stream.next().await {
            let chunk =
                chunk_result.map_err(|e| AppError::msg(format!("Stream read error: {}", e)))?;

            let text = String::from_utf8_lossy(&chunk);
            for line in text.lines() {
                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    // Emit done event
                    let event = StreamEvent {
                        request_id: request_id.to_string(),
                        event_type: StreamEventType::Done,
                        data: StreamEventData::Done {
                            usage: Some(usage.clone()),
                        },
                    };
                    self.emit_stream_event(&event)?;
                    continue;
                }

                if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                    // Process content delta
                    if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                        full_content.push_str(delta);
                        let event = StreamEvent {
                            request_id: request_id.to_string(),
                            event_type: StreamEventType::Token,
                            data: StreamEventData::Token {
                                token: delta.to_string(),
                            },
                        };
                        self.emit_stream_event(&event)?;
                    }

                    // Process tool call deltas
                    if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
                        for tc_delta in tc_deltas {
                            if let (Some(id), Some(name), Some(args)) = (
                                tc_delta["id"].as_str(),
                                tc_delta["function"]["name"].as_str(),
                                tc_delta["function"]["arguments"].as_str(),
                            ) {
                                let tool_call = ToolCall {
                                    id: id.to_string(),
                                    function: FunctionCall {
                                        name: name.to_string(),
                                        arguments: args.to_string(),
                                    },
                                };
                                tool_calls.push(tool_call.clone());
                                let event = StreamEvent {
                                    request_id: request_id.to_string(),
                                    event_type: StreamEventType::ToolCall,
                                    data: StreamEventData::ToolCall { tool_call },
                                };
                                self.emit_stream_event(&event)?;
                            }
                        }
                    }

                    // Update usage if present
                    if let Some(prompt_tokens) = json["usage"]["prompt_tokens"].as_u64() {
                        usage.prompt_tokens = prompt_tokens as u32;
                    }
                    if let Some(completion_tokens) = json["usage"]["completion_tokens"].as_u64() {
                        usage.completion_tokens = completion_tokens as u32;
                    }
                    if let Some(total_tokens) = json["usage"]["total_tokens"].as_u64() {
                        usage.total_tokens = total_tokens as u32;
                    }
                }
            }
        }

        Ok(GatewayResponse {
            content: if full_content.is_empty() {
                None
            } else {
                Some(full_content)
            },
            tool_calls,
            usage,
            finish_reason: "stop".to_string(),
        })
    }

    /// Emit a stream event to the frontend.
    fn emit_stream_event(&self, event: &StreamEvent) -> AppResult<()> {
        let event_name = match event.event_type {
            StreamEventType::Token => "ai:token",
            StreamEventType::ToolCall => "ai:tool_call",
            StreamEventType::Done => "ai:done",
            StreamEventType::Error => "ai:error",
        };
        self.app_handle
            .emit(event_name, event)
            .map_err(|e| AppError::msg(format!("Failed to emit stream event: {}", e)))?;
        Ok(())
    }
}

// ─── Prompt Builder ──────────────────────────────────────

/// Build context-aware prompt for drafting scene.
pub fn build_drafting_prompt(
    document_outline: &str,
    cursor_context: &str,
    packets: &[ContextPacket],
    user_rules: &[String],
) -> String {
    let mut prompt = String::new();

    prompt.push_str("## 当前文稿大纲\n\n");
    prompt.push_str(document_outline);
    prompt.push_str("\n\n## 光标邻域上下文\n\n");
    prompt.push_str(cursor_context);

    if !packets.is_empty() {
        prompt.push_str("\n\n## 参考材料\n\n");
        for packet in packets {
            prompt.push_str(&format!("- [{}] {}\n", packet.citation_label, packet.title));
            prompt.push_str(&format!("  {}\n", packet.excerpt));
        }
    }

    if !user_rules.is_empty() {
        prompt.push_str("\n\n## 写作规则\n\n");
        for rule in user_rules {
            prompt.push_str(&format!("- {}\n", rule));
        }
    }

    prompt
}

/// Build citation suggestion prompt.
pub fn build_citation_prompt(paragraph: &str, candidates: &[ContextPacket]) -> String {
    let mut prompt = String::new();

    prompt.push_str("分析以下段落，推荐合适的法规引用：\n\n");
    prompt.push_str(paragraph);
    prompt.push_str("\n\n可选的引用来源：\n\n");

    for candidate in candidates {
        prompt.push_str(&format!(
            "[{}] {} - {}\n",
            candidate.citation_label, candidate.title, candidate.excerpt
        ));
    }

    prompt.push_str("\n请推荐最相关的引用，并说明理由。");
    prompt
}

// ─── Tests ───────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{SourceType, TrustLevel};

    #[test]
    fn build_system_prompt_includes_packets() {
        let packets = vec![ContextPacket {
            id: "pkt-1".into(),
            source_type: SourceType::Regulation,
            source_path: Some("regulations/discipline.md".into()),
            title: "纪律处分条例".into(),
            heading_path: Some("第三章 违纪行为".into()),
            source_span: None,
            content_hash: "abc123".into(),
            excerpt: "违反组织纪律的行为包括...".into(),
            retrieval_reason: "semantic".into(),
            score: 0.95,
            trust_level: TrustLevel::UserNote,
            citation_label: "[1]".into(),
            stale: false,
        }];

        let prompt = ModelGateway::build_system_prompt(AiScene::KnowledgeLookup, &packets, &[]);

        assert!(prompt.contains("知识管家"));
        assert!(prompt.contains("[1]"));
        assert!(prompt.contains("纪律处分条例"));
    }

    #[test]
    fn build_system_prompt_includes_user_rules() {
        let rules = vec![
            "引用法规时使用条/款格式".to_string(),
            "输出使用中文".to_string(),
        ];

        let prompt = ModelGateway::build_system_prompt(AiScene::DraftingAssist, &[], &rules);

        assert!(prompt.contains("写作伴侣"));
        assert!(prompt.contains("引用法规时使用条/款格式"));
    }

    #[test]
    fn slot_for_scene_mapping() {
        assert_eq!(
            ModelGateway::slot_for_scene(AiScene::KnowledgeLookup),
            CapabilitySlot::Fast
        );
        assert_eq!(
            ModelGateway::slot_for_scene(AiScene::DraftingAssist),
            CapabilitySlot::Writer
        );
        assert_eq!(
            ModelGateway::slot_for_scene(AiScene::ResearchSynthesis),
            CapabilitySlot::Reasoner
        );
    }

    #[test]
    fn tools_to_llm_format_conversion() {
        let tools = vec![ToolSpec {
            name: "search_hybrid".into(),
            description: "混合搜索".into(),
            input_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                }
            }),
            access_level: crate::ai_runtime::ToolAccessLevel::ReadIndex,
            scene_allowlist: vec![],
            requires_confirmation: false,
            max_results: Some(20),
        }];

        let llm_tools = ModelGateway::tools_to_llm_format(&tools);
        assert_eq!(llm_tools.len(), 1);
        assert_eq!(llm_tools[0].function.name, "search_hybrid");
    }
}
