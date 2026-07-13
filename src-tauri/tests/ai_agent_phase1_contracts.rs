use iris_lib::storage::db::Database;

#[test]
fn run_owned_messages_store_stable_content_hashes() {
    let db = Database::open_in_memory().expect("database");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sessions (session_key, created_at, updated_at)
             VALUES ('phase-one-session', datetime('now'), datetime('now'))",
            [],
        )?;
        let session_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, content_hash, turn_id, evidence_refs_json, created_at)
             VALUES (?1, 1, 'user', 'same content', 'same-hash', 'phase-one-turn', '[]', datetime('now')),
                    (?1, 2, 'assistant', 'same content', 'same-hash', 'phase-one-turn', '[]', datetime('now'))",
            [session_id],
        )?;
        let hashes = conn
            .prepare("SELECT content_hash FROM session_messages WHERE session_id = ?1 ORDER BY seq")?
            .query_map([session_id], |row| row.get::<_, Option<String>>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        assert_eq!(hashes, vec![Some("same-hash".to_string()), Some("same-hash".to_string())]);
        Ok(())
    })
    .expect("run-owned message facts");
}

#[test]
fn final_schema_has_no_scene_or_note_path_session_columns() {
    let db = Database::open_in_memory().expect("database");
    db.with_read_conn(|conn| {
        let columns = conn
            .prepare("PRAGMA table_info(sessions)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?;
        assert!(!columns.iter().any(|column| column == "scene"));
        assert!(!columns.iter().any(|column| column == "note_path"));
        Ok(())
    })
    .expect("cutover schema");
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

#[test]
fn tool_catalog_is_capability_driven_without_legacy_scenes() {
    let catalog = include_str!("../src/ai_runtime/tool_catalog_impl.rs");
    assert!(
        !catalog.contains("scene_affinity"),
        "tool exposure must be expressed through capabilities, never legacy scenes"
    );
    assert!(
        !catalog.contains("AiScene"),
        "tool catalog must not depend on AiScene"
    );
}

#[test]
fn run_cutover_has_no_legacy_writing_workflow_or_direct_patch_ipc() {
    let runtime = include_str!("../src/ai_runtime/mod.rs");
    let commands = include_str!("../src/commands/mod.rs");
    let app = include_str!("../src/lib.rs");

    assert!(!runtime.contains("writing_workflow"));
    assert!(!commands.contains("writing_commands"));
    assert!(!app.contains("patch_apply"));
}

#[test]
fn production_types_have_no_task_plan_or_legacy_scene_tool_metadata() {
    let types = include_str!("../src/ai_types/mod.rs");
    assert!(!types.contains("TaskPlan"));
    assert!(!types.contains("legacy_scene"));
}
