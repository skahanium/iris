//! Integration-style tests for tool confirm → checkpoint → resume metadata.

#[cfg(test)]
mod tests {
    use crate::ai_runtime::harness::UsageSource;
    use crate::ai_runtime::harness_confirm::append_rejected_tool_to_checkpoint;
    use crate::ai_runtime::harness_support::{
        load_harness_checkpoint, save_harness_checkpoint, HarnessCheckpoint, HarnessCheckpointMeta,
    };
    use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, TokenUsage, ToolCall};
    use crate::ai_runtime::trace::{TraceRecorder, TraceStatus};
    use crate::ai_runtime::AiScene;
    use crate::app::AppState;
    use crate::storage::db::Database;
    use std::sync::Arc;

    fn test_state() -> Arc<AppState> {
        let dir = tempfile::tempdir().unwrap();
        AppState::new(dir.path().to_path_buf()).unwrap()
    }

    fn sample_checkpoint(_request_id: &str) -> HarnessCheckpoint {
        HarnessCheckpoint {
            meta: HarnessCheckpointMeta {
                scene: "knowledge_lookup".into(),
                session_id: 1,
                note_path: None,
                note_title: None,
                selection_excerpt: None,
                cold_start_packets: vec![],
                web_search_enabled: false,
                depth: 0,
            },
            round: 1,
            messages: vec![LlmMessage {
                role: MessageRole::Assistant,
                content: "partial".into(),
                tool_call_id: None,
                tool_calls: None,
                ..Default::default()
            }],
            tool_calls: vec![],
            tool_results: vec![serde_json::json!({
                "tool_call_id": "tc1",
                "status": "pending_confirmation",
            })],
            evidence_packets: vec![],
            usage: TokenUsage::default(),
            usage_source: UsageSource::Provider,
            bonus_round_used: false,
        }
    }

    #[test]
    fn pending_trace_keeps_checkpoint() {
        let db = Database::open_in_memory().unwrap();
        let rid = "pending-trace-1";
        TraceRecorder::start(&db, rid, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&db, rid, TraceStatus::AwaitingToolConfirmation).unwrap();
        save_harness_checkpoint(&db, rid, &sample_checkpoint(rid)).unwrap();
        let loaded = load_harness_checkpoint(&db, rid).unwrap();
        assert!(loaded.is_some());
    }

    #[test]
    fn reject_appends_tool_message_to_checkpoint() {
        let state = test_state();
        let rid = "reject-cp-1";
        TraceRecorder::start(&state.db, rid, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&state.db, rid, TraceStatus::AwaitingToolConfirmation)
            .unwrap();
        save_harness_checkpoint(&state.db, rid, &sample_checkpoint(rid)).unwrap();

        append_rejected_tool_to_checkpoint(state.as_ref(), rid, "tc1").unwrap();

        let cp = load_harness_checkpoint(&state.db, rid).unwrap().unwrap();
        let api = crate::ai_runtime::model_gateway::messages_for_api(&cp.messages);
        assert!(api[0]["tool_calls"].is_array());
        assert_eq!(api[0]["tool_calls"][0]["id"], "tc1");
        assert_eq!(cp.messages.len(), 2);
        assert!(matches!(cp.messages[1].role, MessageRole::Tool));
        assert!(cp.messages[1].content.contains("rejected"));
        assert!(cp
            .tool_results
            .iter()
            .any(|r| r.get("status").and_then(|s| s.as_str()) == Some("rejected")));
    }

    #[test]
    fn reject_records_sanitized_tool_audit() {
        let state = test_state();
        let rid = "reject-audit-1";
        TraceRecorder::start(&state.db, rid, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&state.db, rid, TraceStatus::AwaitingToolConfirmation)
            .unwrap();
        save_harness_checkpoint(&state.db, rid, &sample_checkpoint(rid)).unwrap();

        append_rejected_tool_to_checkpoint(state.as_ref(), rid, "tc1").unwrap();

        let entries = crate::ai_runtime::tool_audit::query_by_request(&state.db, rid).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].tool_name, "tool_confirmation");
        assert!(!entries[0].success);
        assert!(entries[0]
            .result_summary
            .as_deref()
            .unwrap_or("")
            .contains("rejected"));
    }

    #[test]
    fn checkpoint_round_trip_preserves_reasoning_content() {
        let db = Database::open_in_memory().unwrap();
        let rid = "reasoning-cp-1";
        TraceRecorder::start(&db, rid, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&db, rid, TraceStatus::AwaitingToolConfirmation).unwrap();
        let mut cp = sample_checkpoint(rid);
        cp.messages = vec![LlmMessage {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall::new(
                "call_1",
                "fetch_web_page",
                r#"{"url":"https://example.com"}"#,
            )]),
            reasoning_content: Some("internal chain of thought".into()),
        }];
        save_harness_checkpoint(&db, rid, &cp).unwrap();
        let loaded = load_harness_checkpoint(&db, rid).unwrap().unwrap();
        assert_eq!(
            loaded.messages[0].reasoning_content.as_deref(),
            Some("internal chain of thought")
        );
        let api = crate::ai_runtime::model_gateway::messages_for_api(&loaded.messages);
        assert_eq!(api[0]["reasoning_content"], "internal chain of thought");
    }

    #[test]
    fn tool_confirm_resume_api_body_includes_reasoning_after_tool_result() {
        use crate::ai_runtime::harness_confirm::append_tool_message_to_checkpoint;
        use crate::ai_runtime::model_gateway::{
            build_chat_completions_body, GatewayRequest, ProviderConfig,
        };
        use crate::ai_types::CapabilitySlot;

        let db = Database::open_in_memory().unwrap();
        let rid = "reasoning-resume-1";
        TraceRecorder::start(&db, rid, AiScene::KnowledgeLookup).unwrap();
        TraceRecorder::update_status(&db, rid, TraceStatus::AwaitingToolConfirmation).unwrap();
        let mut cp = sample_checkpoint(rid);
        cp.messages = vec![LlmMessage {
            role: MessageRole::Assistant,
            content: String::new(),
            tool_call_id: None,
            tool_calls: Some(vec![ToolCall::new(
                "call_1",
                "fetch_web_page",
                r#"{"url":"https://example.com"}"#,
            )]),
            reasoning_content: Some("internal chain of thought".into()),
        }];
        save_harness_checkpoint(&db, rid, &cp).unwrap();

        append_tool_message_to_checkpoint(
            &db,
            rid,
            "call_1",
            r#"{"title":"Example"}"#.into(),
            "completed",
            None,
            None,
            None,
        )
        .unwrap();

        let loaded = load_harness_checkpoint(&db, rid).unwrap().unwrap();
        let body = build_chat_completions_body(&GatewayRequest {
            provider: ProviderConfig {
                name: "deepseek".into(),
                base_url: "https://api.deepseek.com".into(),
                model: "deepseek-reasoner".into(),
                api_key: Some("test".into()),
                slot: CapabilitySlot::Reasoner,
            },
            messages: loaded.messages,
            tools: vec![],
            max_tokens: Some(2048),
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
        assert_eq!(body["messages"].as_array().unwrap().len(), 2);
    }

    #[test]
    fn mixed_auto_and_confirm_batch_resume_body_is_valid() {
        use crate::ai_harness::tool_turn::outstanding_confirm_ids;
        use crate::ai_runtime::model_gateway::{
            build_chat_completions_body, GatewayRequest, ProviderConfig,
        };
        use crate::ai_runtime::tool_executor::ToolRegistry;
        use crate::ai_runtime::tool_policy::ToolPolicyContext;
        use crate::ai_runtime::{AutonomyLevel, CapabilitySlot};

        let registry = ToolRegistry::new();
        let ctx = ToolPolicyContext {
            scene: AiScene::KnowledgeLookup,
            autonomy_level: AutonomyLevel::L2,
            web_search_enabled: true,
            skill_allowed_tools: vec![],
            depth: 0,
        };
        let web = ToolCall::new("call_web", "web_search", r#"{"query":"chapter 10"}"#);
        let fetch = ToolCall::new(
            "call_fetch",
            "fetch_web_page",
            r#"{"url":"https://example.com/ch10"}"#,
        );
        let messages = vec![
            LlmMessage {
                role: MessageRole::Assistant,
                content: "searching".into(),
                tool_call_id: None,
                tool_calls: Some(vec![web.clone(), fetch.clone()]),
                reasoning_content: None,
            },
            LlmMessage {
                role: MessageRole::Tool,
                content: r#"{"results":[{"url":"https://example.com/ch10"}]}"#.into(),
                tool_call_id: Some(web.id.clone()),
                tool_calls: None,
                ..Default::default()
            },
        ];
        let fetch_id = fetch.id.clone();
        let pending = outstanding_confirm_ids(&registry, &messages, &ctx);
        assert_eq!(pending, vec![fetch_id.clone()]);

        let mut after_approve = messages.clone();
        after_approve.push(LlmMessage {
            role: MessageRole::Tool,
            content: r#"{"title":"Chapter 10"}"#.into(),
            tool_call_id: Some(fetch_id),
            tool_calls: None,
            ..Default::default()
        });

        let body = build_chat_completions_body(&GatewayRequest {
            provider: ProviderConfig {
                name: "deepseek".into(),
                base_url: "https://api.deepseek.com".into(),
                model: "deepseek-reasoner".into(),
                api_key: Some("test".into()),
                slot: CapabilitySlot::Reasoner,
            },
            messages: after_approve,
            tools: vec![],
            max_tokens: Some(2048),
            temperature: Some(0.7),
            stream: false,
            thinking: true,
            skip_stub_ids: vec![],
        });
        let api_msgs = body["messages"].as_array().unwrap();
        assert_eq!(api_msgs.len(), 3);
        assert!(api_msgs[0]["tool_calls"].is_array());
        assert_eq!(api_msgs[1]["role"], "tool");
        assert_eq!(api_msgs[2]["role"], "tool");
    }
}
