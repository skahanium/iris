use iris_lib::ai_runtime::agent_task::{
    AgentTaskKind, AgentTaskResumePreflight, AgentTaskRuntime, AgentTaskStatus,
    BudgetPauseCheckpointInput, CreateTaskInput, TaskListFilter,
};
use iris_lib::ai_runtime::session::SessionManager;
use iris_lib::ai_runtime::AiScene;
use iris_lib::storage::db::Database;
use std::fs;

#[test]
fn lightweight_task_stores_summary_not_full_user_text() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    let full_text =
        "请阅读这份很长的用户问题正文。这里模拟笔记内容和完整 prompt，不应该被复制进 task 表。";

    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-1".into(),
            session_id,
            kind: AgentTaskKind::Lightweight,
            user_input: full_text.into(),
            budget_policy: serde_json::json!({ "mode": "lightweight", "max_steps": 1 }),
        },
    )
    .unwrap();

    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.status, AgentTaskStatus::Running);
    assert_ne!(task.user_goal_summary, full_text);
    assert!(task.user_goal_summary.chars().count() <= 80);
    assert_eq!(task.budget_policy["mode"], "lightweight");
}

#[test]
fn task_lifecycle_tracks_completion_and_cascades_with_session() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();

    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-2".into(),
            session_id,
            kind: AgentTaskKind::Lightweight,
            user_input: "短问题".into(),
            budget_policy: serde_json::json!({ "mode": "lightweight", "max_steps": 1 }),
        },
    )
    .unwrap();

    AgentTaskRuntime::record_step(
        &db,
        &task_id,
        "respond",
        AgentTaskStatus::Completed,
        "user asked a short question",
        "assistant answered",
        serde_json::json!({ "summary": "assistant answered", "packet_ids": [] }),
    )
    .unwrap();
    AgentTaskRuntime::complete_task(&db, &task_id).unwrap();

    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.status, AgentTaskStatus::Completed);

    SessionManager::delete_session(&db, session_id).unwrap();
    assert!(AgentTaskRuntime::get_task(&db, &task_id).unwrap().is_none());
}

#[test]
fn checkpoint_rejects_full_context_and_secret_shaped_fields() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-sensitive-checkpoint".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "research a long document".into(),
            budget_policy: serde_json::json!({ "mode": "complex", "segment_budget": 2048 }),
        },
    )
    .unwrap();

    let err = AgentTaskRuntime::record_step(
        &db,
        &task_id,
        "research",
        AgentTaskStatus::PausedBudget,
        "input summarized",
        "output summarized",
        serde_json::json!({
            "summary": "partial progress",
            "messages": [{ "role": "user", "content": "full prompt should not be here" }],
            "api_key": "sk-test-secret",
            "note_body": "complete note body should not be here"
        }),
    )
    .unwrap_err();

    assert!(err.to_string().contains("unsafe checkpoint"));
    let steps: i64 = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_task_steps WHERE task_id = ?1",
                [task_id.as_str()],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(steps, 0);
}

#[test]
fn budget_pause_records_safe_checkpoint_and_true_task_lookup() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-budget-pause".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "deep research task".into(),
            budget_policy: serde_json::json!({
                "mode": "complex",
                "segment_input_budget": 1024,
                "segment_output_budget": 512,
                "resume": "auto"
            }),
        },
    )
    .unwrap();

    AgentTaskRuntime::pause_budget(
        &db,
        &task_id,
        "segment exhausted safely",
        serde_json::json!({
            "summary": "finished source triage, needs synthesis",
            "decisions": ["use packet ids instead of full notes"],
            "evidence_packet_ids": ["packet-1", "packet-2"],
            "next_action": "continue synthesis"
        }),
    )
    .unwrap();

    assert_eq!(
        AgentTaskRuntime::task_id_for_request(&db, "req-runtime-budget-pause")
            .unwrap()
            .as_deref(),
        Some(task_id.as_str())
    );

    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.status, AgentTaskStatus::PausedBudget);

    let checkpoint_json: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT checkpoint_json FROM agent_task_steps WHERE task_id = ?1",
                [task_id.as_str()],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert!(checkpoint_json.contains("finished source triage"));
    assert!(!checkpoint_json.contains("deep research task"));
}

