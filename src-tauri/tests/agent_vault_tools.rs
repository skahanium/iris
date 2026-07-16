use iris_lib::ai_runtime::retrieval_scope::RetrievalScope;
use iris_lib::ai_runtime::tool_catalog::catalog_find;
use iris_lib::ai_runtime::tool_dispatch::{
    dispatch_tool, ToolDispatchContext, DISPATCHABLE_TOOL_NAMES,
};

use iris_lib::app::AppState;

fn test_state() -> (std::sync::Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let state = AppState::new_with_test_cas_key(dir.path().to_path_buf(), [0xC5; 32]).unwrap();
    state.set_vault(vault).unwrap();
    (state, dir)
}

fn ctx<'a>(note_path: Option<&'a str>) -> ToolDispatchContext<'a> {
    ctx_with_scope(note_path, Box::leak(Box::new(RetrievalScope::default())))
}

fn ctx_with_scope<'a>(
    note_path: Option<&'a str>,
    retrieval_scope: &'a RetrievalScope,
) -> ToolDispatchContext<'a> {
    ToolDispatchContext {
        note_path,
        file_id: None,
        web_search_enabled: false,
        max_web_fetches: 3,
        cold_start_packets: &[],
        retrieval_scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: 0,
        skill_activation_plan: None,
    }
}

fn index_note(state: &std::sync::Arc<AppState>, path: &str, content: &str) {
    let vault = state.vault_path().unwrap();
    let abs = vault.join(path);
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent).unwrap();
    }
    std::fs::write(&abs, content).unwrap();
    let hash = iris_lib::indexer::scan::content_hash(content);
    state
        .db
        .with_conn(|conn| {
            iris_lib::indexer::scan::index_file_from_content(conn, &vault, &abs, content, &hash)
        })
        .unwrap();
}

#[test]
fn catalog_exposes_phase5_vault_core_tools() {
    for name in [
        "vault_create_note",
        "vault_rename_move",
        "vault_delete_to_trash",
        "vault_asset_write",
        "vault_version_list",
    ] {
        let entry = catalog_find(name).unwrap_or_else(|| panic!("{name} missing from catalog"));
        assert!(
            DISPATCHABLE_TOOL_NAMES.contains(&name),
            "{name} must be dispatchable"
        );
        if name == "vault_version_list" {
            assert!(!entry.requires_confirmation);
        } else {
            assert!(entry.requires_confirmation);
        }
    }
}

