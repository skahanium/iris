use super::agent_evidence_repository::{
    AgentEvidenceRepository, LocalEvidenceInput, MaterialRole, WebEvidenceInput,
};
use super::agent_run_repository::{AcceptRunInput, AgentRunRepository};
use super::normal_session_repository::NormalSessionRepository;
use super::run_contract::{
    ContextMode, Effect, Effort, ExecutionEnvelope, ExplicitConstraint, Freshness, MaterialNeed,
    Modality, RiskClass, SecurityDomain,
};
use crate::storage::db::Database;

fn setup_run() -> (Database, i64, String) {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("normal session");
    let session_id = session.session_id;
    let session_key = session.session_key;
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id,
            session_key: session_key.clone(),
            client_request_id: "evidence-client-request".to_string(),
            run_id: "evidence-run".to_string(),
            turn_id: "evidence-turn".to_string(),
            message: "为证据账本建立可追溯运行".to_string(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effect: Effect::Answer,
                context: ContextMode::ExplicitReferences,
                freshness: Freshness::WebRequired,
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
            },
        },
    )
    .expect("accepted run");
    (db, session_id, session_key)
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