#[test]
fn resume_plan_uses_latest_safe_checkpoint_without_full_prompt_or_results() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-resume-plan".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "very long user prompt that must only be summarized".into(),
            budget_policy: serde_json::json!({
                "mode": "complex",
                "agent_intent": "research",
                "legacy_scene_hint": "research_synthesis",
                "vault_scope_hash": "vault-scope-123",
                "required_permissions": ["read_notes", "web_fetch"]
            }),
        },
    )
    .unwrap();

    AgentTaskRuntime::pause_budget(
        &db,
        &task_id,
        "triage complete; synthesis remains",
        serde_json::json!({
            "continuation_goal": "synthesize the remaining evidence into an answer",
            "evidence_refs": ["packet-a", "packet-b"],
            "evidence_packet_ids": ["packet-a", "packet-b"],
            "evidence_ledger_summary": "2 relevant packets, no raw note body",
            "last_safe_step": "evidence_triage",
            "next_action": "finalize_answer",
            "remaining_budget_hint": {
                "input_tokens": 1600,
                "output_tokens": 700
            }
        }),
    )
    .unwrap();

    let plan = AgentTaskRuntime::prepare_resume_plan(&db, &task_id).unwrap();

    assert_eq!(plan.task_id, task_id);
    assert_eq!(plan.request_id, "req-runtime-resume-plan");
    assert_eq!(plan.session_id, session_id);
    assert_eq!(plan.agent_intent.as_deref(), Some("research"));
    assert_eq!(
        plan.legacy_scene_hint.as_deref(),
        Some("research_synthesis")
    );
    assert_eq!(plan.vault_scope_hash.as_deref(), Some("vault-scope-123"));
    assert_eq!(plan.selected_packet_ids, vec!["packet-a", "packet-b"]);
    assert_eq!(
        plan.continuation_goal.as_deref(),
        Some("synthesize the remaining evidence into an answer")
    );
    assert_eq!(plan.next_action.as_deref(), Some("finalize_answer"));

    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.status, AgentTaskStatus::PausedBudget);

    AgentTaskRuntime::begin_resume(&db, &task_id, &plan).unwrap();
    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.status, AgentTaskStatus::Running);

    let checkpoint_json: String = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT checkpoint_json FROM agent_task_steps WHERE task_id = ?1 ORDER BY step_seq DESC LIMIT 1",
                [task_id.as_str()],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert!(!checkpoint_json.contains("messages"));
    assert!(!checkpoint_json.contains("tool_results"));
    assert!(!checkpoint_json.contains("very long user prompt"));
}

#[test]
fn resume_denies_missing_session_before_runtime_continuation() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-resume-deleted-session".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "deep research task".into(),
            budget_policy: serde_json::json!({ "mode": "complex", "agent_intent": "research" }),
        },
    )
    .unwrap();
    AgentTaskRuntime::pause_budget(
        &db,
        &task_id,
        "needs continuation",
        serde_json::json!({
            "continuation_goal": "continue safely",
            "next_action": "continue_research"
        }),
    )
    .unwrap();

    SessionManager::delete_session(&db, session_id).unwrap();

    let err = AgentTaskRuntime::prepare_resume_plan(&db, &task_id).unwrap_err();
    assert!(err.to_string().contains("agent task not found"));
}

#[test]
fn resume_requires_paused_recoverable_task_state() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-resume-running".into(),
            session_id,
            kind: AgentTaskKind::Lightweight,
            user_input: "short chat".into(),
            budget_policy: serde_json::json!({ "mode": "lightweight" }),
        },
    )
    .unwrap();

    let err = AgentTaskRuntime::prepare_resume_plan(&db, &task_id).unwrap_err();
    assert!(err.to_string().contains("not resumable"));
}