#[tokio::test]
async fn markdown_patch_tool_creates_pre_write_snapshot() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/test.md", "# Test\nHello world");

    let base = "# Test\nHello world";
    let base_hash = iris_lib::cas::hash::content_hash_str(base);
    let result = dispatch_tool(
        &state,
        &ctx(Some("notes/test.md")),
        "replace_selection",
        &serde_json::json!({
            "target_path": "notes/test.md",
            "replacement": "Hi",
            "base_content_hash": base_hash,
            "range": {"start": 7, "end": 12},
            "original_text": "Hello",
            "risk_level": "medium"
        }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    let content =
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
    assert_eq!(content, "# Test\nHi world");

    let versions = iris_lib::version::version_list(&state, "notes/test.md").unwrap();
    assert_eq!(versions.len(), 1);
    let snapshot = iris_lib::version::version_preview(&state, versions[0].id).unwrap();
    assert_eq!(snapshot, base);
}

#[tokio::test]
async fn vault_create_note_writes_markdown_and_indexes_it() {
    let (state, _dir) = test_state();
    let result = dispatch_tool(
        &state,
        &ctx(None),
        "vault_create_note",
        &serde_json::json!({
            "target_path": "notes/new.md",
            "content": "# New\n\nBody"
        }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["path"], "notes/new.md");
    assert_eq!(
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/new.md")).unwrap(),
        "# New\n\nBody"
    );
    let count: i64 = state
        .db
        .with_read_conn(|conn| {
            Ok(conn.query_row(
                "SELECT COUNT(*) FROM files WHERE path = 'notes/new.md'",
                [],
                |row| row.get(0),
            )?)
        })
        .unwrap();
    assert_eq!(count, 1);
}

#[tokio::test]
async fn vault_create_note_never_overwrites_an_existing_note() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/existing.md", "# Existing\n\nOriginal");

    let result = dispatch_tool(
        &state,
        &ctx(None),
        "vault_create_note",
        &serde_json::json!({
            "target_path": "notes/existing.md",
            "content": "# Replacement"
        }),
    )
    .await;

    assert!(!result.success);
    assert_eq!(
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/existing.md")).unwrap(),
        "# Existing\n\nOriginal"
    );
}

#[tokio::test]
async fn search_tool_respects_hard_retrieval_scope_and_clamps_limit() {
    let (state, _dir) = test_state();
    index_note(
        &state,
        "scoped/target.md",
        "# Target\n\nalpha scoped evidence",
    );
    index_note(
        &state,
        "outside/leak.md",
        "# Leak\n\nalpha outside evidence",
    );
    let retrieval_scope = RetrievalScope {
        path_prefixes: vec!["scoped/".into()],
        paths: Vec::new(),
        required_tags: Vec::new(),
    };
    let ctx = ctx_with_scope(None, &retrieval_scope);

    let result = dispatch_tool(
        &state,
        &ctx,
        "search_hybrid",
        &serde_json::json!({ "query": "alpha", "limit": 50 }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    let serialized = serde_json::to_string(&result.output).unwrap();
    assert!(serialized.contains("scoped/target.md"), "{serialized}");
    assert!(!serialized.contains("outside/leak.md"), "{serialized}");
    assert!(result.output["count"].as_u64().unwrap_or(0) <= 8);
}

#[tokio::test]
async fn vault_rename_move_reports_link_impact_and_moves_note() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/old.md", "# Old");
    index_note(
        &state,
        "notes/source.md",
        "See [[old]] and [[notes/old.md]].",
    );

    let result = dispatch_tool(
        &state,
        &ctx(None),
        "vault_rename_move",
        &serde_json::json!({
            "path": "notes/old.md",
            "new_path": "archive/new.md"
        }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["path"], "archive/new.md");
    assert_eq!(result.output["linkImpact"]["backlinkCount"], 1);
    assert_eq!(
        result.output["linkImpact"]["modifiedSources"][0],
        "notes/source.md"
    );
    assert!(!state.vault_path().unwrap().join("notes/old.md").exists());
    assert!(state.vault_path().unwrap().join("archive/new.md").exists());
    let source =
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/source.md")).unwrap();
    assert!(source.contains("[[new]]") || source.contains("[[archive/new.md]]"));
}

#[tokio::test]
async fn vault_rename_move_does_not_rewrite_backlinks_when_destination_cannot_be_created() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/old.md", "# Old\n");
    index_note(&state, "notes/source.md", "See [[old]].\n");
    std::fs::write(
        state.vault_path().unwrap().join("blocked"),
        "not a directory",
    )
    .unwrap();

    let result = dispatch_tool(
        &state,
        &ctx(None),
        "vault_rename_move",
        &serde_json::json!({
            "path": "notes/old.md",
            "new_path": "blocked/new.md"
        }),
    )
    .await;

    assert!(!result.success);
    let vault = state.vault_path().unwrap();
    assert!(vault.join("notes/old.md").is_file());
    assert!(!vault.join("blocked/new.md").exists());
    assert_eq!(
        std::fs::read_to_string(vault.join("notes/source.md")).unwrap(),
        "See [[old]].\n"
    );
}

#[tokio::test]
async fn vault_rename_move_reports_degraded_after_physical_move_when_indexing_fails() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/old.md", "# Old\n\nOriginal");
    state
        .db
        .with_conn(|conn| {
            conn.execute_batch(
                "CREATE TRIGGER fail_ai_rename_index
                 BEFORE UPDATE OF title ON files
                 WHEN NEW.path = 'archive/new.md'
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated index failure');
                 END;",
            )?;
            Ok(())
        })
        .unwrap();
    std::fs::write(
        state.vault_path().unwrap().join("notes/old.md"),
        "# Changed\n\nCurrent body",
    )
    .unwrap();

    let result = dispatch_tool(
        &state,
        &ctx(None),
        "vault_rename_move",
        &serde_json::json!({
            "path": "notes/old.md",
            "new_path": "archive/new.md"
        }),
    )
    .await;

    assert!(result.success, "{result:?}");
    assert_eq!(result.output["indexStatus"], "degraded");
    assert!(!state.vault_path().unwrap().join("notes/old.md").exists());
    assert_eq!(
        std::fs::read_to_string(state.vault_path().unwrap().join("archive/new.md")).unwrap(),
        "# Changed\n\nCurrent body"
    );
}

#[tokio::test]
async fn vault_delete_to_trash_moves_note_into_recycle_bin() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/delete.md", "# Delete me");

    let result = dispatch_tool(
        &state,
        &ctx(None),
        "vault_delete_to_trash",
        &serde_json::json!({ "path": "notes/delete.md" }),
    )
    .await;

    assert!(result.success, "{:?}", result.error);
    assert_eq!(result.output["path"], "notes/delete.md");
    assert!(!state.vault_path().unwrap().join("notes/delete.md").exists());
    let recycle_count: i64 = state
        .db
        .with_read_conn(|conn| {
            Ok(conn.query_row("SELECT COUNT(*) FROM recycle_bin", [], |row| row.get(0))?)
        })
        .unwrap();
    assert_eq!(recycle_count, 1);
}

