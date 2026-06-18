use iris_lib::ai_runtime::agent_task::{
    AgentTaskKind, AgentTaskRuntime, AgentTaskStatus, CreateTaskInput,
};
use iris_lib::ai_runtime::session::SessionManager;
use iris_lib::ai_runtime::AiScene;
use iris_lib::storage::db::Database;

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
