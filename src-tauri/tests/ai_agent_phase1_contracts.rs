use iris_lib::ai_runtime::session::SessionManager;
use iris_lib::ai_runtime::AiScene;
use iris_lib::storage::db::Database;

#[test]
fn session_messages_store_stable_content_hashes() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();

    SessionManager::append_message(&db, session_id, "user", "same content", None, None).unwrap();
    SessionManager::append_message(&db, session_id, "assistant", "same content", None, None)
        .unwrap();

    let messages = SessionManager::recent_messages(&db, session_id, 10).unwrap();
    assert_eq!(messages.len(), 2);
    let first = messages[0].content_hash.as_deref().expect("first hash");
    let second = messages[1].content_hash.as_deref().expect("second hash");
    assert!(!first.is_empty());
    assert_eq!(first, second);
}

#[test]
fn retract_messages_rejects_non_positive_sequence_without_deleting() {
    let db = Database::open_in_memory().unwrap();
    let session_id = SessionManager::ensure(&db, AiScene::KnowledgeLookup, None).unwrap();
    SessionManager::append_message(&db, session_id, "user", "keep me", None, None).unwrap();

    let err = SessionManager::retract_messages(&db, session_id, 0).unwrap_err();
    assert!(err.to_string().contains("from_seq"));

    let messages = SessionManager::recent_messages(&db, session_id, 10).unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].content, "keep me");
}

#[test]
fn stage1_source_contracts_remove_misleading_tool_executor_stub() {
    let tool_executor = include_str!("../src/ai_runtime/tool_executor.rs");

    assert!(!tool_executor.contains("Actual dispatch is handled by the caller"));
    assert!(!tool_executor.contains("output: args"));
}

#[test]
fn stage1_source_contracts_remove_rendered_fetch_from_catalog() {
    let catalog = include_str!("../src/ai_runtime/tool_catalog_impl.rs");
    let web_catalog = include_str!("../src/ai_runtime/tool_catalog/web.rs");

    assert!(!catalog.contains("rendered_fetch"));
    assert!(!web_catalog.contains("rendered_fetch"));
    assert!(web_catalog.contains("WebEvidenceBroker"));
}

#[test]
fn stage1_source_contracts_remove_git_skill_install_runtime() {
    let runtime_mod = include_str!("../src/ai_runtime/mod.rs");
    let skills = include_str!("../src/ai_runtime/skills_impl.rs");

    assert!(!runtime_mod.contains("skill_install_service"));
    assert!(!skills.contains("Command::new(\"git\")"));
    assert!(!skills.contains("core.hooksPath=/dev/null"));
    assert!(!skills.contains("protocol.file.allow=never"));
    assert!(!skills.contains("--no-tags"));
}