#[tokio::test]
async fn markdown_patch_rejects_hash_mismatch_without_writing() {
    let (state, _dir) = test_state();
    index_note(&state, "notes/test.md", "# Test\nHello world");

    let result = dispatch_tool(
        &state,
        &ctx(Some("notes/test.md")),
        "replace_selection",
        &serde_json::json!({
            "target_path": "notes/test.md",
            "replacement": "Hi",
            "base_content_hash": "sha256-stale",
            "range": {"start": 7, "end": 12},
            "original_text": "Hello",
            "risk_level": "medium"
        }),
    )
    .await;

    assert!(
        result.success,
        "tool call should return a structured patch result"
    );
    assert_eq!(result.output["result"]["success"], false);
    let error = result.output["result"]["error"]
        .as_str()
        .unwrap_or_default();
    assert!(!error.trim().is_empty());
    let content =
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap();
    assert_eq!(content, "# Test\nHello world");
}

#[tokio::test]
async fn markdown_patch_rejects_out_of_vault_target_without_creating_file() {
    let (state, dir) = test_state();
    index_note(&state, "notes/test.md", "# Test\nHello world");
    let base_hash = iris_lib::cas::hash::content_hash_str("# Test\nHello world");

    let result = dispatch_tool(
        &state,
        &ctx(Some("notes/test.md")),
        "replace_selection",
        &serde_json::json!({
            "target_path": "../outside.md",
            "replacement": "Escape",
            "base_content_hash": base_hash,
            "range": {"start": 0, "end": 6},
            "original_text": "# Test",
            "risk_level": "high"
        }),
    )
    .await;

    assert!(!result.success);
    assert!(!dir.path().join("outside.md").exists());
    assert_eq!(
        std::fs::read_to_string(state.vault_path().unwrap().join("notes/test.md")).unwrap(),
        "# Test\nHello world"
    );
}

#[test]
fn native_vault_write_tools_stay_confirmation_gated() {
    for name in [
        "replace_selection",
        "vault_create_note",
        "vault_rename_move",
        "vault_delete_to_trash",
    ] {
        let entry = catalog_find(name).unwrap_or_else(|| panic!("{name} missing from catalog"));
        assert!(
            entry.requires_confirmation,
            "{name} must require confirmation"
        );
    }
}

