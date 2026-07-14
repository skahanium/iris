use super::*;
use crate::ai_runtime::{CorpusPacketMeta, SourceType, TrustLevel};

fn test_provider(name: &str) -> ProviderConfig {
    ProviderConfig {
        name: name.into(),
        base_url: format!("https://{name}.example/v1"),
        api_key: Some(zeroize::Zeroizing::new("test".to_string())),
        model: format!("{name}-model"),
        endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
    }
}

#[test]
fn invalid_provider_json_uses_safe_error_code_without_response_preview() {
    let malformed = "{ user note content must never surface";
    let error = super::parse_gateway_json(malformed).expect_err("malformed response should fail");

    assert_eq!(error.to_string(), "llm_response_invalid_json");
    assert!(!error.to_string().contains("user note content"));
}

#[test]
fn run_owned_observer_streams_are_always_sanitized_before_becoming_visible() {
    assert_eq!(
        super::run_observer_stream_surface(),
        StreamSurface::VisibleAnswerSanitized
    );
}

#[test]
fn failover_selects_next_model_pool_candidate_for_provider_level_failure() {
    let primary = test_provider("primary");
    let backup = test_provider("backup");
    let unrelated = test_provider("unrelated");
    let selected = select_failover_provider(
        &[primary.clone(), backup.clone(), unrelated],
        &primary,
        "LLM streaming request failed: connection reset by peer",
    )
    .unwrap();

    assert_eq!(selected.name, backup.name);
}

#[test]
fn failover_rejects_auth_context_and_user_abort_errors() {
    let primary = test_provider("primary");
    let backup = test_provider("backup");

    for message in [
        "invalid_api_key: check API key",
        "401 unauthorized",
        "context length exceeded",
        "request aborted by user",
        "partial_visible_stream_error: after visible content",
    ] {
        assert!(
            select_failover_provider(&[primary.clone(), backup.clone()], &primary, message)
                .is_none(),
            "unexpected failover for {message}"
        );
    }
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
            "web_search",
            r#"{"query":"https://example.com"}"#,
        )]),
        reasoning_content: Some("internal chain of thought".into()),
    }];
    let api = messages_for_api(&messages);
    assert_eq!(api[0]["reasoning_content"], "internal chain of thought");
    assert_eq!(api[0]["tool_calls"][0]["type"], "function");
}

#[test]
fn resume_after_tool_confirm_body_preserves_reasoning_and_thinking() {
    let provider = ProviderConfig {
        name: "deepseek".into(),
        base_url: "https://api.deepseek.com".into(),
        model: "deepseek-reasoner".into(),
        api_key: Some(zeroize::Zeroizing::new("test".to_string())),
        endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
    };
    let messages = vec![
        LlmMessage {
            role: MessageRole::Assistant,
            content: String::new().into(),
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall::new(
                "call_1",
                "web_search",
                r#"{"query":"https://example.com"}"#,
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
        input_token_budget: None,
        temperature: Some(0.7),
        stream: false,
        thinking: true,
        reasoning: crate::ai_types::ResolvedReasoningRequest::legacy_enabled(true),
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
                    "web_search",
                    r#"{"query":"https://example.com"}"#,
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
fn format_busy_service_error() {
    let body = r#"{"error":{"type":"service_unavailable_error","message":"Service is too busy"}}"#;
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
        capability_affinity: vec![],
    }];

    let llm_tools = ModelGateway::tools_to_llm_format(&tools);
    assert_eq!(llm_tools.len(), 1);
    assert_eq!(llm_tools[0].function.name, "search_hybrid");
}
