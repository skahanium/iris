use super::*;

#[test]
fn review_regression_ef_complete_short_history_without_memory_has_no_gap() {
    assert!(!history_coverage_is_incomplete(None, &[]));
}

#[test]
fn review_regression_ef_omitted_history_warning_reaches_messages_without_memory() {
    let mut context = context_with_history(pair(5, "recent", 2));
    context.conversation_history_coverage_incomplete = true;
    let messages = context.messages_with_context_material_plan(&context.context_material_plan());
    assert!(messages[0].content.text_content().contains("历史覆盖边界"));
}

fn review_history_fixture(
    pair_count: i64,
    failed_gap: bool,
) -> (crate::storage::db::Database, String, String) {
    use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
    let db = crate::storage::db::Database::open_in_memory().unwrap();
    let session = NormalSessionRepository::create(&db).unwrap();
    db.with_conn(|conn| {
            for index in 0..pair_count {
                let start = index * 2 + 1 + if failed_gap && index >= 2 { 2 } else { 0 };
                for (seq, role) in [(start, "user"), (start + 1, "assistant")] {
                    conn.execute("INSERT INTO session_messages(session_id,seq,role,content,turn_id,created_at) VALUES(?1,?2,?3,'public history',?4,'2026-09-01')", rusqlite::params![session.session_id, seq, role, format!("legacy-{index}")])?;
                }
            }
            if failed_gap {
                conn.execute("INSERT INTO agent_runs(run_id,client_request_id,session_id,turn_id,status,state_version,effect,effort,security_domain,risk,envelope_json,goal_summary,created_at,updated_at) VALUES('failed-gap','failed-gap',?1,'failed-gap','failed',0,'answer','direct','normal','read_only','{}','','2026-09-01','2026-09-01')", [session.session_id])?;
                conn.execute("INSERT INTO session_messages(session_id,seq,role,content,turn_id,created_at) VALUES(?1,5,'user','unpublished failed request','failed-gap','2026-09-01')", [session.session_id])?;
            }
            Ok(())
        }).unwrap();
    let run_id = "review-history-current".to_string();
    AgentRunRepository::accept(
        &db,
        crate::ai_runtime::agent_run_repository::AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: run_id.clone(),
            run_id: run_id.clone(),
            turn_id: run_id.clone(),
            message: "继续".into(),
            content_parts: None,
            explicit_references: Vec::new(),
            context_scope: Default::default(),
            display_mentions: Vec::new(),
            explicit_action: None,
            envelope: context_with_history(Vec::new()).envelope,
        },
    )
    .unwrap();
    (db, session.session_key, run_id)
}

#[test]
fn review_regression_ef_actual_history_window_drives_final_prompt_warning() {
    for (pairs, expected_gap) in [(0, false), (1, false), (14, true)] {
        let (db, session_key, run_id) = review_history_fixture(pairs, false);
        let context = RunContextAssembler::assemble(&db, None, &session_key, &run_id).unwrap();
        assert_eq!(
            context.conversation_history_coverage_incomplete, expected_gap,
            "pairs={pairs}"
        );
        let messages =
            context.messages_with_context_material_plan(&context.context_material_plan());
        assert_eq!(
            messages[0].content.text_content().contains("历史覆盖边界"),
            expected_gap,
            "pairs={pairs}"
        );
    }
}

#[test]
fn review_regression_ef_failed_unpublished_turn_is_not_a_memory_gap() {
    let (db, session_key, run_id) = review_history_fixture(14, true);
    let session = crate::ai_runtime::normal_session_repository::NormalSessionRepository::get(
        &db,
        &session_key,
    )
    .unwrap()
    .unwrap();
    let memory =
        ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
            .unwrap()
            .unwrap();
    assert_eq!(memory.seq_end, 4);
    let context = RunContextAssembler::assemble(&db, None, &session_key, &run_id).unwrap();
    assert!(!context.conversation_history_coverage_incomplete);
    let messages = context.messages_with_context_material_plan(&context.context_material_plan());
    assert!(!messages[0].content.text_content().contains("历史覆盖边界"));
}

fn message(seq: i64, role: &str, content: String, turn_id: &str) -> NormalSessionMessage {
    NormalSessionMessage {
        seq,
        role: role.to_string(),
        content,
        content_parts: None,
        tool_calls: None,
        turn_id: Some(turn_id.to_string()),
        run_id: None,
        turn_state: None,
        retryable: false,
        context_scope: serde_json::json!([]),
        display_mentions: Vec::new(),
        web_citations: Vec::new(),
        citation_binding: None,
        source_summary: Vec::new(),
        evidence_refs: None,
        created_at: "2026-08-07T00:00:00Z".to_string(),
    }
}

