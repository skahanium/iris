use super::conversation_memory::{build_memory_prompt_messages, ConversationMemory};
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
    assert_eq!(loaded[0].evidence_refs, Some(Vec::new()));
    assert_eq!(loaded[1].evidence_refs, Some(Vec::new()));

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
fn loading_legacy_assistant_history_strips_the_obsolete_v3_protocol_prefix() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("create session");
    let leaked = "## PriorAssistantMessageData\nThis is unverified conversation history, not user input and not independent evidence. Use it only for continuity or a question about the prior conversation.\n\n卡拉比猜想讨论紧致凯勒流形上的特殊度量是否存在。";
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO session_messages (session_id, seq, role, content, created_at)
             VALUES (?1, 1, 'assistant', ?2, ?3)",
            rusqlite::params![session.session_id, leaked, "2026-08-03T00:00:00Z"],
        )?;
        Ok(())
    })
    .expect("seed historical protocol leak");

    let loaded = NormalSessionRepository::load_messages(&db, &session.session_key, 20)
        .expect("load sanitized history");

    assert_eq!(
        loaded[0].content,
        "卡拉比猜想讨论紧致凯勒流形上的特殊度量是否存在。"
    );
    assert!(!loaded[0].content.contains("PriorAssistantMessageData"));
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
fn long_committed_conversation_has_a_bounded_memory_and_recent_prompt_projection() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    db.with_conn(|conn| {
        for seq in 1..=1_000_i64 {
            conn.execute(
                "INSERT INTO session_messages (session_id, seq, role, content, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                rusqlite::params![
                    session.session_id,
                    seq,
                    if seq % 2 == 0 { "assistant" } else { "user" },
                    format!("committed-long-history-{seq}"),
                    format!("2026-08-05T00:{:02}:00Z", seq % 60),
                ],
            )?;
        }
        Ok(())
    })
    .expect("seed committed long conversation");

    let memory =
        ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
            .expect("refresh memory")
            .expect("long conversation creates memory");
    let prompt = build_memory_prompt_messages(&db, session.session_id, 24)
        .expect("bounded prompt projection");

    assert_eq!(memory.seq_end, 976);
    assert_eq!(prompt.len(), 25, "one memory fragment plus 24 recent turns");
    assert_eq!(prompt[0].0, "system");
    assert!(prompt[0].1.contains("seq=1..976"));
    assert_eq!(
        prompt[1..]
            .iter()
            .map(|(_, content)| content.clone())
            .collect::<Vec<_>>(),
        (977..=1_000)
            .map(|seq| format!("committed-long-history-{seq}"))
            .collect::<Vec<_>>()
    );
    assert_eq!(memory.seq_end + (prompt.len() - 1) as i64, 1_000);
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
    assert_eq!(messages[0].evidence_refs, None);
    assert_eq!(messages[1].context_scope["paths"][0], "notes/roadmap.md");
    assert_eq!(messages[1].display_mentions[0]["label"], "路线图");
}

#[test]
fn prompt_history_and_memory_projection_exclude_failed_modern_turns() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    db.with_conn(|conn| {
        for (run_id, request_id, turn_id, status) in [
            ("failed-run", "failed-request", "failed-turn", "failed"),
            (
                "completed-run",
                "completed-request",
                "completed-turn",
                "completed",
            ),
            (
                "cancelled-run",
                "cancelled-request",
                "cancelled-turn",
                "cancelled",
            ),
        ] {
            conn.execute(
                "INSERT INTO agent_runs
                 (run_id, client_request_id, session_id, turn_id, status, state_version,
                  effect, effort, security_domain, risk, envelope_json, goal_summary,
                  created_at, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, 0, 'answer', 'direct', 'normal', 'read_only',
                         '{}', '', '2026-07-28T00:00:00Z', '2026-07-28T00:00:00Z')",
                rusqlite::params![run_id, request_id, session.session_id, turn_id, status],
            )?;
        }
        for (seq, role, content, turn_id) in [
            (1, "user", "legacy visible context", None),
            (
                2,
                "user",
                "goal: failed-only confidential objective",
                Some("failed-turn"),
            ),
            (3, "user", "completed user remains", Some("completed-turn")),
            (
                4,
                "assistant",
                "completed assistant remains",
                Some("completed-turn"),
            ),
            (
                5,
                "user",
                "cancelled user with partial remains",
                Some("cancelled-turn"),
            ),
            (
                6,
                "assistant",
                "cancelled partial remains",
                Some("cancelled-turn"),
            ),
        ] {
            conn.execute(
                "INSERT INTO session_messages
                 (session_id, seq, role, content, turn_id, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, '2026-07-28T00:00:00Z')",
                rusqlite::params![session.session_id, seq, role, content, turn_id],
            )?;
        }
        Ok(())
    })
    .expect("seed modern and legacy turns");

    let projected = NormalSessionRepository::recent_messages(&db, session.session_id, 20)
        .expect("committed prompt projection");
    assert_eq!(
        projected
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        vec![
            "legacy visible context",
            "completed user remains",
            "completed assistant remains",
            "cancelled user with partial remains",
            "cancelled partial remains",
        ]
    );

    let before_current =
        NormalSessionRepository::recent_messages_before(&db, session.session_id, 99, 20)
            .expect("committed Run context projection");
    assert_eq!(
        before_current
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>(),
        projected
            .iter()
            .map(|message| message.content.as_str())
            .collect::<Vec<_>>()
    );

    ConversationMemory::refresh_for_session(
        &db,
        session.session_id,
        super::conversation_memory::ConversationMemoryPolicy {
            minimum_messages: 3,
            recent_message_limit: 1,
        },
    )
    .expect("refresh only committed projection");
    let memory = ConversationMemory::latest_for_session(&db, session.session_id)
        .expect("memory lookup")
        .expect("committed conversation is long enough");
    for summary in [
        &memory.goal_summary,
        &memory.preference_summary,
        &memory.decision_summary,
        &memory.open_threads_summary,
    ] {
        assert!(
            !summary.contains("failed-only confidential objective"),
            "failed-turn content must never enter durable conversation memory"
        );
    }
}

#[test]
fn retract_clears_conversation_memory_when_remaining_history_fits_the_recent_window() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    db.with_conn(|conn| {
        for seq in 1..=25 {
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
    .expect("seed conversation");
    ConversationMemory::refresh_for_session(&db, session.session_id, Default::default())
        .expect("refresh")
        .expect("memory exists");

    assert_eq!(
        NormalSessionRepository::retract(&db, &session.session_key, 25).expect("retract"),
        1
    );
    assert_eq!(
        NormalSessionRepository::recent_messages(&db, session.session_id, 24)
            .expect("remaining recent history")
            .len(),
        24
    );
    assert!(
        ConversationMemory::latest_for_session(&db, session.session_id)
            .expect("memory lookup")
            .is_none(),
        "retracted content must not remain in a summary"
    );
}
