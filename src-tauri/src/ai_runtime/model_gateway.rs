//! Model Gateway — LLM provider abstraction with streaming and tool-calling.
//!
//! Handles:
//! 1. Selecting provider/model by capability slot
//! 2. Building messages with system prompt + context packets
//! 3. Streaming responses via Tauri events
//! 4. Processing tool calls from LLM and routing to ToolExecutor

use crate::ai_runtime::{AiScene, ContextPacket, ToolSpec};
use crate::error::{AppError, AppResult};
use reqwest::Client;
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

/// 能力槽位，用于 provider/model 选择。
///
/// 每个场景映射到一个槽位，每个槽位可配置不同的 provider。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySlot {
    /// 快速响应（知识查阅等简单任务）
    Fast,
    /// 文稿写作（范文学习、写作辅助）
    Writer,
    /// 深度推理（学术研究、多材料论证）
    Reasoner,
    /// 长上下文（大文档处理）
    LongContext,
    /// 文本嵌入（向量检索）
    Embedding,
    /// 重排序（检索结果重排）
    Reranker,
    /// 本地私有模型（离线场景）
    LocalPrivate,
}

/// LLM 对话消息角色。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    /// 系统提示（persona / rules / evidence）
    System,
    /// 用户消息
    User,
    /// 助手回复
    Assistant,
    /// 工具调用结果
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

/// Tool call from LLM (OpenAI / DeepSeek chat completions format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_call_type")]
    pub call_type: String,
    pub function: FunctionCall,
}

fn default_tool_call_type() -> String {
    "function".into()
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            call_type: default_tool_call_type(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }
}

/// Serialize messages for provider APIs (tool_calls need `type`, tool role needs `tool_call_id`).
pub fn messages_for_api(messages: &[LlmMessage]) -> Vec<serde_json::Value> {
    messages
        .iter()
        .map(|m| {
            let role = match m.role {
                MessageRole::System => "system",
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::Tool => "tool",
            };
            if matches!(m.role, MessageRole::Tool) {
                return serde_json::json!({
                    "role": "tool",
                    "tool_call_id": m.tool_call_id,
                    "content": m.content,
                });
            }
            if matches!(m.role, MessageRole::Assistant)
                && m.tool_calls.as_ref().is_some_and(|tc| !tc.is_empty())
            {
                let content: serde_json::Value = if m.content.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::String(m.content.clone())
                };
                return serde_json::json!({
                    "role": "assistant",
                    "content": content,
                    "tool_calls": m.tool_calls,
                });
            }
            serde_json::json!({
                "role": role,
                "content": m.content,
            })
        })
        .collect()
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
    #[serde(default)]
    pub prompt_cache_hit_tokens: u32,
    #[serde(default)]
    pub prompt_cache_miss_tokens: u32,
}

