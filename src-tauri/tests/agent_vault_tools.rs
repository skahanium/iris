use iris_lib::ai_runtime::tool_catalog::catalog_find;
use iris_lib::ai_runtime::tool_dispatch::{
    dispatch_tool, ToolDispatchContext, DISPATCHABLE_TOOL_NAMES,
};
use iris_lib::ai_runtime::{writing_workflow, AiScene};
use iris_lib::app::AppState;

fn test_state() -> (std::sync::Arc<AppState>, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(&vault).unwrap();
    let state = AppState::new(dir.path().to_path_buf()).unwrap();
    state.set_vault(vault).unwrap();
    (state, dir)
}

fn ctx<'a>(note_path: Option<&'a str>) -> ToolDispatchContext<'a> {
    ToolDispatchContext {
        scene: AiScene::DraftingAssist,
        note_path,
        file_id: None,
        web_search_enabled: false,
        cold_start_packets: &[],
        app_handle: None,
        attachment_count: 0,
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
            iris_lib::indexer::scan::index_file_from_content(
                conn,
                &vault,
                &abs,
                content,
                &hash,
                Some(state),
            )
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
    let base_hash = writing_workflow::compute_content_hash(base);
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
