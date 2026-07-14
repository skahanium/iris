use super::domain_executor::DomainMaterialRole;
use super::run_context::{RunContext, RunContextAssembler, RunContextMaterial};
use crate::ai_runtime::agent_run_repository::{AcceptRunInput, AgentRunRepository};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed, Modality, RiskClass,
    SecurityDomain,
};
use crate::ai_types::{ContextReferenceKind, ContextReferenceWire};
use crate::storage::db::Database;

fn envelope() -> ExecutionEnvelope {
    ExecutionEnvelope {
        effect: Effect::Answer,
        context: ContextMode::ExplicitReferences,
        freshness: Freshness::Offline,
        effort: Effort::Direct,
        security_domain: SecurityDomain::Normal,
        risk: RiskClass::ReadOnly,
        modalities: vec![Modality::Text],
        material_needs: vec![MaterialNeed::Reference],
        required_capabilities: vec![],
        explicit_constraints: vec![],
    }
}

#[test]
fn assemble_reads_only_the_run_persisted_explicit_reference() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/attached.md"), "attached evidence").expect("attached note");
    std::fs::write(vault.join("notes/unattached.md"), "must never be read")
        .expect("unattached note");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "explicit-reference-context".into(),
            run_id: "run-explicit-reference-context".into(),
            turn_id: "turn-explicit-reference-context".into(),
            message: "请根据附件回答".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "attached".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/attached.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: "untrusted client excerpt".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-explicit-reference-context",
    )
    .expect("assembled context");

    assert_eq!(context.user_message, "请根据附件回答");
    assert_eq!(context.materials.len(), 1);
    assert_eq!(context.materials[0].content, "attached evidence");
    assert_eq!(context.materials[0].source_path, "notes/attached.md");
    assert!(!context
        .prompt_with_domain_plan(&context.domain_plan())
        .contains("must never be read"));
    assert!(!context
        .prompt_with_domain_plan(&context.domain_plan())
        .contains("untrusted client excerpt"));
}

#[test]
fn assemble_rejects_reserved_or_changed_explicit_references() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join(".classified")).expect("classified directory");
    std::fs::write(vault.join(".classified/secret.md"), "secret").expect("classified note");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "reserved-reference-context".into(),
            run_id: "run-reserved-reference-context".into(),
            turn_id: "turn-reserved-reference-context".into(),
            message: "请回答".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "secret".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some(".classified/secret.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-reserved-reference-context",
    )
    .expect_err("normal runs must reject classified inputs");
    assert_eq!(error.to_string(), "agent_run_invalid_explicit_reference");
}

#[test]
fn assemble_allows_direct_chat_without_a_vault_when_no_reference_exists() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "direct-chat-without-vault".into(),
            run_id: "run-direct-chat-without-vault".into(),
            turn_id: "turn-direct-chat-without-vault".into(),
            message: "不引用文件的普通问答".into(),
            content_parts: None,
            explicit_references: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::None,
                material_needs: vec![],
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        None,
        &session.session_key,
        "run-direct-chat-without-vault",
    )
    .expect("direct chat context");

    assert!(context.materials.is_empty());
    assert_eq!(context.user_message, "不引用文件的普通问答");
}

#[test]
fn assemble_rejects_reference_when_persisted_hash_no_longer_matches() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/changed.md"), "accepted version").expect("accepted version");
    let accepted_hash = crate::cas::hash::content_hash_str("accepted version");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "changed-reference-context".into(),
            run_id: "run-changed-reference-context".into(),
            turn_id: "turn-changed-reference-context".into(),
            message: "引用文件发生变化后不得继续".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "changed".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/changed.md".into()),
                content_hash: Some(accepted_hash),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");
    std::fs::write(vault.join("notes/changed.md"), "changed after acceptance").expect("changed");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-changed-reference-context",
    )
    .expect_err("changed reference must block Provider prompt assembly");
    assert_eq!(error.to_string(), "agent_run_explicit_reference_changed");
}

#[test]
fn explicit_materials_register_as_run_owned_evidence_without_storing_bodies() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/evidence.md"), "evidence body").expect("evidence note");

    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "evidence-ledger-context".into(),
            run_id: "run-evidence-ledger-context".into(),
            turn_id: "turn-evidence-ledger-context".into(),
            message: "请引用附件".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "evidence".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/evidence.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-evidence-ledger-context",
    )
    .expect("assembled context");
    let evidence_ids =
        RunContextAssembler::register_evidence(&db, "run-evidence-ledger-context", &context)
            .expect("registered evidence");

    assert_eq!(evidence_ids.len(), 1);
    db.with_read_conn(|conn| {
        let (source_path, body_column_count): (String, i64) = conn.query_row(
            "SELECT source_path, COUNT(*) OVER () FROM session_evidence WHERE id = ?1",
            [evidence_ids[0]],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(source_path, "notes/evidence.md");
        assert_eq!(body_column_count, 1);
        Ok(())
    })
    .expect("ledger metadata");
}

#[test]
fn prompt_applies_the_domain_executor_rules_without_expanding_explicit_context() {
    let context = RunContext {
        session_id: 1,
        message_seq_first: 1,
        user_message: "请结合制度写一份请示".into(),
        content_parts: None,
        envelope: ExecutionEnvelope {
            material_needs: vec![MaterialNeed::Authority, MaterialNeed::Exemplar],
            ..envelope()
        },
        materials: vec![RunContextMaterial {
            role: DomainMaterialRole::Reference,
            source_path: "notes/attached.md".into(),
            content_hash: "hash".into(),
            source_span_start: 0,
            source_span_end: 8,
            content: "用户明确附上的事实".into(),
        }],
    };

    let prompt = context.prompt_with_domain_plan(&context.domain_plan());
    assert!(prompt.contains("内容依据"));
    assert!(prompt.contains("写法参考"));
    assert!(prompt.contains("role=\"reference\""));
    assert!(prompt.contains("用户明确附上的事实"));
    assert!(!prompt.contains("当前活动文档"));
}
