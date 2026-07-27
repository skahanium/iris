use super::conversation_memory::ConversationMemory;
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
    assert_eq!(loaded[0].turn_id.as_deref(), Some("history-turn-1"));
    assert_eq!(loaded[1].turn_id.as_deref(), Some("history-turn-1"));

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

#[test]
fn normal_session_history_loads_the_latest_240_messages_in_chronological_order() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("create session");
    db.with_conn(|conn| {
        for seq in 1..=260_i64 {
            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    session.session_id,
                    seq,
                    if seq % 2 == 0 { "assistant" } else { "user" },
                    format!("message-{seq}"),
                    format!("2026-07-27T00:{:02}:00Z", seq % 60),
                ],
            )?;
        }
        Ok(())
    })
    .expect("seed long conversation");

    let messages = NormalSessionRepository::load_messages(&db, &session.session_key, 240)
        .expect("load bounded long conversation");

    assert_eq!(messages.len(), 240);
    assert_eq!(messages.first().expect("first").seq, 21);
    assert_eq!(messages.last().expect("last").seq, 260);
    assert!(messages.windows(2).all(|pair| pair[0].seq < pair[1].seq));
}

#[test]
fn normal_session_history_restores_new_turn_metadata_and_defaults_legacy_rows_to_arrays() {
    let db = Database::open_in_memory().expect("database");
    let created = NormalSessionRepository::create(&db).expect("create session");
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, created_at)
             VALUES (?1, 1, 'user', 'legacy', ?2)",
            rusqlite::params![created.session_id, "2026-07-13T00:00:00Z"],
        )?;
        conn.execute(
            "INSERT INTO session_messages
             (session_id, seq, role, content, context_scope_json,
              display_mentions_json, created_at)
             VALUES (?1, 2, 'user', '分析 路线图', ?2, ?3, ?4)",
            rusqlite::params![
                created.session_id,
                r#"{"paths":["notes/roadmap.md"],"pathPrefixes":[],"corpusIds":[],"requiredTags":[]}"#,
                r#"[{"kind":"file","value":"notes/roadmap.md","label":"路线图","range":{"from":3,"to":6}}]"#,
                "2026-07-13T00:00:01Z",
            ],
        )?;
        Ok(())
    })
    .expect("seed history");

    let messages = NormalSessionRepository::load_messages(&db, &created.session_key, 20)
        .expect("load history");
    assert_eq!(messages[0].context_scope, serde_json::json!([]));
    assert!(messages[0].display_mentions.is_empty());
    assert_eq!(messages[1].context_scope["paths"][0], "notes/roadmap.md");
    assert_eq!(messages[1].display_mentions[0]["label"], "路线图");
}

#[test]
fn retract_clears_conversation_memory_when_remaining_history_fits_the_recent_window() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    db.with_conn(|conn| {
        for seq in 1..=7 {
            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    session.session_id,
                    seq,
                    if seq % 2 == 0 { "assistant" } else { "user" },
                    format!("message-{seq}"),
                    format!("2026-07-27T00:00:0{seq}Z"),
                ],
            )?;
        }
        Ok(())
    })
    .expect("seed conversation");
    ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
        .expect("refresh")
        .expect("memory exists");

    assert_eq!(
        NormalSessionRepository::retract(&db, &session.session_key, 7).expect("retract"),
        1
    );
    assert!(
        ConversationMemory::latest_for_session(&db, session.session_id)
            .expect("memory lookup")
            .is_none(),
        "retracted content must not remain in a summary"
    );
}