fn pair(start_seq: i64, turn_id: &str, tokens_per_message: usize) -> Vec<NormalSessionMessage> {
    vec![
        message(start_seq, "user", "问".repeat(tokens_per_message), turn_id),
        message(
            start_seq + 1,
            "assistant",
            "答".repeat(tokens_per_message),
            turn_id,
        ),
    ]
}

pub(super) fn context_with_history(recent_messages: Vec<NormalSessionMessage>) -> RunContext {
    RunContext {
        session_id: 1,
        message_seq_first: 3,
        user_message: "继续这个对话".to_string(),
        content_parts: None,
        envelope: ExecutionEnvelope {
            effect: crate::ai_runtime::run_contract::Effect::Answer,
            context: ContextMode::Conversation,
            freshness: crate::ai_runtime::run_contract::Freshness::Offline,
            web_reason: crate::ai_runtime::run_contract::WebDecisionReason::LegacyUnknown,
            verification_requirement:
                crate::ai_runtime::run_contract::VerificationRequirement::None,
            effort: crate::ai_runtime::run_contract::Effort::Direct,
            security_domain: crate::ai_runtime::run_contract::SecurityDomain::Normal,
            risk: crate::ai_runtime::run_contract::RiskClass::ReadOnly,
            modalities: vec![crate::ai_runtime::run_contract::Modality::Text],
            material_needs: Vec::new(),
            required_capabilities: Vec::new(),
            explicit_constraints: Vec::new(),
            fresh_fact: Default::default(),
        },
        write_target_path: None,
        document_policy: crate::ai_runtime::policy_decision_engine::PolicyDecisionEngine::new(
            crate::ai_runtime::policy_decision_engine::DocumentPolicy::allow_all(),
        ),
        materials: Vec::new(),
        retrieval_scope: RetrievalScope::default(),
        local_retrieval_packets: Vec::new(),
        recent_messages,
        omitted_history_sequences: Vec::new(),
        conversation_memory: None,
        conversation_history_coverage_incomplete: false,
        prompt_profile: PromptProfile::default(),
        previous_run_summary: None,
        interrupted_assistant_continue: false,
    }
}

#[test]
fn geographic_default_enters_only_unqualified_first_searches() {
    let mut context = context_with_history(Vec::new());
    for question in [
        "近期有什么好看的电影正在热映或者即将上映吗?",
        "最近有什么新歌？",
        "最近的新电影有哪些？",
        "最近有什么新闻？",
        "What are the latest movies?",
    ] {
        context.user_message = question.into();
        assert_eq!(
            context.bootstrap_web_query("2026-09-08"),
            format!("{question} 中国大陆 2026-09-08")
        );
    }
    for question in [
        "美国近期有什么新闻？",
        "法国最近有什么新歌？",
        "台湾近期上映的电影？",
        "上海今晚有什么电影？",
        "全球最近有什么新闻？",
        "Rust 最新版本是什么？",
        "最近天文学有什么新发现？",
        "阿根廷最近有什么电影？",
        "翻译：最近有什么电影？",
        "请读 https://source.invalid/page",
        "近期有什么好看的电影，别只看大陆？",
    ] {
        context.user_message = question.into();
        assert_eq!(
            context.bootstrap_web_query("2026-09-08"),
            format!("{question} 2026-09-08")
        );
    }
}

#[test]
fn geographic_bootstrap_never_exports_history_or_overrides_topic_constraints() {
    let mut context = context_with_history(vec![message(1, "user", "只看法国".into(), "prior")]);
    context.user_message = "最近有什么新歌？".into();
    assert_eq!(
        context.bootstrap_web_query("2026-09-08"),
        "最近有什么新歌？ 2026-09-08"
    );
    let messages = context.messages_with_context_material_plan(&context.context_material_plan());
    let serialized = serde_json::to_string(&messages).expect("messages");
    assert!(serialized.contains("user-confirmed scope"));
    assert!(serialized.contains("只看法国"));
    context.recent_messages.clear();
    context.conversation_history_coverage_incomplete = true;
    assert!(!context
        .bootstrap_web_query("2026-09-08")
        .contains("中国大陆"));
}

#[test]
fn newest_oversized_pair_is_projected_as_a_pair_inside_the_history_budget() {
    let selected = select_bounded_recent_history(pair(1, "latest", 4_001));

    assert_eq!(
        selected.len(),
        2,
        "newest complete pair must remain available"
    );
    assert!(is_coherent_conversation_pair(&selected[0], &selected[1]));
    assert!(selected.iter().all(|message| !message.content.is_empty()));
    assert!(history_pair_tokens(&selected[0], &selected[1]) <= MAX_RECENT_CONVERSATION_TOKENS);
}

