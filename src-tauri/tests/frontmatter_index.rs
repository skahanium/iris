use iris_lib::app::AppState;
use iris_lib::error::AppResult;
use iris_lib::indexer::scan::{index_file, remove_file_index, scan_vault};
use iris_lib::storage::migrate::migrate_up;
use rusqlite::Connection;
use std::fs;
use tempfile::tempdir;

fn tag_names(conn: &Connection, file_id: i64) -> Vec<String> {
    let mut stmt = conn
        .prepare(
            "SELECT t.name FROM tags t
             INNER JOIN file_tags ft ON ft.tag_id = t.id
             WHERE ft.file_id = ?1
             ORDER BY t.name",
        )
        .unwrap();
    stmt.query_map([file_id], |row| row.get(0))
        .unwrap()
        .flatten()
        .collect()
}

#[test]
fn indexes_frontmatter_and_tags() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    let note = vault.join("meet.md");
    fs::write(
        &note,
        "---\ntitle: 会议记录\ntags: [工作, iris]\n---\n\n# Hello\n",
    )
    .unwrap();

    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    index_file(&conn, &vault, &note).unwrap();

    let (fm, title, file_id): (Option<String>, String, i64) = conn
        .query_row(
            "SELECT frontmatter, title, id FROM files WHERE path = 'meet.md'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
        )
        .unwrap();

    assert!(fm.is_some());
    assert!(fm.unwrap().contains("tags"));
    assert_eq!(title, "会议记录");
    assert_eq!(tag_names(&conn, file_id), vec!["iris", "工作"]);
}

#[test]
fn no_frontmatter_leaves_frontmatter_null() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    let note = vault.join("plain.md");
    fs::write(&note, "# Plain\n\nText.").unwrap();

    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    index_file(&conn, &vault, &note).unwrap();

    let fm: Option<String> = conn
        .query_row(
            "SELECT frontmatter FROM files WHERE path = 'plain.md'",
            [],
            |row| row.get(0),
        )
        .unwrap();
    assert!(fm.is_none());

    let file_id: i64 = conn
        .query_row("SELECT id FROM files WHERE path = 'plain.md'", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert!(tag_names(&conn, file_id).is_empty());
}

#[test]
fn reindex_updates_tags() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    let note = vault.join("t.md");
    fs::write(&note, "---\ntags: [old]\n---\n").unwrap();

    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    index_file(&conn, &vault, &note).unwrap();

    fs::write(&note, "---\ntags: [new, fresh]\n---\n").unwrap();
    index_file(&conn, &vault, &note).unwrap();

    let file_id: i64 = conn
        .query_row("SELECT id FROM files WHERE path = 't.md'", [], |r| r.get(0))
        .unwrap();
    assert_eq!(tag_names(&conn, file_id), vec!["fresh", "new"]);
}

#[test]
fn delete_file_cascades_file_tags() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    let note = vault.join("gone.md");
    fs::write(&note, "---\ntags: [x]\n---\n").unwrap();

    let conn = Connection::open_in_memory().unwrap();
    migrate_up(&conn).unwrap();
    index_file(&conn, &vault, &note).unwrap();

    let links: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(links, 1);

    remove_file_index(&conn, "gone.md").unwrap();

    let links: i64 = conn
        .query_row("SELECT COUNT(*) FROM file_tags", [], |r| r.get(0))
        .unwrap();
    assert_eq!(links, 0);
}

#[test]
fn scan_vault_indexes_multiple_notes() {
    let dir = tempdir().unwrap();
    let vault = dir.path().join("vault");
    fs::create_dir_all(&vault).unwrap();
    fs::write(vault.join("a.md"), "---\ntags: [a]\n---\n").unwrap();
    fs::write(vault.join("b.md"), "no fm\n").unwrap();

    let data = dir.path().join("data");
    let state = AppState::new(data).unwrap();
    state.set_vault(vault.clone()).unwrap();

    let entries = state.db.with_conn(|conn| scan_vault(conn, &vault)).unwrap();
    assert_eq!(entries.len(), 2);

    state
        .db
        .with_conn(|conn: &Connection| -> AppResult<()> {
            let with_fm: i64 = conn.query_row(
                "SELECT COUNT(*) FROM files WHERE frontmatter IS NOT NULL",
                [],
                |r| r.get(0),
            )?;
            assert_eq!(with_fm, 1);
            Ok(())
        })
        .unwrap();
}