#[test]
fn budget_pause_checkpoint_shape_is_sufficient_for_safe_resume() {
    let checkpoint = AgentTaskRuntime::build_budget_pause_checkpoint(BudgetPauseCheckpointInput {
        finish_reason: "budget_exhausted",
        selected_packet_ids: vec!["packet-1".into()],
        evidence_packet_ids: vec!["packet-1".into(), "packet-2".into()],
        evidence_ledger_summary: "2 packets retained, raw notes excluded".into(),
        continuation_goal: "continue from compacted evidence".into(),
        last_safe_step: "round_limit_before_final".into(),
        next_action: "resume task with compacted context".into(),
        remaining_budget_hint: serde_json::json!({
            "input_tokens": 1024,
            "output_tokens": 512
        }),
    });

    for key in [
        "finish_reason",
        "selected_packet_ids",
        "evidence_packet_ids",
        "evidence_ledger_summary",
        "continuation_goal",
        "last_safe_step",
        "next_action",
        "remaining_budget_hint",
    ] {
        assert!(checkpoint.get(key).is_some(), "missing key {key}");
    }
    assert!(checkpoint.get("messages").is_none());
    assert!(checkpoint.get("tool_results").is_none());
    assert!(checkpoint.get("content").is_none());
}

#[test]
fn resume_preflight_rejects_scope_packet_skill_permission_and_model_drift() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-resume-preflight".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "research task".into(),
            budget_policy: serde_json::json!({
                "mode": "complex",
                "vault_scope_hash": "vault-old",
                "note_path": "notes/research.md",
                "required_permissions": ["web.fetch"],
                "required_skills": ["source-triage"],
                "required_model_slot": "reasoner",
            }),
        },
    )
    .unwrap();
    AgentTaskRuntime::pause_budget(
        &db,
        &task_id,
        "needs continuation",
        AgentTaskRuntime::build_budget_pause_checkpoint(BudgetPauseCheckpointInput {
            finish_reason: "budget_exhausted",
            selected_packet_ids: vec!["packet-missing".into()],
            evidence_packet_ids: vec!["packet-known".into()],
            evidence_ledger_summary: "1 retained packet".into(),
            continuation_goal: "continue safely".into(),
            last_safe_step: "context_compacted".into(),
            next_action: "resume".into(),
            remaining_budget_hint: serde_json::json!({
                "input_tokens": 1000,
                "output_tokens": 400
            }),
        }),
    )
    .unwrap();

    let plan = AgentTaskRuntime::prepare_resume_plan(&db, &task_id).unwrap();
    let preflight = AgentTaskResumePreflight {
        current_session_id: Some(session_id),
        current_vault_scope_hash: Some("vault-new".into()),
        accessible_note_paths: vec![],
        available_packet_ids: vec!["packet-known".into()],
        enabled_skill_names: vec![],
        active_permissions: vec![],
        current_model_slot: Some("fast".into()),
    };

    let err = AgentTaskRuntime::validate_resume_preflight(&plan, &preflight).unwrap_err();
    let message = err.to_string();
    assert!(message.contains("vault scope changed"));
    assert!(message.contains("note path unavailable"));
    assert!(message.contains("selected packet unavailable"));
    assert!(message.contains("skill unavailable"));
    assert!(message.contains("permission expired"));
    assert!(message.contains("model capability changed"));

    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.status, AgentTaskStatus::PausedBudget);
}

