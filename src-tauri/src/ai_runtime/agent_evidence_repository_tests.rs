use super::agent_evidence_repository::{
    AgentEvidenceRepository, ExternalToolEvidenceInput, LocalEvidenceInput, MaterialRole,
    WebEvidenceInput,
};
use super::agent_run_repository::{AcceptRunInput, AgentRunRepository, AppendRunEventInput};
use super::normal_session_repository::NormalSessionRepository;
use super::run_contract::{
    ContextMode, Effect, Effort, ExecutionEnvelope, ExplicitConstraint, Freshness, MaterialNeed,
    Modality, RiskClass, RunEventPayload, RunEventType, SecurityDomain, WebDecisionReason,
};
use crate::storage::db::Database;

fn accept_test_run(
    db: &Database,
    session_id: i64,
    session_key: &str,
    client_request_id: &str,
    run_id: &str,
    turn_id: &str,
    message: &str,
) {
    AgentRunRepository::accept(
        db,
        AcceptRunInput {
            session_id,
            session_key: session_key.to_string(),
            client_request_id: client_request_id.to_string(),
            run_id: run_id.to_string(),
            turn_id: turn_id.to_string(),
            message: message.to_string(),
            content_parts: None,
            explicit_references: vec![],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effect: Effect::Answer,
                context: ContextMode::ExplicitReferences,
                freshness: Freshness::WebPreferred,
                web_reason: WebDecisionReason::LegacyUnknown,
                verification_requirement:
                    crate::ai_runtime::run_contract::VerificationRequirement::None,
                effort: Effort::ToolLoop,
                security_domain: SecurityDomain::Normal,
                risk: RiskClass::ReadOnly,
                modalities: vec![Modality::Text],
                material_needs: vec![MaterialNeed::Reference, MaterialNeed::Web],
                required_capabilities: vec![],
                explicit_constraints: vec![ExplicitConstraint {
                    kind: "no_implicit_context".to_string(),
                    value: None,
                }],
                fresh_fact: Default::default(),
            },
        },
    )
    .expect("accepted run");
}

fn setup_run() -> (Database, i64, String) {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("normal session");
    let session_id = session.session_id;
    let session_key = session.session_key;
    accept_test_run(
        &db,
        session_id,
        &session_key,
        "evidence-client-request",
        "evidence-run",
        "evidence-turn",
        "为证据账本建立可追溯运行",
    );
    (db, session_id, session_key)
}

fn cancel_test_run(db: &Database, run_id: &str) {
    AgentRunRepository::append_event(
        db,
        AppendRunEventInput {
            run_id: run_id.to_string(),
            state_version: 0,
            event_type: RunEventType::Cancelled,
            payload: RunEventPayload::Cancelled {
                reason: "test_fixture_finished".to_string(),
            },
        },
    )
    .expect("cancel the first run before accepting the next turn");
}

