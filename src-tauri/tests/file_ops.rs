use iris_lib::app::AppState;
use iris_lib::indexer::scan::{
    content_hash, index_file_from_content, scan_vault, IndexEmbeddingMode,
};
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

#[test]
fn index_file_from_content_updates_all_derived_rows_immediately() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();

    let target = vault.join("Target.md");
    let source = vault.join("source.md");
    fs::write(&target, "---\ntitle: \"Target\"\n---\n\nTarget body.").unwrap();
    let initial = "---\ntitle: \"Source\"\n---\n\nSee [[Target]].\n\n![old](old.png)\n\nOld body.";
    fs::write(&source, initial).unwrap();

    let data = dir.path().join("data");
    let state = AppState::new(data).unwrap();
    state.set_vault(vault.clone()).unwrap();

    state
        .db
        .with_conn(|conn| {
            scan_vault(conn, &vault)?;
            Ok(())
        })
        .unwrap();

    let updated = "---\ntitle: \"Source\"\n---\n\nNo links now.\n\n![new](new.png)\n\nNew body.";
    fs::write(&source, updated).unwrap();
    let updated_hash = content_hash(updated);
    let entry = state
        .db
        .with_conn(|conn| {
            index_file_from_content(
                conn,
                &vault,
                &source,
                updated,
                &updated_hash,
                IndexEmbeddingMode::Queue(&state),
            )
        })
        .unwrap();

    state
        .db
        .with_read_conn(|conn| {
            let stored_hash: String = conn.query_row(
                "SELECT content_hash FROM files WHERE path = 'source.md'",
                [],
                |row| row.get(0),
            )?;
            assert_eq!(stored_hash, updated_hash);

            let chunk_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM chunks WHERE file_id = ?1 AND content LIKE '%New body%'",
                [entry.id],
                |row| row.get(0),
            )?;
            assert_eq!(chunk_count, 1);

            let link_count: i64 = conn.query_row(
                "SELECT COUNT(*) FROM links WHERE source_id = ?1",
                [entry.id],
                |row| row.get(0),
            )?;
            assert_eq!(link_count, 0);

            let old_images: i64 = conn.query_row(
                "SELECT COUNT(*) FROM image_refs WHERE source_id = ?1 AND image_path = 'old.png'",
                [entry.id],
                |row| row.get(0),
            )?;
            assert_eq!(old_images, 0);

            let new_images: i64 = conn.query_row(
                "SELECT COUNT(*) FROM image_refs WHERE source_id = ?1 AND image_path = 'new.png'",
                [entry.id],
                |row| row.get(0),
            )?;
            assert_eq!(new_images, 1);

            Ok(())
        })
        .unwrap();
}
