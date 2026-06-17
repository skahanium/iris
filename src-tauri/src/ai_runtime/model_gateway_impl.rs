//! Model Gateway — LLM provider abstraction with streaming and tool-calling.
//!
//! Handles:
//! 1. Selecting provider/model by capability slot
//! 2. Building messages with system prompt + context packets
//! 3. Streaming responses via Tauri events
//! 4. Processing tool calls from LLM and routing to ToolExecutor

pub use crate::ai_types::{
    AiScene, CapabilitySlot, ContextPacket, EndpointFamily, FunctionCall, LlmMessage, MessageRole,
    ProviderConfig, TokenUsage, ToolCall, ToolSpec,
};
use crate::error::{AppError, AppResult};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tauri::AppHandle;

#[path = "model_gateway/abort.rs"]
mod abort_impl;
#[path = "model_gateway/body.rs"]
mod body_impl;
#[path = "model_gateway/http_backend.rs"]
mod http_backend_impl;
#[path = "model_gateway/messages.rs"]
mod messages_impl;
#[path = "model_gateway/prompts.rs"]
mod prompts_impl;
#[path = "model_gateway/streaming.rs"]
mod streaming_impl;
#[path = "model_gateway/usage.rs"]
mod usage_impl;

pub use abort_impl::{clear_abort, is_abort_requested, request_abort};
use body_impl::build_llm_api_body;
pub use body_impl::{build_chat_completions_body, GatewayRequest, LlmFunctionDef, LlmToolDef};
use http_backend_impl::format_llm_http_error;
pub use http_backend_impl::HttpLlmBackend;
pub use messages_impl::{
    insert_missing_tool_result_stubs, messages_for_api, prepare_tool_api_messages,
    remove_orphan_tool_messages, repair_tool_api_messages, tool_api_message_chain_valid,
};
use prompts_impl::is_rule_applicable_for_scene;
pub use prompts_impl::{build_citation_prompt, build_drafting_prompt};
pub use streaming_impl::{StreamEvent, StreamEventData, StreamEventType};
use usage_impl::parse_usage;

// Provider types (ProviderConfig, CapabilitySlot, MessageRole, LlmMessage,
// ToolCall, FunctionCall, TokenUsage) live in `crate::ai_types`.

