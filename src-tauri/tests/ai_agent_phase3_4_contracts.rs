use iris_lib::ai_runtime::conversation_memory::{
    build_memory_prompt_messages, ConversationMemory, ConversationMemoryPolicy,
};
use iris_lib::ai_runtime::deliberation::{
    append_verification_notice, verification_notice, verify_completion, DeliberationInput,
    DeliberationState, VerificationNoticeStatus, VerificationStatus,
};
use iris_lib::ai_runtime::harness::{HarnessFinishReason, HarnessRunResult};
use iris_lib::ai_runtime::model_gateway::TokenUsage;
use iris_lib::ai_runtime::session::SessionManager;
use iris_lib::ai_runtime::{AiScene, ContextPacket, SourceType, TrustLevel};
use iris_lib::storage::db::Database;

fn context_packet(label: &str) -> ContextPacket {
    ContextPacket {
        id: format!("packet-{label}"),
        source_type: SourceType::Session,
        source_path: None,
        title: "阶段验证材料".into(),
        heading_path: None,
        source_span: None,
        content_hash: "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into(),
        excerpt: "阶段 3/4 验收材料摘要".into(),
        retrieval_reason: "phase contract".into(),
        score: 1.0,
        trust_level: TrustLevel::DerivedCache,
        citation_label: label.into(),
        stale: false,
        web: None,
        corpus: None,
    }
}

#[test]
fn conversation_memory_persists_traceable_long_dialogue_summary() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();

    SessionManager::append_message(
        &db,
        session_id,
        "user",
        "目标: 高质量完成阶段 3 Conversation Memory。偏好: 不要增加无关复杂度。",
        None,
        None,
    )
    .unwrap();
    for i in 2..=52 {
        let content = if i == 21 {
            "决定: 会话摘要必须带 seq 范围与 content_hash。"
        } else if i == 39 {
            "开放问题: resume 后如何恢复目标、偏好、决策、待处理事项。"
        } else {
            "普通长对话轮次，只用于制造 50+ turns 的上下文压力。"
        };
        let role = if i % 2 == 0 { "assistant" } else { "user" };
        SessionManager::append_message(&db, session_id, role, content, None, None).unwrap();
    }

    let memory = ConversationMemory::refresh_for_session(
        &db,
        session_id,
        ConversationMemoryPolicy {
            minimum_messages: 20,
            recent_message_limit: 4,
        },
    )
    .unwrap()
    .expect("long conversation should produce memory");

    assert_eq!(memory.session_id, session_id);
    assert_eq!(memory.seq_start, 1);
    assert!(memory.seq_end >= 48);
    assert_eq!(memory.content_hash.len(), 64);
    assert!(memory.goal_summary.contains("阶段 3"));
    assert!(memory.preference_summary.contains("无关复杂度"));
    assert!(memory.decision_summary.contains("seq"));
    assert!(memory.open_threads_summary.contains("resume"));

    let stored = ConversationMemory::latest_for_session(&db, session_id)
        .unwrap()
        .expect("stored memory");
    assert_eq!(stored.content_hash, memory.content_hash);

    let prompt_messages = build_memory_prompt_messages(&db, session_id, 4).unwrap();
    assert!(prompt_messages[0].1.contains("ConversationMemory"));
    assert!(prompt_messages[0].1.contains("seq=1.."));
    assert_eq!(prompt_messages.len(), 5);
    assert!(prompt_messages
        .iter()
        .skip(1)
        .all(|(role, _)| role == "user" || role == "assistant"));
}

#[test]
fn deliberation_state_tracks_plan_and_blocks_unverified_completion() {
    let state = DeliberationState::from_input(DeliberationInput {
        request_id: "phase4-deliberation".into(),
        session_id: 42,
        user_goal: "完成阶段 4：复杂任务 Deliberation 与 Verification".into(),
        evidence_packet_count: 0,
        tool_result_count: 0,
        max_rounds: 4,
        token_budget: 16_000,
    });

    assert!(state.current_goal.contains("阶段 4"));
    assert!(!state.plan_outline.is_empty());
    assert!(!state.assumptions.is_empty());
    assert!(state
        .verification_items
        .iter()
        .any(|item| item.description.contains("验收")));
    assert!(state.evidence_gaps.iter().any(|gap| gap.contains("证据")));

    let failed = verify_completion(state.clone(), "", &[], HarnessFinishReason::Completed);
    assert!(!failed.passed);
    assert!(failed
        .items
        .iter()
        .any(|item| item.status == VerificationStatus::Failed));

    let passed = verify_completion(
        state,
        "已按阶段 4 验收项完成，并引用 [S1]。",
        &[context_packet("S1")],
        HarnessFinishReason::Completed,
    );
    assert!(passed.passed);
    assert!(passed
        .items
        .iter()
        .all(|item| item.status == VerificationStatus::Passed));
}

#[test]
fn failed_verification_gets_short_user_visible_notice() {
    let state = DeliberationState::from_input(DeliberationInput {
        request_id: "phase4-notice".into(),
        session_id: 42,
        user_goal: "完成阶段 4：复杂任务 Deliberation 与 Verification".into(),
        evidence_packet_count: 0,
        tool_result_count: 0,
        max_rounds: 4,
        token_budget: 16_000,
    });
    let failed = verify_completion(state, "已有初步回答", &[], HarnessFinishReason::Completed);
    let notice = verification_notice(&failed, HarnessFinishReason::Completed)
        .expect("failed verification should produce notice");

    assert_eq!(notice.status, VerificationNoticeStatus::AnswerWithCaveat);
    assert!(notice.message.contains("未验证项"));
    assert!(notice.failed_items.iter().any(|item| item.contains("证据")));

    let content = append_verification_notice("已有初步回答", Some(&notice));
    assert!(content.contains("已有初步回答"));
    assert!(content.contains("未验证项"));

    let finalize = include_str!("../src/ai_harness/harness/finalize.rs");
    assert!(finalize.contains("append_verification_notice"));
    assert!(finalize.contains("verification_notice"));
}

#[test]
fn harness_result_exposes_deliberation_and_verification_without_raw_checkpoint() {
    let result = HarnessRunResult {
        request_id: "phase4-result".into(),
        session_id: 7,
        content: "done [S1]".into(),
        tool_calls: vec![],
        tool_results: vec![],
        usage: TokenUsage::default(),
        citation_valid: true,
        harness_rounds: 2,
        pending_confirmation: false,
        evidence_packets: vec![context_packet("S1")],
        usage_source: iris_lib::ai_runtime::harness::UsageSource::Provider,
        finish_reason: HarnessFinishReason::Completed,
        deliberation_state: Some(DeliberationState::from_input(DeliberationInput {
            request_id: "phase4-result".into(),
            session_id: 7,
            user_goal: "完成阶段 4".into(),
            evidence_packet_count: 1,
            tool_result_count: 0,
            max_rounds: 2,
            token_budget: 4_000,
        })),
        verification_summary: None,
    };

    let json = serde_json::to_value(&result).unwrap();
    assert!(json.get("deliberation_state").is_some());
    assert!(json.get("verification_summary").is_some());
    assert!(json.get("checkpoint").is_none());
    assert!(json.get("full_messages").is_none());
    assert!(json.get("note_content").is_none());
}