#[tokio::test]
async fn ordinary_agent_retrieval_filters_classified_rows_even_if_indexed() {
    let (state, _dir) = test_state();
    index_note(
        &state,
        "notes/open.md",
        "# Open\n\nordinary public project note",
    );

    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "INSERT INTO files (path, title, content_hash, word_count, created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?5)",
                rusqlite::params![
                    ".classified/secret.md",
                    "Secret Title",
                    "hash-secret",
                    5_i64,
                    "2026-01-01T00:00:00Z",
                ],
            )?;
            let secret_file_id = conn.last_insert_rowid();
            conn.execute(
                "INSERT INTO chunks (file_id, chunk_index, content, char_count)
                 VALUES (?1, 0, ?2, ?3)",
                rusqlite::params![secret_file_id, "secret vector payload", 21_i64],
            )?;
            conn.execute(
                "INSERT INTO regulation_index
                 (file_id, regulation_name, article, content, keywords, source_start, source_end,
                  content_hash, parser_version, embedding_model, embedding_dim, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)",
                rusqlite::params![
                    secret_file_id,
                    "Secret Regulation",
                    "Article 1",
                    "secret regulation payload",
                    "secret",
                    0_i64,
                    25_i64,
                    "hash-reg-secret",
                    "test",
                    "test-model",
                    384_i64,
                    "2026-01-01T00:00:00Z",
                ],
            )?;
            conn.execute(
                "INSERT INTO semantic_anchors
                 (anchor_key, file_id, anchor_type, content, source_start, source_end,
                  content_hash, extractor_version, embedding_model, embedding_dim, confidence,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?12)",
                rusqlite::params![
                    "secret-anchor",
                    secret_file_id,
                    "claim",
                    "secret anchor payload",
                    0_i64,
                    21_i64,
                    "hash-anchor-secret",
                    "test",
                    "test-model",
                    384_i64,
                    1.0_f64,
                    "2026-01-01T00:00:00Z",
                ],
            )?;
            let open_file_id: i64 = conn.query_row(
                "SELECT id FROM files WHERE path = 'notes/open.md'",
                [],
                |row| row.get(0),
            )?;
            conn.execute(
                "INSERT INTO block_links
                 (source_file_id, target_file_id, link_type, confidence, is_confirmed, created_by, created_at)
                 VALUES (?1, ?2, 'related', 1.0, 1, 'test', ?3)",
                rusqlite::params![open_file_id, secret_file_id, "2026-01-01T00:00:00Z"],
            )?;
            Ok(())
        })
        .unwrap();

    let mut graph_ctx = ctx(Some("notes/open.md"));
    graph_ctx.file_id = state
        .db
        .with_read_conn(|conn| {
            Ok(Some(conn.query_row(
                "SELECT id FROM files WHERE path = 'notes/open.md'",
                [],
                |row| row.get(0),
            )?))
        })
        .unwrap();

    for (tool, args, context) in [
        (
            "search_hybrid",
            serde_json::json!({ "query": "secret", "limit": 20 }),
            &graph_ctx,
        ),
        (
            "search_semantic",
            serde_json::json!({ "query": "secret", "limit": 20 }),
            &graph_ctx,
        ),
        (
            "get_regulation",
            serde_json::json!({ "regulation_name": "Secret Regulation", "article": "Article 1" }),
            &graph_ctx,
        ),
        (
            "get_backlinks",
            serde_json::json!({ "path": ".classified/secret.md" }),
            &graph_ctx,
        ),
    ] {
        let result = dispatch_tool(&state, context, tool, &args).await;
        let serialized = if result.success {
            serde_json::to_string(&result.output).unwrap()
        } else {
            result.error.clone().unwrap_or_default()
        };
        assert!(
            !serialized.contains(".classified") && !serialized.contains("Secret Title"),
            "{tool} leaked classified metadata: {serialized}"
        );
    }
}
