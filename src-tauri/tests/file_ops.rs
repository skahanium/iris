use iris_lib::app::AppState;
use iris_lib::indexer::scan::scan_vault;
use iris_lib::storage::paths::resolve_vault_path;
use std::fs;
use tempfile::tempdir;

#[test]
fn vault_index_and_read() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    let note = vault.join("hello.md");
    fs::write(&note, "# Hello\n\nWorld.").unwrap();

    let data = dir.path().join("data");
    let state = AppState::new(data).unwrap();
    state.set_vault(vault.clone()).unwrap();

    let entries = state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].path, "hello.md");

    let resolved = resolve_vault_path(&vault, "hello.md").unwrap();
    let content = fs::read_to_string(resolved).unwrap();
    assert!(content.contains("Hello"));
}