fn parse_usage(json: &serde_json::Value) -> TokenUsage {
    let usage = &json["usage"];
    TokenUsage {
        prompt_tokens: usage["prompt_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: usage["completion_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: usage["total_tokens"].as_u64().unwrap_or(0) as u32,
        prompt_cache_hit_tokens: usage["prompt_cache_hit_tokens"].as_u64().unwrap_or(0) as u32,
        prompt_cache_miss_tokens: usage["prompt_cache_miss_tokens"].as_u64().unwrap_or(0) as u32,
    }
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
    client: Client,
    providers: HashMap<CapabilitySlot, ProviderConfig>,
}

impl ModelGateway {
    /// Create a new gateway with injected HTTP client and provider configurations.
    pub fn new(app_handle: AppHandle, client: Client, providers: Vec<ProviderConfig>) -> Self {
        let mut provider_map = HashMap::new();
        for p in providers {
            provider_map.insert(p.slot, p);
        }
        Self {
            app_handle,
            client,
            providers: provider_map,
        }
    }

    /// Create a gateway with default pinned HTTP client.
    pub fn with_defaults(app_handle: AppHandle, providers: Vec<ProviderConfig>) -> AppResult<Self> {
        let client = crate::network::cert_pinning::create_pinned_client()?;
        Ok(Self::new(app_handle, client, providers))
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

    /// Load active user rules from the DB, filtered by scene relevance.
    ///
    /// Rules are only injected for the scenes where they apply:
    /// - `writing_style`, `citation_habits` → DraftingAssist, ExemplarLearning
    /// - `citation_habits` (also) → KnowledgeLookup, ResearchSynthesis
    /// - `tool_preferences`, `model_preferences`, `custom_rules`, `agent_behavior` → All scenes
    pub fn load_active_rules_for_scene(
        db: &crate::storage::db::Database,
        scene: AiScene,
    ) -> crate::error::AppResult<Vec<String>> {
        let mut rules = Vec::new();

        db.with_conn(|conn| {
            let mut stmt = conn
                .prepare("SELECT key, value FROM user_profile WHERE is_active = 1 ORDER BY key")?;

            let rows = stmt.query_map([], |row| {
                let key: String = row.get(0)?;
                let json_str: String = row.get(1)?;
                Ok((key, json_str))
            })?;

            for row in rows {
                let (key, json_str) = row?;
                if !is_rule_applicable_for_scene(&key, scene) {
                    continue;
                }

                // Extract human-readable rule text from JSON value
                if let Ok(value) = serde_json::from_str::<serde_json::Value>(&json_str) {
                    let rule_text = match &value {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Object(obj) => {
                            if let Some(desc) = obj.get("description").and_then(|v| v.as_str()) {
                                desc.to_string()
                            } else {
                                format!("{key}: {value}")
                            }
                        }
                        other => format!("{key}: {other}"),
                    };
                    if !rule_text.is_empty() {
                        rules.push(rule_text);
                    }
                }
            }

            Ok(())
        })?;

        Ok(rules)
    }

    /// Build system prompt for a scene with context packets.
    pub fn build_system_prompt(
        scene: AiScene,
        packets: &[ContextPacket],
        user_rules: &[String],
        web_search_enabled: bool,
    ) -> String {
        let mut prompt = String::new();

        prompt.push_str(&Self::unified_persona(scene, web_search_enabled));

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

    /// Stable prefix messages for cache-friendly layouts (persona + rules + evidence).
    pub fn build_stable_prefix(
        scene: AiScene,
        packets: &[ContextPacket],
        user_rules: &[String],
        web_search: bool,
    ) -> Vec<LlmMessage> {
        let persona = Self::unified_persona(scene, web_search);
        let mut messages = vec![LlmMessage {
            role: MessageRole::System,
            content: persona,
            tool_call_id: None,
            tool_calls: None,
        }];

        if !user_rules.is_empty() {
            let mut rules = String::from("## 用户规则\n\n");
            for rule in user_rules {
                rules.push_str(&format!("- {rule}\n"));
            }
            messages.push(LlmMessage {
                role: MessageRole::System,
                content: rules,
                tool_call_id: None,
                tool_calls: None,
            });
        }

        if !packets.is_empty() {
            messages.push(LlmMessage {
                role: MessageRole::System,
                content: Self::format_evidence_packets(packets),
                tool_call_id: None,
                tool_calls: None,
            });
        }

        messages
    }

    /// Unified assistant persona「砚」with scene-specific capability focus.
    pub fn unified_persona(scene: AiScene, web_search_enabled: bool) -> String {
        let focus = match scene {
            AiScene::KnowledgeLookup => {
                if web_search_enabled {
                    "知识查阅：先 search_hybrid 检索本地笔记，再 web_search（Token Plan 搜索）补充摘要；\
                     若摘要不足可对 1～2 个 HTTPS 链接调用 fetch_web_page 读取正文（需用户确认）；\
                     本地与网络证据结合、交叉引用，不可偏废"
                } else {
                    "知识查阅：通过 search_hybrid 检索本地笔记；仅依据本地知识库回答"
                }
            }
            AiScene::ExemplarLearning => "范文学习：分析结构、句式与表达；模板保存需用户确认",
            AiScene::DraftingAssist => "文稿创作：低干扰辅助；写入笔记须用户确认；避免大段照搬范文",
            AiScene::ResearchSynthesis => "研究综合：多材料交叉论证、证据缺口与引用核查",
        };
        let web_instruction = if web_search_enabled {
            "联网已开启：web_search 使用 MiniMax Token Plan 搜索 API（返回标题/链接/摘要），\
             无需询问是否允许搜索。\n\
             需要页面正文时对明确 URL 调用 fetch_web_page（会弹出用户确认，每轮最多 1～2 次）。\n\
             禁止在正文中输出 DSML 或伪工具标记；必须通过工具 API 调用。\n\
             本地检索与网络搜索应结合、相互印证，不可只做其一。\n"
        } else {
            "联网未开启——仅使用本地知识库，不要调用 web_search 或 fetch_web_page。\n"
        };
        format!(
            "你是「砚」，Iris 本地 Markdown 笔记本的 AI 助手。对用户你始终是同一个身份：\
             语气克制、清晰、可追溯。\n\
             Iris 以用户的 .md 为唯一数据源；通过工具检索知识库、读取笔记。\
             在用户确认后修改文稿。\n\
             {web_instruction}\
             当前侧重：{focus}。\n\
             回答须基于工具结果与证据；引用时请使用证据包中提供的标签（如 [C1]、[W0]），\
             也可直接指明来源文件名或 URL；证据不足时直接说明，不编造。\n"
        )
    }

    /// Format context packets as markdown evidence block.
    pub fn format_evidence_packets(packets: &[ContextPacket]) -> String {
        let mut evidence = String::from("## 本地证据包\n\n");
        evidence.push_str("以下是从你的笔记中检索到的材料，请在回答中引用（使用 [标签] 格式），并结合网络搜索结果交叉验证：\n\n");
        for packet in packets {
            evidence.push_str(&format!(
                "### {} ({})\n",
                packet.citation_label, packet.title
            ));
            if let Some(path) = &packet.source_path {
                evidence.push_str(&format!("来源: {path}\n"));
            }
            if let Some(heading) = &packet.heading_path {
                evidence.push_str(&format!("章节: {heading}\n"));
            }
            evidence.push_str(&format!("相关度: {:.0}%\n", packet.score * 100.0));
            evidence.push_str(&format!("{}\n\n", packet.excerpt));
        }
        evidence
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
        let url = crate::llm::providers::chat_completions_url(&request.provider.base_url);

        let mut body = serde_json::json!({
            "model": request.provider.model,
            "messages": messages_for_api(&request.messages),
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

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

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
            return Err(AppError::msg(format_llm_http_error(status, &text)));
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
                            call_type: tc["type"].as_str().unwrap_or("function").to_string(),
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

        let usage = parse_usage(&json);

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
        let url = crate::llm::providers::chat_completions_url(&request.provider.base_url);

        let mut body = serde_json::json!({
            "model": request.provider.model,
            "messages": messages_for_api(&request.messages),
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

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

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
            return Err(AppError::msg(format_llm_http_error(status, &text)));
        }

        let mut full_content = String::new();
        let mut usage = TokenUsage::default();
        let mut token_index: u32 = 0;

        // Incremental tool call accumulator: index -> (id, name, args_buf).
        // OpenAI streams tool calls as deltas: id+name arrive first, then
        // argument fragments across multiple subsequent deltas.
        let mut tool_call_deltas: std::collections::HashMap<
            usize,
            (Option<String>, Option<String>, String),
        > = std::collections::HashMap::new();

        // Process SSE stream with carry buffer to handle chunks split across TCP boundaries
        let mut stream = response.bytes_stream();
        use futures_util::StreamExt;
        let mut carry = String::new();

        while let Some(chunk_result) = stream.next().await {
            let chunk =
                chunk_result.map_err(|e| AppError::msg(format!("Stream read error: {}", e)))?;

            carry.push_str(&String::from_utf8_lossy(&chunk));

            while let Some(pos) = carry.find('\n') {
                let line: String = carry.drain(..=pos).collect();
                let line = line.trim_end_matches('\n').trim_end_matches('\r');

                if !line.starts_with("data: ") {
                    continue;
                }

                let data = &line[6..];
                if data == "[DONE]" {
                    let event = StreamEvent {
                        request_id: request_id.to_string(),
                        event_type: StreamEventType::Done,
                        data: StreamEventData::Done {
                            usage: Some(usage.clone()),
                        },
                    };
                    self.emit_stream_event(&event, token_index)?;
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
                        self.emit_stream_event(&event, token_index)?;
                        token_index += 1;
                    }

                    // Accumulate tool call deltas by index
                    if let Some(tc_deltas) = json["choices"][0]["delta"]["tool_calls"].as_array() {
                        for tc_delta in tc_deltas {
                            let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                            let entry =
                                tool_call_deltas
                                    .entry(idx)
                                    .or_insert((None, None, String::new()));

                            if let Some(id) = tc_delta["id"].as_str() {
                                entry.0 = Some(id.to_string());
                            }
                            if let Some(name) = tc_delta["function"]["name"].as_str() {
                                entry.1 = Some(name.to_string());
                            }
                            if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                                entry.2.push_str(args);
                            }
                        }
                    }

                    if json.get("usage").is_some() && json["usage"].get("prompt_tokens").is_some() {
                        usage = parse_usage(&json);
                    }
                }
            }
        }

        // Flush remaining carry buffer
        if !carry.trim().is_empty() {
            if let Some(pos) = carry.find("data: ") {
                let remainder = &carry[pos..];
                if let Some(data) = remainder.strip_prefix("data: ") {
                    let data = data.trim();
                    if data != "[DONE]" {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(data) {
                            if let Some(delta) = json["choices"][0]["delta"]["content"].as_str() {
                                full_content.push_str(delta);
                            }
                            if let Some(tc_deltas) =
                                json["choices"][0]["delta"]["tool_calls"].as_array()
                            {
                                for tc_delta in tc_deltas {
                                    let idx = tc_delta["index"].as_u64().unwrap_or(0) as usize;
                                    let entry = tool_call_deltas.entry(idx).or_insert((
                                        None,
                                        None,
                                        String::new(),
                                    ));
                                    if let Some(id) = tc_delta["id"].as_str() {
                                        entry.0 = Some(id.to_string());
                                    }
                                    if let Some(name) = tc_delta["function"]["name"].as_str() {
                                        entry.1 = Some(name.to_string());
                                    }
                                    if let Some(args) = tc_delta["function"]["arguments"].as_str() {
                                        entry.2.push_str(args);
                                    }
                                }
                            }
                            if json.get("usage").is_some()
                                && json["usage"].get("prompt_tokens").is_some()
                            {
                                usage = parse_usage(&json);
                            }
                        }
                    }
                }
            }
        }

        // Assemble tool calls from accumulated deltas (deduplicated by index)
        let tool_calls: Vec<ToolCall> = tool_call_deltas
            .into_iter()
            .filter_map(|(_, (id, name, args))| {
                Some(ToolCall {
                    id: id?,
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: name?,
                        arguments: args,
                    },
                })
            })
            .collect();

        // Emit tool call events for each assembled call
        for tc in &tool_calls {
            let event = StreamEvent {
                request_id: request_id.to_string(),
                event_type: StreamEventType::ToolCall,
                data: StreamEventData::ToolCall {
                    tool_call: tc.clone(),
                },
            };
            self.emit_stream_event(&event, token_index)?;
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

    /// Emit a stream event to the frontend (`llm:*` 与 `engine.rs` / 侧栏监听一致).
    fn emit_stream_event(&self, event: &StreamEvent, token_index: u32) -> AppResult<()> {
        let emit_err = |e: tauri::Error| AppError::msg(format!("Failed to emit stream event: {e}"));
        match event.event_type {
            StreamEventType::Token => {
                if let StreamEventData::Token { token } = &event.data {
                    self.app_handle
                        .emit(
                            "llm:token",
                            serde_json::json!({
                                "request_id": event.request_id,
                                "token": token,
                                "index": token_index,
                            }),
                        )
                        .map_err(emit_err)?;
                }
            }
            StreamEventType::Done => {
                self.app_handle
                    .emit(
                        "llm:done",
                        serde_json::json!({ "request_id": event.request_id }),
                    )
                    .map_err(emit_err)?;
            }
            StreamEventType::Error => {
                let message = if let StreamEventData::Error { message } = &event.data {
                    message.clone()
                } else {
                    "stream error".to_string()
                };
                self.app_handle
                    .emit(
                        "llm:error",
                        serde_json::json!({
                            "request_id": event.request_id,
                            "error": message,
                        }),
                    )
                    .map_err(emit_err)?;
            }
            StreamEventType::ToolCall => {
                self.app_handle
                    .emit("ai:tool_call", event)
                    .map_err(emit_err)?;
            }
        }
        Ok(())
    }
}

// ─── Prompt Builder ──────────────────────────────────────

/// 构建写作场景的上下文感知 prompt。
///
/// 将文稿大纲、光标上下文、证据包和写作规则组装为结构化 prompt。
///
/// # Arguments
///
/// - `document_outline` — 当前文稿的大纲结构
/// - `cursor_context` — 光标邻域的文本上下文
/// - `packets` — 参考材料证据包
/// - `user_rules` — 用户自定义写作规则
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

/// 构建引用建议 prompt，分析段落并推荐合适的法规引用。
///
/// # Arguments
///
/// - `paragraph` — 待分析的段落文本
/// - `candidates` — 候选引用的证据包列表
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

// ─── Rule Scene Mapping ─────────────────────────────────

/// Determine whether a user profile rule applies to a given AI scene.
///
/// Scoped rules (writing_style, citation_habits) only apply to relevant scenes.
/// Global rules (custom_rules, tool_preferences, etc.) apply everywhere.
fn is_rule_applicable_for_scene(key: &str, scene: AiScene) -> bool {
    match key {
        "writing_style" => {
            matches!(scene, AiScene::DraftingAssist | AiScene::ExemplarLearning)
        }
        "citation_habits" => {
            matches!(
                scene,
                AiScene::DraftingAssist | AiScene::ResearchSynthesis | AiScene::KnowledgeLookup
            )
        }
        "tool_preferences" | "model_preferences" | "custom_rules" | "agent_behavior" => true,
        _ => {
            // Unknown keys: conservative approach — don't inject into knowledge-lookup
            // to avoid polluting retrieval with irrelevant preferences
            !matches!(scene, AiScene::KnowledgeLookup)
        }
    }
}

// ─── Tests ───────────────────────────────────────────────

fn format_llm_http_error(status: reqwest::StatusCode, text: &str) -> String {
    let lower = text.to_lowercase();
    if status == reqwest::StatusCode::SERVICE_UNAVAILABLE
        || lower.contains("service_unavailable")
        || lower.contains("too busy")
        || lower.contains("overloaded")
    {
        return "模型服务繁忙，请稍后重试或在设置中更换模型。".into();
    }
    if status == reqwest::StatusCode::TOO_MANY_REQUESTS || lower.contains("rate limit") {
        return "请求过于频繁，请稍后再试。".into();
    }
    if status == reqwest::StatusCode::UNAUTHORIZED || lower.contains("invalid_api_key") {
        return "API Key 无效或未配置，请在设置中检查。".into();
    }
    if text.len() > 200 {
        format!("模型请求失败（{}）", status)
    } else {
        format!("模型请求失败（{}）：{}", status, text)
    }
}

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
            web: None,
        }];

        let prompt =
            ModelGateway::build_system_prompt(AiScene::KnowledgeLookup, &packets, &[], false);

        assert!(prompt.contains("砚"));
        assert!(prompt.contains("知识查阅"));
        assert!(prompt.contains("[1]"));
        assert!(prompt.contains("纪律处分条例"));
    }

    #[test]
    fn build_system_prompt_includes_user_rules() {
        let rules = vec![
            "引用法规时使用条/款格式".to_string(),
            "输出使用中文".to_string(),
        ];

        let prompt = ModelGateway::build_system_prompt(AiScene::DraftingAssist, &[], &rules, false);

        assert!(prompt.contains("砚"));
        assert!(prompt.contains("文稿创作"));
        assert!(prompt.contains("引用法规时使用条/款格式"));
    }

    #[test]
    fn messages_for_api_includes_tool_call_type() {
        let messages = vec![
            LlmMessage {
                role: MessageRole::User,
                content: "查一下".into(),
                tool_call_id: None,
                tool_calls: None,
            },
            LlmMessage {
                role: MessageRole::Assistant,
                content: String::new(),
                tool_call_id: None,
                tool_calls: Some(vec![ToolCall::new(
                    "call_1",
                    "search_hybrid",
                    r#"{"query":"x"}"#,
                )]),
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"ok":true}"#.into(),
                tool_call_id: Some("call_1".into()),
                tool_calls: None,
            },
        ];
        let api = messages_for_api(&messages);
        assert_eq!(api[1]["tool_calls"][0]["type"], "function");
        assert!(api[1]["content"].is_null());
        assert_eq!(api[2]["role"], "tool");
        assert_eq!(api[2]["tool_call_id"], "call_1");
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
    fn format_busy_service_error() {
        let body =
            r#"{"error":{"type":"service_unavailable_error","message":"Service is too busy"}}"#;
        let msg = super::format_llm_http_error(reqwest::StatusCode::SERVICE_UNAVAILABLE, body);
        assert!(msg.contains("繁忙"));
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
