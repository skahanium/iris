use iris_lib::app::AppState;
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
