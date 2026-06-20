use iris_lib::ai_runtime::agent_task::{
    AgentTaskKind, AgentTaskRuntime, AgentTaskStatus, CreateTaskInput,
};
use iris_lib::ai_runtime::session::SessionManager;
use iris_lib::ai_runtime::AiScene;
use iris_lib::app::AppState;
use iris_lib::storage::db::Database;
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;
use std::fs;

#[test]
fn vault_set_persists_path_for_watcher_restart() {
    let dir = tempfile::tempdir().unwrap();
    let vault_a = dir.path().join("vault-a");
    let vault_b = dir.path().join("vault-b");
    fs::create_dir_all(&vault_a).unwrap();
    fs::create_dir_all(&vault_b).unwrap();

    let state = AppState::new(dir.path().join("data")).unwrap();
    assert!(state.watcher.lock().unwrap().is_none());

    state.set_vault(vault_a.clone()).unwrap();
    let path_a = state.vault_path().unwrap();
    assert_eq!(path_a, vault_a.canonicalize().unwrap());

    state.set_vault(vault_b.clone()).unwrap();
    let path_b = state.vault_path().unwrap();
    assert_eq!(path_b, vault_b.canonicalize().unwrap());

    let conn = Connection::open(dir.path().join("data").join("iris.db")).unwrap();
    migrate_up(&conn).unwrap();
    let stored: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'vault_path'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(stored.contains("vault-b"));
}

#[test]
fn invalid_stored_vault_reset_aborts_recoverable_tasks() {
    let dir = tempfile::tempdir().unwrap();
    let data_dir = dir.path().join("data");
    fs::create_dir_all(&data_dir).unwrap();
    let db_path = data_dir.join("iris.db");
    let db = Database::open(&db_path).unwrap();
    let missing_vault = dir.path().join("missing-vault");

    db.with_conn(|conn| {
        let json = serde_json::to_string(missing_vault.to_string_lossy().as_ref()).unwrap();
        conn.execute(
            "INSERT INTO settings (key, value) VALUES ('vault_path', ?1)",
            [json],
        )?;
        Ok::<_, iris_lib::error::AppError>(())
    })
    .unwrap();

    let session_id = SessionManager::ensure(&db, AiScene::ResearchSynthesis, None).unwrap();
    let task_id = AgentTaskRuntime::create_task(
        &db,
        CreateTaskInput {
            request_id: "req-invalid-vault-reset".into(),
            session_id,
            kind: AgentTaskKind::Complex,
            user_input: "paused task from missing vault".into(),
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
    drop(db);

    let state = AppState::new(data_dir).unwrap();

    assert!(state.vault_path().is_err());
    let task = AgentTaskRuntime::get_task(&state.db, &task_id)
        .unwrap()
        .unwrap();
    assert_eq!(task.status, AgentTaskStatus::Aborted);

    let setting_count: i64 = state
        .db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT COUNT(*) FROM settings WHERE key = 'vault_path'",
                [],
                |row| row.get(0),
            )
            .map_err(Into::into)
        })
        .unwrap();
    assert_eq!(setting_count, 0);
}