/// Gateway response (non-streaming).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GatewayResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
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
        let client = crate::network::cert_pinning::create_https_client()?;
        Ok(Self::new(app_handle, client, providers))
    }

    /// Get provider for a capability slot.
    pub fn get_provider(&self, slot: CapabilitySlot) -> Option<&ProviderConfig> {
        self.providers.get(&slot)
    }

    /// Select appropriate capability slot for scene.
    pub fn slot_for_scene(scene: AiScene) -> CapabilitySlot {
        crate::ai_types::slot_for_scene(scene)
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
        Self::build_system_prompt_with_profile(
            scene,
            packets,
            user_rules,
            web_search_enabled,
            &crate::ai_runtime::prompt_profile::PromptProfile::default(),
        )
    }

    /// Build system prompt with an explicit user prompt profile.
    pub fn build_system_prompt_with_profile(
        scene: AiScene,
        packets: &[ContextPacket],
        user_rules: &[String],
        web_search_enabled: bool,
        profile: &crate::ai_runtime::prompt_profile::PromptProfile,
    ) -> String {
        let mut prompt = String::new();

        let resolved = crate::ai_runtime::persona_resolver::resolve_persona(
            profile,
            scene,
            web_search_enabled,
        );
        prompt.push_str(&crate::ai_runtime::persona_resolver::render_persona(
            &resolved,
        ));

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
                if let Some(corpus) = &packet.corpus {
                    prompt.push_str(&format!(
                        "语料角色: {}（{}）\n使用边界: {}\n",
                        corpus.label, corpus.name, corpus.instruction
                    ));
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
            content: persona.into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        }];

        if !user_rules.is_empty() {
            let mut rules = String::from("## 用户规则\n\n");
            for rule in user_rules {
                rules.push_str(&format!("- {rule}\n"));
            }
            messages.push(LlmMessage {
                role: MessageRole::System,
                content: rules.into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            });
        }

        if !packets.is_empty() {
            messages.push(LlmMessage {
                role: MessageRole::System,
                content: Self::format_evidence_packets(packets).into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            });
        }

        messages
    }

    /// Unified assistant persona with scene-specific capability focus.
    ///
    /// Delegates to `PersonaResolver` for persona resolution.
    /// When a user PromptProfile is available, prefer `persona_resolver::resolve_persona`
    /// directly. This method uses the default profile for backward compatibility.
    pub fn unified_persona(scene: AiScene, web_search_enabled: bool) -> String {
        use crate::ai_runtime::persona_resolver::{render_persona, resolve_persona};
        use crate::ai_runtime::prompt_profile::PromptProfile;
        let profile = PromptProfile::default();
        let resolved = resolve_persona(&profile, scene, web_search_enabled);
        render_persona(&resolved)
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
            if let Some(corpus) = &packet.corpus {
                evidence.push_str(&format!(
                    "语料角色: {}（{}）\n使用边界: {}\n",
                    corpus.label, corpus.name, corpus.instruction
                ));
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
        let url = llm_endpoint_url(&request.provider);

        let body = build_llm_api_body(&request)?;

        let mut req_builder = self
            .client
            .post(&url)
            .header("Content-Type", "application/json");

        if let Some(api_key) = &request.provider.api_key {
            req_builder =
                apply_auth_headers(req_builder, request.provider.endpoint_family, api_key);
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

        let response_text = response
            .text()
            .await
            .map_err(|e| AppError::msg(format!("Failed to read LLM response body: {}", e)))?;

        let json: serde_json::Value = serde_json::from_str(&response_text).map_err(|e| {
            let preview: String = response_text.chars().take(500).collect();
            let suffix = if response_text.chars().count() > 500 {
                "…"
            } else {
                ""
            };
            AppError::msg(format!(
                "Failed to parse LLM response: {}. Body preview: {}{}",
                e, preview, suffix
            ))
        })?;

        Ok(parse_gateway_response(
            request.provider.endpoint_family,
            &json,
        ))
    }

    /// Send a streaming request and emit events to frontend.
    pub async fn send_streaming_request(
        &self,
        request_id: &str,
        request: GatewayRequest,
    ) -> AppResult<GatewayResponse> {
        streaming_impl::send_streaming_request(&self.app_handle, &self.client, request_id, request)
            .await
    }
}

fn llm_endpoint_url(provider: &ProviderConfig) -> String {
    let base = provider.base_url.trim_end_matches('/');
    match provider.endpoint_family {
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            crate::llm::providers::chat_completions_url(&provider.base_url)
        }
        EndpointFamily::AnthropicMessages => {
            if base.ends_with("/v1") {
                format!("{base}/messages")
            } else {
                format!("{base}/v1/messages")
            }
        }
        EndpointFamily::OllamaChat => format!("{base}/api/chat"),
    }
}

fn apply_auth_headers(
    builder: reqwest::RequestBuilder,
    endpoint_family: EndpointFamily,
    api_key: &str,
) -> reqwest::RequestBuilder {
    match endpoint_family {
        EndpointFamily::AnthropicMessages => builder.header("x-api-key", api_key).header(
            "anthropic-version",
            crate::llm::providers::ANTHROPIC_API_VERSION,
        ),
        EndpointFamily::OllamaChat => builder.header("Authorization", format!("Bearer {api_key}")),
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            builder.header("Authorization", format!("Bearer {api_key}"))
        }
    }
}

fn parse_gateway_response(
    endpoint_family: EndpointFamily,
    json: &serde_json::Value,
) -> GatewayResponse {
    match endpoint_family {
        EndpointFamily::AnthropicMessages => parse_anthropic_response(json),
        EndpointFamily::OllamaChat => parse_ollama_response(json),
        EndpointFamily::OpenAiCompatibleChatCompletions | EndpointFamily::ResponsesReserved => {
            parse_openai_compatible_response(json)
        }
    }
}