#[test]
fn resume_preflight_rejects_session_mismatch() {
    let db = Database::open_in_memory().unwrap();
    let session_a = SessionManager::ensure(&db, AiScene::DraftingAssist, None).unwrap();
    let session_b = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-session-preflight".into(),
            session_id: session_a,
            kind: AgentTaskKind::Complex,
            user_input: "rewrite selected text".into(),
            budget_policy: serde_json::json!({
                "mode": "complex",
                "vault_scope_hash": "vault-scope",
                "required_model_slot": "writer"
            }),
        },
    )
    .unwrap();
    AgentTaskRuntime::pause_budget(
        &db,
        &task_id,
        "awaiting safe continuation",
        AgentTaskRuntime::build_budget_pause_checkpoint(BudgetPauseCheckpointInput {
            finish_reason: "budget_exhausted",
            selected_packet_ids: vec![],
            evidence_packet_ids: vec![],
            evidence_ledger_summary: "no raw body retained".into(),
            continuation_goal: "continue write proposal".into(),
            last_safe_step: "confirmation_pending".into(),
            next_action: "resume".into(),
            remaining_budget_hint: serde_json::json!({"input_tokens": 256}),
        }),
    )
    .unwrap();

    let plan = AgentTaskRuntime::prepare_resume_plan(&db, &task_id).unwrap();
    let err = AgentTaskRuntime::validate_resume_preflight(
        &plan,
        &AgentTaskResumePreflight {
            current_session_id: Some(session_b),
            current_vault_scope_hash: Some("vault-scope".into()),
            accessible_note_paths: vec![],
            available_packet_ids: vec![],
            enabled_skill_names: vec![],
            active_permissions: vec![],
            current_model_slot: Some("writer".into()),
        },
    )
    .unwrap_err();

    assert!(err.to_string().contains("session changed"));
    let task = AgentTaskRuntime::get_task(&db, &task_id).unwrap().unwrap();
    assert_eq!(task.session_id, session_a);
    assert_eq!(task.status, AgentTaskStatus::PausedBudget);
}

#[test]
fn task_list_filters_by_session_and_status() {
    let db = Database::open_in_memory().unwrap();
    let sid_one = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    let sid_two = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let paused_task = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-list-paused".into(),
            session_id: sid_one,
            kind: AgentTaskKind::Complex,
            user_input: "long task".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();
    AgentTaskRuntime::pause_budget(
        &db,
        &paused_task,
        "paused safely",
        serde_json::json!({
            "continuation_goal": "continue safely",
            "next_action": "continue_context_gathering"
        }),
    )
    .unwrap();
    AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-list-other-session".into(),
            session_id: sid_two,
            kind: AgentTaskKind::Complex,
            user_input: "other task".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();

    let tasks = AgentTaskRuntime::list_tasks(
        &db,
        TaskListFilter {
            session_id: Some(sid_one),
            status: Some(AgentTaskStatus::PausedBudget),
        },
    )
    .unwrap();

    assert_eq!(tasks.len(), 1);
    assert_eq!(tasks[0].task_id, paused_task);
    assert_eq!(tasks[0].status, AgentTaskStatus::PausedBudget);
}

#[test]
fn task_step_and_event_dtos_are_summary_only_for_ui() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-ui-dto".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "summarize only for UI".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();
    AgentTaskRuntime::record_step(
        &db,
        &task_id,
        "research",
        AgentTaskStatus::PausedBudget,
        "input summary only",
        "output summary with citations",
        serde_json::json!({
            "summary": "safe checkpoint",
            "evidence_packet_ids": ["pkt-1", "pkt-2"],
            "continuation_goal": "continue from summaries"
        }),
    )
    .unwrap();
    AgentTaskRuntime::record_event(
        &db,
        &task_id,
        "permission_wait",
        "waiting for vault write approval",
        serde_json::json!({
            "tool": "write_markdown",
            "raw_result": "must not be exposed"
        }),
    )
    .unwrap();

    let steps = AgentTaskRuntime::list_steps(&db, &task_id).unwrap();
    let events = AgentTaskRuntime::list_events(&db, &task_id).unwrap();

    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].kind, "research");
    assert_eq!(steps[0].output_summary, "output summary with citations");
    assert_eq!(steps[0].evidence_packet_ids, vec!["pkt-1", "pkt-2"]);
    let serialized_step = serde_json::to_string(&steps[0]).unwrap();
    assert!(!serialized_step.contains("checkpoint_json"));
    assert!(!serialized_step.contains("continuation_goal"));

    assert_eq!(events.len(), 1);
    assert_eq!(events[0].event_type, "permission_wait");
    assert_eq!(events[0].message, "waiting for vault write approval");
    let serialized_event = serde_json::to_string(&events[0]).unwrap();
    assert!(!serialized_event.contains("raw_result"));
}

