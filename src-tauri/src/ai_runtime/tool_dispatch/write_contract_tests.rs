use super::*;

#[tokio::test]
async fn rejected_patch_is_not_a_successful_dispatch_and_creates_no_version() {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    std::fs::write(vault.join("note.md"), "before\n").unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    state.set_vault(vault.clone()).unwrap();
    state
        .db
        .with_conn(|conn| crate::indexer::scan::index_file(conn, &vault, &vault.join("note.md")))
        .unwrap();
    let scope = crate::ai_runtime::retrieval_scope::RetrievalScope::default();
    let ctx = ToolDispatchContext {
        db: Some(&state.db),
        selected_web_provider_id: None,
        note_path: None,
        file_id: None,
        run_id: None,
        write_target_path: Some("note.md"),
        confirmed_write_targets: None,
        document_policy: None,
        web_search_enabled: false,
        available_tool_names: &[],
        max_web_fetches: 0,
        cold_start_packets: &[],
        retrieval_scope: &scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
    };
    let result = dispatch_tool(
        &state,
        &ctx,
        "replace_selection",
        &serde_json::json!({
            "target_path": "note.md", "base_content_hash": "stale",
            "range": {"start": 0, "end": 6}, "original_text": "before", "replacement": "after"
        }),
    )
    .await;
    assert!(
        !result.success,
        "a rejected edit must not advance the applied checkpoint"
    );
    assert_eq!(
        std::fs::read_to_string(vault.join("note.md")).unwrap(),
        "before\n"
    );
    assert!(crate::version::version_list(&state, "note.md")
        .unwrap()
        .is_empty());

    state
        .db
        .with_conn(|conn| {
            conn.execute("UPDATE files SET is_locked = 1 WHERE path = 'note.md'", [])?;
            Ok(())
        })
        .unwrap();
    let result = dispatch_tool(&state, &ctx, "replace_selection", &serde_json::json!({
        "target_path": "note.md", "base_content_hash": crate::cas::hash::content_hash_str("before\n"),
        "range": {"start": 0, "end": 6}, "original_text": "before", "replacement": "after"
    })).await;
    assert!(!result.success);
    assert!(
        crate::version::version_list(&state, "note.md")
            .unwrap()
            .is_empty(),
        "locked edits must be rejected before snapshot creation"
    );
}
