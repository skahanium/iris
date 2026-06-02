//! Integration-style tests for tool confirm → checkpoint → resume metadata.

#[cfg(test)]
mod tests {
    use crate::ai_runtime::harness_support::{
        load_harness_checkpoint, save_harness_checkpoint, HarnessCheckpoint, HarnessCheckpointMeta,
    };
    use crate::ai_runtime::harness_confirm::append_rejected_tool_to_checkpoint;
    use crate::ai_runtime::model_gateway::{LlmMessage, MessageRole, TokenUsage};
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
            }],
            tool_calls: vec![],
            tool_results: vec![serde_json::json!({
                "tool_call_id": "tc1",
                "status": "pending_confirmation",
            })],
            evidence_packets: vec![],
            usage: TokenUsage::default(),
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
        TraceRecorder::update_status(&state.db, rid, TraceStatus::AwaitingToolConfirmation).unwrap();
        save_harness_checkpoint(&state.db, rid, &sample_checkpoint(rid)).unwrap();

        append_rejected_tool_to_checkpoint(state.as_ref(), rid, "tc1").unwrap();

        let cp = load_harness_checkpoint(&state.db, rid).unwrap().unwrap();
        assert_eq!(cp.messages.len(), 2);
        assert!(matches!(cp.messages[1].role, MessageRole::Tool));
        assert!(cp.messages[1].content.contains("rejected"));
        assert!(
            cp.tool_results
                .iter()
                .any(|r| r.get("status").and_then(|s| s.as_str()) == Some("rejected"))
        );
    }
}