#[test]
fn lifecycle_cleanup_aborts_recoverable_tasks_without_deleting_sessions_or_notes() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let running_task = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-cleanup-running".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "running task".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();
    let paused_task = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-cleanup-paused".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "paused task".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();
    AgentTaskRuntime::pause_budget(
        &db,
        &paused_task,
        "safe pause",
        serde_json::json!({
            "continuation_goal": "continue later",
            "next_action": "resume"
        }),
    )
    .unwrap();

    let aborted = AgentTaskRuntime::abort_recoverable_tasks(
        &db,
        "cache_clear",
        "AI cache clear invalidated recoverable task state",
    )
    .unwrap();

    assert_eq!(aborted, 2);
    assert_eq!(
        AgentTaskRuntime::get_task(&db, &running_task)
            .unwrap()
            .unwrap()
            .status,
        AgentTaskStatus::Aborted
    );
    assert_eq!(
        AgentTaskRuntime::get_task(&db, &paused_task)
            .unwrap()
            .unwrap()
            .status,
        AgentTaskStatus::Aborted
    );
    assert!(SessionManager::get_session(&db, session_id)
        .unwrap()
        .is_some());

    let event_count: i64 = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM agent_task_events WHERE event_type = 'lifecycle_cleanup'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(event_count, 2);
}

#[test]
fn cache_clear_sequence_deletes_recovery_state_without_deleting_note_file() {
    let temp = tempfile::tempdir().unwrap();
    let note_path = temp.path().join("notes").join("a.md");
    fs::create_dir_all(note_path.parent().unwrap()).unwrap();
    fs::write(&note_path, "user note body").unwrap();

    let db = Database::open_in_memory().unwrap();
    let session_id =
        SessionManager::ensure(&db, AiScene::ResearchSynthesis, Some("notes/a.md")).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-cache-clear-sequence".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "task tied to a note".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();
    AgentTaskRuntime::pause_budget(
        &db,
        &task_id,
        "safe pause",
        serde_json::json!({
            "continuation_goal": "continue later",
            "next_action": "resume"
        }),
    )
    .unwrap();
    AgentTaskRuntime::record_event(
        &db,
        &task_id,
        "status",
        "paused",
        serde_json::json!({ "safe": true }),
    )
    .unwrap();

    let aborted = AgentTaskRuntime::abort_recoverable_tasks(
        &db,
        "CACHE_CLEAR",
        "AI cache clear invalidated recoverable task state",
    )
    .unwrap();
    let deleted = SessionManager::delete_all_filtered(&db, None, None).unwrap();

    assert_eq!(aborted, 1);
    assert_eq!(deleted, 1);
    for table in ["agent_tasks", "agent_task_steps", "agent_task_events"] {
        let count: i64 = db
            .with_read_conn(|conn| {
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} should be cleared by cache cleanup");
    }
    assert_eq!(fs::read_to_string(note_path).unwrap(), "user note body");
}

#[test]
fn session_clear_filtered_cascades_tasks_steps_and_events() {
    let db = Database::open_in_memory().unwrap();
    let session_id =
        SessionManager::ensure(&db, AiScene::KnowledgeLookup, Some("notes/a.md")).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-runtime-clear-filtered".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "task tied to a deleted session".into(),
            budget_policy: serde_json::json!({ "mode": "complex" }),
        },
    )
    .unwrap();
    AgentTaskRuntime::record_step(
        &db,
        &task_id,
        "respond",
        AgentTaskStatus::Running,
        "input summary",
        "output summary",
        serde_json::json!({ "summary": "safe checkpoint" }),
    )
    .unwrap();
    AgentTaskRuntime::record_event(
        &db,
        &task_id,
        "status",
        "started",
        serde_json::json!({ "safe": true }),
    )
    .unwrap();

    let deleted =
        SessionManager::delete_all_filtered(&db, Some("knowledge_lookup"), Some("notes/a.md"))
            .unwrap();
    assert_eq!(deleted, 1);

    for table in ["agent_tasks", "agent_task_steps", "agent_task_events"] {
        let count: i64 = db
            .with_read_conn(|conn| {
                conn.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |row| {
                    row.get(0)
                })
                .map_err(Into::into)
            })
            .unwrap();
        assert_eq!(count, 0, "{table} should not contain orphan rows");
    }
}
