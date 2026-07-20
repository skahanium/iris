use std::fs;
use std::sync::{Arc, Barrier};
use std::thread;

use tempfile::tempdir;

use crate::app::AppState;
use crate::commands::file::file_rename_inner;
use crate::indexer::scan::index_file;
use crate::storage::note_write::{FileWriteIndexStatus, NoteWriteService};

#[test]
fn preserves_markdown_and_returns_degraded_when_index_refresh_fails() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let note = vault.join("note.md");
    fs::write(&note, "---\ntitle: Old\n---\n\nOld").expect("seed note");
    let state = AppState::new(directory.path().join("data")).expect("application state");
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

#[test]
fn write_after_trash_recreates_note_without_ghost_at_old_path() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("set vault");
    NoteWriteService::write(&state, "race.md", "body before trash").expect("seed note");

    crate::recycle::trash_document(&state, "race.md").expect("trash note");

    let original = vault.join("race.md");
    assert!(
        !original.exists(),
        "trashed note must not remain at the original vault path"
    );
    NoteWriteService::write(&state, "race.md", "body after trash")
        .expect("write after trash recreates the note");
    assert_eq!(
        fs::read_to_string(original).expect("recreated note"),
        "body after trash"
    );
}

#[test]
fn concurrent_rename_and_write_never_leave_corrupt_or_half_moved_note() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("set vault");
    NoteWriteService::write(&state, "race.md", "seed body").expect("seed note");

    let barrier = Arc::new(Barrier::new(2));
    let rename_state = Arc::clone(&state);
    let write_state = Arc::clone(&state);
    let rename_barrier = Arc::clone(&barrier);
    let write_barrier = Arc::clone(&barrier);

    let rename_thread = thread::spawn(move || {
        rename_barrier.wait();
        file_rename_inner(
            rename_state,
            "race.md".to_string(),
            "renamed.md".to_string(),
        )
    });
    let write_thread = thread::spawn(move || {
        write_barrier.wait();
        // Amplify interleaving: many short writes racing rename.
        let mut last = Ok(());
        for i in 0..20 {
            last = NoteWriteService::write(&write_state, "race.md", &format!("concurrent-{i}"))
                .map(|_| ());
        }
        last
    });

    let rename_result = rename_thread.join().expect("rename thread");
    let write_result = write_thread.join().expect("write thread");

    let race_path = vault.join("race.md");
    let renamed_path = vault.join("renamed.md");
    let race_exists = race_path.exists();
    let renamed_exists = renamed_path.exists();
    assert!(
        race_exists || renamed_exists,
        "at least one of the race/renamed paths must survive"
    );

    if renamed_exists {
        let content = fs::read_to_string(&renamed_path).expect("renamed content");
        assert!(
            content == "seed body" || content.starts_with("concurrent-"),
            "renamed.md must contain seed or a complete write body, got {content:?}"
        );
        rename_result.expect("rename should succeed when destination exists");
    }
    if race_exists {
        let content = fs::read_to_string(&race_path).expect("race content");
        assert!(
            content == "seed body" || content.starts_with("concurrent-"),
            "race.md must contain seed or a complete write body, got {content:?}"
        );
        // A surviving race.md after a successful rename is a post-rename recreate,
        // which requires the write path to have completed successfully.
        if renamed_exists {
            write_result.expect("post-rename recreate write must succeed");
        }
    }
    // Never leave the unique temporary sibling that atomic_write uses mid-flight.
    let leftovers: Vec<_> = fs::read_dir(&vault)
        .expect("vault dir")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp") || name.contains("iris-tmp"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temporary write artifacts should remain: {leftovers:?}"
    );
}

#[test]
fn concurrent_trash_and_write_serialize_without_lost_body() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("set vault");
    NoteWriteService::write(&state, "race.md", "seed body").expect("seed note");

    let barrier = Arc::new(Barrier::new(2));
    let trash_state = Arc::clone(&state);
    let write_state = Arc::clone(&state);
    let trash_barrier = Arc::clone(&barrier);
    let write_barrier = Arc::clone(&barrier);

    let trash_thread = thread::spawn(move || {
        trash_barrier.wait();
        crate::recycle::trash_document(&trash_state, "race.md")
    });
    let write_thread = thread::spawn(move || {
        write_barrier.wait();
        NoteWriteService::write(&write_state, "race.md", "writer body")
    });

    let trash_result = trash_thread.join().expect("trash thread");
    let write_result = write_thread.join().expect("write thread");
    assert!(
        trash_result.is_ok() || write_result.is_ok(),
        "at least one of trash/write must succeed under serialization"
    );

    let race_path = vault.join("race.md");
    if race_path.exists() {
        assert_eq!(
            fs::read_to_string(&race_path).expect("race content"),
            "writer body",
            "a surviving race.md must be a complete post-lock write"
        );
        write_result.expect("surviving race.md implies write succeeded");
    } else {
        trash_result.expect("missing race.md implies trash succeeded");
    }
}

#[test]
fn write_locked_file_returns_note_locked_error() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("set vault");
    NoteWriteService::write(&state, "locked.md", "seed").expect("seed note");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE files SET is_locked = 1 WHERE path = 'locked.md'",
                [],
            )?;
            Ok(())
        })
        .expect("lock note");

    let err = NoteWriteService::write(&state, "locked.md", "should fail")
        .expect_err("locked write must fail");
    assert!(
        err.to_string().contains("note_locked"),
        "expected note_locked, got {err}"
    );
    assert_eq!(
        fs::read_to_string(vault.join("locked.md")).expect("disk unchanged"),
        "seed"
    );
}

#[test]
fn write_under_move_lock_bypass_allows_locked_note() {
    let directory = tempdir().expect("temporary directory");
    let vault = directory.path().join("vault");
    fs::create_dir_all(&vault).expect("vault directory");
    let state = AppState::new(directory.path().join("data")).expect("application state");
    state.set_vault(vault.clone()).expect("set vault");
    NoteWriteService::write(&state, "locked.md", "seed").expect("seed note");
    state
        .db
        .with_conn(|conn| {
            conn.execute(
                "UPDATE files SET is_locked = 1 WHERE path = 'locked.md'",
                [],
            )?;
            Ok(())
        })
        .expect("lock note");

    crate::storage::atomic_write::with_vault_move_lock(|| {
        NoteWriteService::write_under_move_lock(&state, "locked.md", "cascade rewrite", true)
    })
    .expect("bypass must allow locked cascade rewrite");
    assert_eq!(
        fs::read_to_string(vault.join("locked.md")).expect("rewritten"),
        "cascade rewrite"
    );
}
