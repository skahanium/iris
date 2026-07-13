use super::normal_session_repository::NormalSessionRepository;
use crate::storage::db::Database;

#[test]
fn normal_session_is_created_and_resolved_without_scene_or_note_binding() {
    let db = Database::open_in_memory().expect("database");

    let created = NormalSessionRepository::create(&db).expect("create normal session");
    let resolved = NormalSessionRepository::get(&db, &created.session_key)
        .expect("get normal session")
        .expect("session exists");

    assert_eq!(resolved, created);
    assert!(created.session_key.starts_with("run_session:"));
    db.with_read_conn(|conn| {
        let (vault_id, title): (Option<String>, Option<String>) = conn.query_row(
            "SELECT vault_id, title FROM sessions WHERE id = ?1",
            [created.session_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert!(
            vault_id.is_none(),
            "new normal session must not bind a vault"
        );
        assert!(
            title.is_none(),
            "new normal session must not synthesize a target title"
        );
        Ok(())
    })
    .expect("unbound session facts");
}

#[test]
fn normal_session_repository_resolves_an_opaque_persisted_key() {
    let db = Database::open_in_memory().expect("database");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sessions (session_key, vault_id, title, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?4)",
            rusqlite::params![
                "persisted-session-key",
                "vault-1",
                "saved conversation",
                "2026-07-13T00:00:00Z",
            ],
        )?;
        Ok(())
    })
    .expect("seed persisted session");

    let resolved = NormalSessionRepository::get(&db, "persisted-session-key")
        .expect("resolve session")
        .expect("session remains readable");

    assert_eq!(resolved.session_key, "persisted-session-key");
}

#[test]
fn normal_session_history_uses_only_opaque_keys() {
    let db = Database::open_in_memory().expect("database");
    let created = NormalSessionRepository::create(&db).expect("create session");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, turn_id, evidence_refs_json, created_at)
             VALUES (?1, 1, 'user', 'first', 'history-turn-1', '[]', ?2)",
            rusqlite::params![created.session_id, "2026-07-13T00:00:00Z"],
        )?;
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, turn_id, evidence_refs_json, created_at)
             VALUES (?1, 2, 'assistant', 'second', 'history-turn-1', '[]', ?2)",
            rusqlite::params![created.session_id, "2026-07-13T00:00:01Z"],
        )?;
        Ok(())
    })
    .expect("seed run-owned message history");

    let listed = NormalSessionRepository::list(&db, 20, 0).expect("list sessions");
    assert!(listed
        .iter()
        .any(|item| item.session_key == created.session_key));
    assert!(listed
        .iter()
        .all(|item| !item.session_key.contains("drafting")));

    let loaded = NormalSessionRepository::load_messages(&db, &created.session_key, 20)
        .expect("load messages");
    assert_eq!(loaded.len(), 2);
    assert_eq!(loaded[0].content, "first");

    NormalSessionRepository::rename(&db, &created.session_key, "renamed").expect("rename");
    let renamed = NormalSessionRepository::list(&db, 20, 0).expect("list renamed");
    assert_eq!(
        renamed
            .iter()
            .find(|item| item.session_key == created.session_key)
            .expect("session summary")
            .title,
        "renamed"
    );

    assert_eq!(
        NormalSessionRepository::retract(&db, &created.session_key, 2).expect("retract"),
        1
    );
    assert!(NormalSessionRepository::delete(&db, &created.session_key).expect("delete"));
}