fn parse_openai_compatible_response(json: &serde_json::Value) -> GatewayResponse {
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .map(|s| s.to_string());

    let reasoning_content = json["choices"][0]["message"]["reasoning_content"]
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

    GatewayResponse {
        content,
        tool_calls,
        usage: parse_usage(json),
        finish_reason: json["choices"][0]["finish_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        reasoning_content,
    }
}

fn parse_anthropic_response(json: &serde_json::Value) -> GatewayResponse {
    let content = json["content"]
        .as_array()
        .map(|parts| {
            parts
                .iter()
                .filter_map(|part| part["text"].as_str())
                .collect::<Vec<_>>()
                .join("")
        })
        .filter(|s| !s.is_empty());
    let usage = TokenUsage {
        prompt_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32,
        completion_tokens: json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        total_tokens: json["usage"]["input_tokens"].as_u64().unwrap_or(0) as u32
            + json["usage"]["output_tokens"].as_u64().unwrap_or(0) as u32,
        ..Default::default()
    };
    GatewayResponse {
        content,
        tool_calls: Vec::new(),
        usage,
        finish_reason: json["stop_reason"]
            .as_str()
            .unwrap_or("unknown")
            .to_string(),
        reasoning_content: None,
    }
}

fn parse_ollama_response(json: &serde_json::Value) -> GatewayResponse {
    GatewayResponse {
        content: json["message"]["content"].as_str().map(|s| s.to_string()),
        tool_calls: Vec::new(),
        usage: TokenUsage::default(),
        finish_reason: if json["done"].as_bool().unwrap_or(false) {
            "stop".into()
        } else {
            "unknown".into()
        },
        reasoning_content: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ai_runtime::{CorpusPacketMeta, SourceType, TrustLevel};

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
            corpus: None,
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
    fn format_evidence_packets_labels_lookup_role_as_non_authoritative() {
        let packets = vec![ContextPacket {
            id: "pkt-lookup".into(),
            source_type: SourceType::Note,
            source_path: Some("materials/temporary.md".into()),
            title: "临时资料".into(),
            heading_path: None,
            source_span: None,
            content_hash: "hash".into(),
            excerpt: "这是一段低权威查阅材料。".into(),
            retrieval_reason: "fts_keyword_match".into(),
            score: 0.8,
            trust_level: TrustLevel::UserNote,
            citation_label: "[1]".into(),
            stale: false,
            web: None,
            corpus: Some(CorpusPacketMeta {
                id: "lookup".into(),
                name: "查阅资料库".into(),
                kind: "lookup".into(),
                label: "查阅资料".into(),
                instruction: "可摘要其内容，但不能作为依据。".into(),
                can_be_authority: false,
            }),
        }];

        let evidence = ModelGateway::format_evidence_packets(&packets);

        assert!(evidence.contains("查阅资料"));
        assert!(evidence.contains("不能作为依据"));
    }

    #[test]
    fn messages_for_api_includes_reasoning_content_with_tool_calls() {
        let messages = vec![LlmMessage {
            role: MessageRole::Assistant,
            content: String::new().into(),
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall::new(
                "call_1",
                "fetch_web_page",
                r#"{"url":"https://example.com"}"#,
            )]),
            reasoning_content: Some("internal chain of thought".into()),
        }];
        let api = messages_for_api(&messages);
        assert_eq!(api[0]["reasoning_content"], "internal chain of thought");
        assert_eq!(api[0]["tool_calls"][0]["type"], "function");
    }

    #[test]
    fn resume_after_tool_confirm_body_preserves_reasoning_and_thinking() {
        use crate::ai_types::CapabilitySlot;

        let provider = ProviderConfig {
            name: "deepseek".into(),
            base_url: "https://api.deepseek.com".into(),
            model: "deepseek-reasoner".into(),
            api_key: Some("test".into()),
            slot: CapabilitySlot::Reasoner,
            endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        };
        let messages = vec![
            LlmMessage {
                role: MessageRole::Assistant,
                content: String::new().into(),
                tool_call_id: None,
                tool_calls: Some(vec![ToolCall::new(
                    "call_1",
                    "fetch_web_page",
                    r#"{"url":"https://example.com"}"#,
                )]),
                reasoning_content: Some("internal chain of thought".into()),
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"title":"Example"}"#.into(),
                tool_call_id: Some("call_1".into()),
                tool_calls: None,
                ..Default::default()
            },
        ];
        let body = build_chat_completions_body(&GatewayRequest {
            provider,
            messages,
            tools: vec![],
            max_tokens: Some(1024),
            temperature: Some(0.7),
            stream: false,
            thinking: true,
            skip_stub_ids: vec![],
        });
        assert_eq!(
            body["messages"][0]["reasoning_content"],
            "internal chain of thought"
        );
        assert_eq!(body["thinking"]["type"], "enabled");
        assert_eq!(body["messages"][1]["role"], "tool");
    }

    #[test]
    fn messages_for_api_includes_tool_call_type() {
        let messages = vec![
            LlmMessage {
                role: MessageRole::User,
                content: "查一下".into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Assistant,
                content: String::new().into(),
                tool_call_id: None,
                tool_calls: Some(vec![ToolCall::new(
                    "call_1",
                    "search_hybrid",
                    r#"{"query":"x"}"#,
                )]),
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"ok":true}"#.into(),
                tool_call_id: Some("call_1".into()),
                tool_calls: None,
                ..Default::default()
            },
        ];
        let api = messages_for_api(&messages);
        assert_eq!(api[1]["tool_calls"][0]["type"], "function");
        assert!(api[1]["content"].is_null());
        assert_eq!(api[2]["role"], "tool");
        assert_eq!(api[2]["tool_call_id"], "call_1");
    }

    #[test]
    fn prepare_tool_api_messages_completes_mixed_auto_and_confirm_batch() {
        let mut messages = vec![
            LlmMessage {
                role: MessageRole::Assistant,
                content: "searching".into(),
                tool_call_id: None,
                tool_calls: Some(vec![
                    ToolCall::new("call_search", "web_search", r#"{"query":"x"}"#),
                    ToolCall::new(
                        "call_fetch",
                        "fetch_web_page",
                        r#"{"url":"https://example.com"}"#,
                    ),
                ]),
                reasoning_content: None,
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"results":[]}"#.into(),
                tool_call_id: Some("call_search".into()),
                tool_calls: None,
                reasoning_content: None,
            },
        ];
        prepare_tool_api_messages(&mut messages, &["call_fetch".into()]);
        assert_eq!(messages.len(), 2);
        let api = messages_for_api(&messages);
        assert_eq!(api.len(), 2);
        assert_eq!(api[1]["role"], "tool");
    }

    #[test]
    fn remove_orphan_tool_messages_drops_invalid_history_rows() {
        let mut messages = vec![
            LlmMessage {
                role: MessageRole::User,
                content: "hi".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"x":1}"#.into(),
                tool_call_id: Some("orphan".into()),
                tool_calls: None,
                reasoning_content: None,
            },
        ];
        remove_orphan_tool_messages(&mut messages);
        assert_eq!(messages.len(), 1);
    }

    #[test]
    fn repair_tool_api_messages_restores_missing_tool_calls() {
        let mut messages = vec![
            LlmMessage {
                role: MessageRole::Assistant,
                content: "partial".into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"ok":true}"#.into(),
                tool_call_id: Some("call_1".into()),
                tool_calls: None,
                ..Default::default()
            },
        ];
        repair_tool_api_messages(&mut messages);
        let api = messages_for_api(&messages);
        assert!(api[0]["tool_calls"].is_array());
        assert_eq!(api[0]["tool_calls"][0]["id"], "call_1");
        assert_eq!(api[1]["role"], "tool");
    }

    #[test]
    fn prepare_repairs_legacy_assistant_before_orphan_cleanup() {
        let mut messages = vec![
            LlmMessage {
                role: MessageRole::Assistant,
                content: "partial".into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"title":"Page"}"#.into(),
                tool_call_id: Some("call_fetch".into()),
                tool_calls: None,
                ..Default::default()
            },
        ];
        prepare_tool_api_messages(&mut messages, &[]);
        assert_eq!(messages.len(), 2);
        assert!(tool_api_message_chain_valid(&messages));
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
            requires_confirmation: false,
            max_results: Some(20),
            scene_affinity: vec![],
        }];

        let llm_tools = ModelGateway::tools_to_llm_format(&tools);
        assert_eq!(llm_tools.len(), 1);
        assert_eq!(llm_tools[0].function.name, "search_hybrid");
    }
}