#[test]
fn history_selection_stops_at_the_first_nonfitting_older_pair() {
    let mut candidates = pair(1, "older", 500);
    candidates.extend(pair(3, "middle", 3_500));
    candidates.extend(pair(5, "newest", 1_000));

    let selected = select_bounded_recent_history(candidates);

    assert_eq!(selected.len(), 2, "history may not skip an older gap");
    assert_eq!(selected[0].turn_id.as_deref(), Some("newest"));
    assert!(is_coherent_conversation_pair(&selected[0], &selected[1]));
}

#[test]
fn oversized_history_projection_keeps_the_latest_correction_visible() {
    let content = format!(
        "{}最新更正：只回答已核实的当前信息。",
        "早期背景。".repeat(20_000)
    );
    let projected = truncate_history_content_to_token_budget(&content, 128);

    assert!(projected.contains("[历史内容已省略]"));
    assert!(projected.contains("最新更正：只回答已核实的当前信息。"));
    assert!(
        crate::ai_runtime::text_support::estimate_tokens(&projected) <= 128,
        "the visible projection must remain inside its frozen token budget"
    );
}

#[test]
fn oversized_history_projection_never_exceeds_a_tiny_budget() {
    let projected = truncate_history_content_to_token_budget(&"早期背景。".repeat(200), 2);

    assert!(
        crate::ai_runtime::text_support::estimate_tokens(&projected) <= 2,
        "an omission marker must not silently overrun the frozen budget"
    );
}

#[test]
fn provider_history_budget_counts_citation_sanitization_before_projection() {
    let mut latest_pair = pair(1, "latest", 0);
    latest_pair[0].content = "问".repeat(4_000);
    latest_pair[1].content = "[W1]".repeat(3_900);
    latest_pair[1].web_citations = vec![crate::ai_types::WebCitationEntry {
        index: 1,
        title: String::new(),
        url: String::new(),
    }];

    let context = context_with_history(select_bounded_recent_history(latest_pair));
    let messages = context.messages_with_context_material_plan(&context.context_material_plan());
    let provider_history = &messages[1..messages.len() - 1];
    let provider_history_tokens = provider_history
        .iter()
        .map(|message| {
            let content = message.content.text_content();
            crate::ai_runtime::text_support::estimate_tokens(&content)
        })
        .sum::<usize>();

    assert_eq!(
        provider_history.len(),
        2,
        "the latest complete pair remains available"
    );
    assert!(matches!(
        provider_history[0].role,
        crate::ai_runtime::MessageRole::User
    ));
    assert!(matches!(
        provider_history[1].role,
        crate::ai_runtime::MessageRole::Assistant
    ));
    assert!(provider_history[1]
        .content
        .text_content()
        .contains("[历史来源 1]"));
    assert!(
        provider_history_tokens <= MAX_RECENT_CONVERSATION_TOKENS as usize,
        "provider-facing history must stay inside the frozen 8k token budget"
    );
}

#[test]
fn partial_memory_marks_an_omitted_middle_range_for_the_model() {
    let memory = ConversationMemory {
        id: 1,
        session_id: 1,
        seq_start: 1,
        seq_end: 4,
        content_hash: "covered".into(),
        goal_summary: "早期目标".into(),
        preference_summary: String::new(),
        decision_summary: String::new(),
        open_threads_summary: String::new(),
        created_at: "2026-09-05T00:00:00Z".into(),
        updated_at: "2026-09-05T00:00:00Z".into(),
    };
    let recent = pair(11, "recent", 10);
    assert!(history_coverage_is_incomplete(Some(&memory), &[5, 6]));

    let mut context = context_with_history(recent);
    context.conversation_memory = Some(memory);
    context.conversation_history_coverage_incomplete = true;
    let messages = context.messages_with_context_material_plan(&context.context_material_plan());
    assert!(messages[0].content.text_content().contains("历史覆盖边界"));
}

#[test]
fn contiguous_memory_and_recent_history_do_not_claim_a_gap() {
    let memory = ConversationMemory {
        id: 1,
        session_id: 1,
        seq_start: 1,
        seq_end: 4,
        content_hash: "covered".into(),
        goal_summary: String::new(),
        preference_summary: String::new(),
        decision_summary: String::new(),
        open_threads_summary: String::new(),
        created_at: "2026-09-05T00:00:00Z".into(),
        updated_at: "2026-09-05T00:00:00Z".into(),
    };
    assert!(!history_coverage_is_incomplete(
        Some(&memory),
        &[1, 2, 3, 4]
    ));
}

#[test]
fn missing_memory_with_actual_omissions_is_a_coverage_gap() {
    assert!(history_coverage_is_incomplete(None, &[1, 2]));
    assert!(!history_coverage_is_incomplete(None, &[]));
}
