use std::fs;
use std::sync::Arc;

use tempfile::tempdir;

use crate::app::AppState;
use crate::indexer::scan::index_file;
use crate::storage::note_write::{FileWriteIndexStatus, NoteWriteService};

#[test]
fn preserves_markdown_and_returns_degraded_when_index_refresh_fails() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let note = vault.join("note.md");
    fs::write(&note, "---\ntitle: Old\n---\n\nOld").expect("seed note");
    let state = Arc::new(AppState::new(directory.path().join("data")).expect("application state"));
    state.set_vault(vault.clone()).expect("set vault");
    state
        .db
        .with_conn(|conn| {
            index_file(conn, &vault, &note)?;
            conn.execute_batch(
                "CREATE TRIGGER fail_note_write_index
                 BEFORE UPDATE OF title ON files
                 WHEN NEW.path = 'note.md'
                 BEGIN
                   SELECT RAISE(ABORT, 'simulated index failure');
                 END;",
            )?;
            Ok(())
        })
        .expect("install failing index trigger");
    let body = "---\ntitle: New\n---\n\nPersist even if indexing is unavailable";

    let result =
        NoteWriteService::write(&state, "note.md", body).expect("authoritative markdown write");

    assert_eq!(result.index_status, FileWriteIndexStatus::Degraded);
    assert_eq!(fs::read_to_string(note).expect("persisted note"), body);
}