fn register_web_evidence(
    db: &Database,
    session_id: i64,
    run_id: &str,
    title: &str,
    url: &str,
) -> crate::ai_runtime::agent_evidence_repository::RegisteredEvidence {
    AgentEvidenceRepository::register_web(
        db,
        WebEvidenceInput {
            session_id,
            run_id: run_id.to_string(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: title.to_string(),
            url: url.to_string(),
            normalized_url: url.to_string(),
            domain: url
                .trim_start_matches("https://")
                .split('/')
                .next()
                .unwrap_or("example.test")
                .to_string(),
            retrieved_at: "2026-07-13T00:00:00Z".to_string(),
            provider_id: "official-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: format!("web-result-{run_id}-{url}"),
            extraction_method: "article_quote".to_string(),
            bounded_excerpt: "bounded excerpt".to_string(),
            retrieval_reason: Some("required_web_fact".to_string()),
            score: Some(0.91),
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("web evidence")
}

#[test]
fn local_evidence_is_bound_to_its_normal_run_and_never_persists_a_body() {
    let (db, session_id, _) = setup_run();

    let evidence = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id,
            run_id: "evidence-run".to_string(),
            message_seq_first: 1,
            material_role: MaterialRole::Authority,
            title: "会议制度".to_string(),
            source_path: "policies/meeting.md".to_string(),
            source_span_start: 12,
            source_span_end: 48,
            heading_path: Some("第三章/会议规则".to_string()),
            content_hash: "note-content-hash".to_string(),
            retrieval_reason: Some("explicit_reference".to_string()),
            score: Some(0.98),
        },
    )
    .expect("local evidence");

    assert_eq!(evidence.evidence_id, 1);
    assert_eq!(evidence.reference.display_label, "[C1]");
    assert!(!evidence.reference.stale);
    let returned = serde_json::to_string(&evidence.reference).expect("safe reference JSON");
    assert!(!returned.contains("policies/meeting.md"));
    assert!(!returned.contains("note-content-hash"));

    db.with_read_conn(|conn| {
        let row: (String, String, Option<String>, i64) = conn.query_row(
            "SELECT origin_run_id, material_role, bounded_excerpt, stale
             FROM session_evidence WHERE id = 1",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(row.0, "evidence-run");
        assert_eq!(row.1, "authority");
        assert_eq!(row.2, None);
        assert_eq!(row.3, 0);
        Ok(())
    })
    .expect("stored local evidence metadata");

    let summary = AgentEvidenceRepository::source_summary_for_current_run(
        &db,
        "evidence-run",
        &[evidence.evidence_id],
    )
    .expect("source summary");
    assert!(summary
        .entries()
        .iter()
        .any(|entry| entry.category == "authorized_material" && entry.count == 1));
}

#[test]
fn web_evidence_persists_only_a_bounded_excerpt_and_returns_a_safe_reference() {
    let (db, session_id, _) = setup_run();
    let excerpt = "监管机构页面明确了会议记录最低保留期限。";

    let evidence = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id,
            run_id: "evidence-run".to_string(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "官方规范".to_string(),
            url: "https://example.test/rules".to_string(),
            normalized_url: "https://example.test/rules".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-13T00:00:00Z".to_string(),
            provider_id: "official-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "web-result-hash".to_string(),
            extraction_method: "article_quote".to_string(),
            bounded_excerpt: excerpt.to_string(),
            retrieval_reason: Some("required_web_fact".to_string()),
            score: Some(0.91),
            source_rank: Some(1),
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect("web evidence");

    let returned = serde_json::to_string(&evidence.reference).expect("safe reference JSON");
    assert!(!returned.contains(excerpt));
    assert!(!returned.contains("https://example.test/rules"));

    db.with_read_conn(|conn| {
        let stored: String = conn.query_row(
            "SELECT bounded_excerpt FROM session_evidence WHERE id = 1",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(stored, excerpt);
        Ok(())
    })
    .expect("bounded Web excerpt");
    assert!(AgentEvidenceRepository::has_current_run_web_evidence(
        &db,
        "evidence-run",
        &[evidence.evidence_id]
    )
    .expect("current run Web evidence"));
    assert!(!AgentEvidenceRepository::has_current_run_web_evidence(
        &db,
        "another-run",
        &[evidence.evidence_id]
    )
    .expect("a different Run cannot reuse evidence"));
    let links = AgentEvidenceRepository::list_current_run_web_citation_links(&db, "evidence-run")
        .expect("Run-local citation links");
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].label, "[W1]");
    assert_eq!(links[0].url, "https://example.test/rules");
}

#[test]
fn current_run_citation_links_exclude_foreign_and_retired_evidence() {
    let (db, session_id, session_key) = setup_run();
    let owned = register_web_evidence(
        &db,
        session_id,
        "evidence-run",
        "Owned source",
        "https://example.test/owned",
    );
    cancel_test_run(&db, "evidence-run");
    accept_test_run(
        &db,
        session_id,
        &session_key,
        "evidence-second-client-request",
        "evidence-second-run",
        "evidence-second-turn",
        "第二个运行",
    );
    let _foreign = register_web_evidence(
        &db,
        session_id,
        "evidence-second-run",
        "Foreign source",
        "https://example.test/foreign",
    );

    let links = AgentEvidenceRepository::list_current_run_web_citation_links(&db, "evidence-run")
        .expect("Run-local citation links");
    assert_eq!(links.len(), 1, "only the current Run evidence may be bound");
    assert_eq!(links[0].url, "https://example.test/owned");

    db.with_conn(|conn| {
        conn.execute(
            "UPDATE session_evidence SET retired_at = ?1 WHERE id = ?2",
            rusqlite::params!["2026-07-14T00:00:00Z", owned.evidence_id],
        )?;
        Ok(())
    })
    .expect("retire current Run evidence");

    let after_retire =
        AgentEvidenceRepository::list_current_run_web_citation_links(&db, "evidence-run")
            .expect("Run-local citation links after retirement");
    assert!(
        after_retire.is_empty(),
        "retired evidence must never become an Exact/Normalized citation"
    );
}

#[test]
fn web_provenance_ordinals_restart_at_w1_for_each_run_with_high_ledger_ids() {
    let (db, session_id, session_key) = setup_run();
    db.with_conn(|conn| {
        conn.execute(
            "INSERT INTO sqlite_sequence(name, seq) VALUES ('session_evidence', 1000)",
            [],
        )?;
        Ok(())
    })
    .expect("advance evidence ledger sequence");
    let first = register_web_evidence(
        &db,
        session_id,
        "evidence-run",
        "First Run source",
        "https://example.test/first",
    );
    cancel_test_run(&db, "evidence-run");
    accept_test_run(
        &db,
        session_id,
        &session_key,
        "evidence-second-client-request",
        "evidence-second-run",
        "evidence-second-turn",
        "第二个运行",
    );
    let second = register_web_evidence(
        &db,
        session_id,
        "evidence-second-run",
        "Second Run source",
        "https://example.test/second",
    );

    assert_eq!((first.evidence_id, second.evidence_id), (1001, 1002));
    for run_id in ["evidence-run", "evidence-second-run"] {
        let policy = AgentEvidenceRepository::provenance_policy(&db, run_id, true)
            .expect("Run-local provenance policy");
        assert_eq!(
            policy.current_run_web_evidence_ids,
            std::collections::BTreeSet::from([1]),
            "each Run must expose its first Web source as W1"
        );
    }
}

#[test]
fn hr1_same_session_runs_keep_w1_bound_to_their_own_evidence() {
    let (db, session_id, session_key) = setup_run();
    let first = register_web_evidence(
        &db,
        session_id,
        "evidence-run",
        "First Run source",
        "https://example.test/first-run",
    );
    cancel_test_run(&db, "evidence-run");
    accept_test_run(
        &db,
        session_id,
        &session_key,
        "evidence-second-client-request",
        "evidence-second-run",
        "evidence-second-turn",
        "第二个运行",
    );
    let second = register_web_evidence(
        &db,
        session_id,
        "evidence-second-run",
        "Second Run source",
        "https://example.test/second-run",
    );

    let first_links =
        AgentEvidenceRepository::list_current_run_web_citation_links(&db, "evidence-run")
            .expect("first Run citation links");
    let second_links =
        AgentEvidenceRepository::list_current_run_web_citation_links(&db, "evidence-second-run")
            .expect("second Run citation links");
    assert_eq!(
        (first_links[0].label.as_str(), first_links[0].url.as_str()),
        ("[W1]", "https://example.test/first-run")
    );
    assert_eq!(
        (second_links[0].label.as_str(), second_links[0].url.as_str()),
        ("[W1]", "https://example.test/second-run")
    );
    assert_ne!(first.evidence_id, second.evidence_id);
    assert_eq!(
        AgentEvidenceRepository::current_run_web_urls(&db, "evidence-second-run")
            .expect("second Run source URLs"),
        std::collections::BTreeSet::from(["https://example.test/second-run".to_string()]),
        "the same W1 label must resolve only through the active Run's registrations"
    );
}

#[test]
fn current_run_fetch_urls_exclude_foreign_and_retired_web_evidence() {
    let (db, session_id, session_key) = setup_run();
    let owned = register_web_evidence(
        &db,
        session_id,
        "evidence-run",
        "Owned source",
        "https://example.test/owned",
    );
    cancel_test_run(&db, "evidence-run");
    accept_test_run(
        &db,
        session_id,
        &session_key,
        "evidence-second-client-request",
        "evidence-second-run",
        "evidence-second-turn",
        "第二个运行",
    );
    register_web_evidence(
        &db,
        session_id,
        "evidence-second-run",
        "Foreign source",
        "https://example.test/foreign",
    );

    assert_eq!(
        AgentEvidenceRepository::current_run_web_urls(&db, "evidence-run")
            .expect("owned fetch URLs"),
        std::collections::BTreeSet::from(["https://example.test/owned".to_string()])
    );

    db.with_conn(|conn| {
        conn.execute(
            "UPDATE session_evidence SET retired_at = ?1 WHERE id = ?2",
            rusqlite::params!["2026-07-14T00:00:00Z", owned.evidence_id],
        )?;
        Ok(())
    })
    .expect("retire current Run evidence");
    assert!(
        AgentEvidenceRepository::current_run_web_urls(&db, "evidence-run")
            .expect("retired URLs excluded")
            .is_empty()
    );
}

#[test]
fn external_tool_evidence_is_run_owned_and_persists_only_bounded_output() {
    let (db, session_id, session_key) = setup_run();
    let evidence = AgentEvidenceRepository::register_external_tool(
        &db,
        ExternalToolEvidenceInput {
            session_id,
            run_id: "evidence-run".into(),
            message_seq_first: 1,
            title: "external_read_record_deadbeef".into(),
            provider_id: "readonly".into(),
            provider_config_hash: "provider-hash".into(),
            binding_id: "binding-id".into(),
            raw_result_hash: "result-hash".into(),
            retrieved_at: "2026-07-30T00:00:00Z".into(),
            bounded_excerpt: "bounded external result".into(),
            url: None,
            normalized_url: None,
            domain: None,
        },
    )
    .expect("external evidence");

    db.with_read_conn(|conn| {
        let stored: (String, String, String, String, String, String) = conn.query_row(
            "SELECT provider_id, provider_kind, raw_result_hash, retrieved_at,
                    bounded_excerpt, extraction_method
             FROM session_evidence WHERE id = ?1",
            [evidence.evidence_id],
            |row| {
                Ok((
                    row.get(0)?,
                    row.get(1)?,
                    row.get(2)?,
                    row.get(3)?,
                    row.get(4)?,
                    row.get(5)?,
                ))
            },
        )?;
        assert_eq!(
            stored,
            (
                "readonly".into(),
                "mcp".into(),
                "result-hash".into(),
                "2026-07-30T00:00:00Z".into(),
                "bounded external result".into(),
                "mcp_tool_output_v1".into(),
            )
        );
        let source: String = conn.query_row(
            "SELECT registration_source FROM agent_run_evidence
             WHERE run_id = 'evidence-run' AND evidence_id = ?1",
            [evidence.evidence_id],
            |row| row.get(0),
        )?;
        assert_eq!(source, "external_tool");
        Ok(())
    })
    .expect("external evidence metadata");

    cancel_test_run(&db, "evidence-run");
    accept_test_run(
        &db,
        session_id,
        &session_key,
        "evidence-second-client",
        "evidence-second-run",
        "evidence-second-turn",
        "再次获取同一外部结果",
    );
    let repeated = AgentEvidenceRepository::register_external_tool(
        &db,
        ExternalToolEvidenceInput {
            session_id,
            run_id: "evidence-second-run".into(),
            message_seq_first: 2,
            title: "external_read_record_deadbeef".into(),
            provider_id: "readonly".into(),
            provider_config_hash: "provider-hash".into(),
            binding_id: "binding-id".into(),
            raw_result_hash: "result-hash".into(),
            retrieved_at: "2026-07-30T00:01:00Z".into(),
            bounded_excerpt: "bounded external result".into(),
            url: None,
            normalized_url: None,
            domain: None,
        },
    )
    .expect("second Run gets its own acquisition");
    assert_ne!(repeated.evidence_id, evidence.evidence_id);
    db.with_read_conn(|conn| {
        let second: (String, String) = conn.query_row(
            "SELECT origin_run_id, retrieved_at
             FROM session_evidence WHERE id = ?1",
            [repeated.evidence_id],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(
            second,
            ("evidence-second-run".into(), "2026-07-30T00:01:00Z".into())
        );
        Ok(())
    })
    .expect("second acquisition ownership");

    let error = AgentEvidenceRepository::register_external_tool(
        &db,
        ExternalToolEvidenceInput {
            session_id,
            run_id: "another-run".into(),
            message_seq_first: 1,
            title: "external_read_record_deadbeef".into(),
            provider_id: "readonly".into(),
            provider_config_hash: "provider-hash".into(),
            binding_id: "binding-id".into(),
            raw_result_hash: "result-hash-2".into(),
            retrieved_at: "2026-07-30T00:00:00Z".into(),
            bounded_excerpt: "must not persist".into(),
            url: None,
            normalized_url: None,
            domain: None,
        },
    )
    .expect_err("different Run");
    assert_eq!(error.to_string(), "agent_evidence_run_not_found");
}

#[test]
fn evidence_rejects_a_run_from_another_session_without_writing_a_ledger_row() {
    let (db, _, _) = setup_run();
    let other_session = NormalSessionRepository::create(&db)
        .expect("other normal session")
        .session_id;

    let error = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id: other_session,
            run_id: "evidence-run".to_string(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "不应归属到其他会话".to_string(),
            source_path: "notes/a.md".to_string(),
            source_span_start: 0,
            source_span_end: 1,
            heading_path: None,
            content_hash: "hash".to_string(),
            retrieval_reason: None,
            score: None,
        },
    )
    .expect_err("session mismatch must fail");
    assert_eq!(error.to_string(), "agent_evidence_run_not_found");

    db.with_read_conn(|conn| {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM session_evidence", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 0);
        Ok(())
    })
    .expect("no row written");
}

#[test]
fn evidence_requires_its_first_reference_message_to_exist_in_the_same_session() {
    let (db, session_id, _) = setup_run();

    let error = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id,
            run_id: "evidence-run".to_string(),
            message_seq_first: 99,
            material_role: MaterialRole::Reference,
            title: "不存在的消息序号".to_string(),
            source_path: "notes/a.md".to_string(),
            source_span_start: 0,
            source_span_end: 1,
            heading_path: None,
            content_hash: "hash".to_string(),
            retrieval_reason: None,
            score: None,
        },
    )
    .expect_err("missing message sequence must fail");
    assert_eq!(error.to_string(), "agent_evidence_message_not_found");

    db.with_read_conn(|conn| {
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM session_evidence", [], |row| {
            row.get(0)
        })?;
        assert_eq!(count, 0);
        Ok(())
    })
    .expect("no orphan evidence");
}

#[test]
fn web_evidence_rejects_an_excerpt_larger_than_the_safe_bound() {
    let (db, session_id, _) = setup_run();
    let error = AgentEvidenceRepository::register_web(
        &db,
        WebEvidenceInput {
            session_id,
            run_id: "evidence-run".to_string(),
            message_seq_first: 1,
            material_role: MaterialRole::Reference,
            title: "过大摘录".to_string(),
            url: "https://example.test/large".to_string(),
            normalized_url: "https://example.test/large".to_string(),
            domain: "example.test".to_string(),
            retrieved_at: "2026-07-13T00:00:00Z".to_string(),
            provider_id: "official-web".to_string(),
            provider_kind: "https".to_string(),
            raw_result_hash: "web-result-hash".to_string(),
            extraction_method: "article_quote".to_string(),
            bounded_excerpt: "x".repeat(2_001),
            retrieval_reason: None,
            score: None,
            source_rank: None,
            conflict_group: None,
            failure_reason: None,
        },
    )
    .expect_err("oversized excerpt must fail");
    assert_eq!(error.to_string(), "agent_evidence_excerpt_too_large");
}
