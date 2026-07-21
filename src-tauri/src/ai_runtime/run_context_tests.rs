use super::domain_executor::DomainMaterialRole;
use super::run_context::{
    classify_context_assembly_failure, RunContext, RunContextAssembler, RunContextMaterial,
};
use crate::ai_runtime::agent_evidence_repository::{
    AgentEvidenceRepository, LocalEvidenceInput, MaterialRole,
};
use crate::ai_runtime::agent_run_repository::{
    AcceptRunInput, AgentRunRepository, AppendRunEventInput, FinalizeRunInput,
};
use crate::ai_runtime::normal_session_repository::NormalSessionRepository;
use crate::ai_runtime::run_contract::{
    ContextMode, Effect, Effort, ExecutionEnvelope, Freshness, MaterialNeed, Modality, RiskClass,
    SafeRunErrorCode, SecurityDomain, WebDecisionReason,
};
use crate::ai_runtime::tool_dispatch::{dispatch_tool, ToolDispatchContext};
use crate::ai_runtime::tool_execution_pipeline::{
    audit_dispatched_tool, evaluate_tool_execution, ToolExecutionGate,
};
use crate::ai_runtime::tool_policy::ToolPolicyContext;
use crate::ai_types::{ContextReferenceKind, ContextReferenceWire};
use crate::app::AppState;
use crate::storage::db::Database;

fn envelope() -> ExecutionEnvelope {
    ExecutionEnvelope {
        effect: Effect::Answer,
        context: ContextMode::ExplicitReferences,
        freshness: Freshness::Offline,
        web_reason: WebDecisionReason::LegacyUnknown,
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
fn context_assembly_failures_map_to_precise_safe_codes() {
    for (internal, expected) in [
        (
            "agent_run_invalid_explicit_reference",
            SafeRunErrorCode::InvalidExplicitReference,
        ),
        (
            "agent_run_explicit_reference_changed",
            SafeRunErrorCode::ExplicitReferenceChanged,
        ),
        (
            "agent_run_invalid_retrieval_scope",
            SafeRunErrorCode::InvalidRetrievalScope,
        ),
        (
            "agent_run_local_reference_index_unavailable",
            SafeRunErrorCode::LocalReferenceIndexUnavailable,
        ),
    ] {
        assert_eq!(
            classify_context_assembly_failure(&crate::error::AppError::msg(internal)),
            expected
        );
    }
    assert_eq!(
        classify_context_assembly_failure(&crate::error::AppError::msg("database unavailable")),
        SafeRunErrorCode::PersistenceFailed
    );
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
                content_hash: Some(crate::cas::hash::content_hash_str("attached evidence")),
                utf8_range: None,
                editor_range: None,
                excerpt: "untrusted client excerpt".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
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
                content_hash: Some(crate::cas::hash::content_hash_str("secret")),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
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
            context_scope: Default::default(),
            display_mentions: vec![],
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
            context_scope: Default::default(),
            display_mentions: vec![],
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
fn assemble_rejects_a_note_reference_without_a_backend_content_hash() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/unhashed.md"), "disk authority").expect("note");
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "unhashed-note-reference".into(),
            run_id: "run-unhashed-note-reference".into(),
            turn_id: "turn-unhashed-note-reference".into(),
            message: "请总结附件".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "unhashed".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/unhashed.md".into()),
                content_hash: None,
                utf8_range: None,
                editor_range: None,
                excerpt: "client supplied body must not be trusted".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-unhashed-note-reference",
    )
    .expect_err("unhashed note references must fail closed");

    assert_eq!(error.to_string(), "agent_run_invalid_explicit_reference");
}

#[test]
fn assemble_requires_selection_and_paragraph_references_to_have_valid_ranges() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let body = "alpha beta gamma";
    std::fs::write(vault.join("notes/ranged.md"), body).expect("note");
    let hash = crate::cas::hash::content_hash_str(body);

    for (index, kind) in [
        ContextReferenceKind::Selection,
        ContextReferenceKind::Paragraph,
    ]
    .into_iter()
    .enumerate()
    {
        let db = Database::open_in_memory().expect("database");
        let session = NormalSessionRepository::create(&db).expect("session");
        let run_id = format!("run-invalid-range-{index}");
        AgentRunRepository::accept(
            &db,
            AcceptRunInput {
                session_id: session.session_id,
                session_key: session.session_key.clone(),
                client_request_id: format!("invalid-range-{index}"),
                run_id: run_id.clone(),
                turn_id: format!("turn-invalid-range-{index}"),
                message: "请分析选区".into(),
                content_parts: None,
                explicit_references: vec![ContextReferenceWire {
                    id: format!("range-{index}"),
                    kind,
                    file_path: Some("notes/ranged.md".into()),
                    content_hash: Some(hash.clone()),
                    utf8_range: None,
                    editor_range: None,
                    excerpt: "alpha".into(),
                    heading_path: None,
                    anchor: None,
                    stale: false,
                    invalid_reason: None,
                }],
                context_scope: Default::default(),
                display_mentions: vec![],
                explicit_action: None,
                envelope: envelope(),
            },
        )
        .expect("accepted run");

        let error = RunContextAssembler::assemble(&db, Some(&vault), &session.session_key, &run_id)
            .expect_err("selection and paragraph references require a disk range");
        assert_eq!(error.to_string(), "agent_run_invalid_explicit_reference");
    }
}

#[test]
fn assemble_rereads_an_exact_chinese_utf8_selection_from_disk() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let body = "甲乙😀丙丁";
    std::fs::write(vault.join("notes/chinese.md"), body).expect("note");
    let start = "甲".len();
    let end = "甲乙😀丙".len();
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "valid-chinese-selection".into(),
            run_id: "run-valid-chinese-selection".into(),
            turn_id: "turn-valid-chinese-selection".into(),
            message: "请分析选区".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "selection".into(),
                kind: ContextReferenceKind::Selection,
                file_path: Some("notes/chinese.md".into()),
                content_hash: Some(crate::cas::hash::content_hash_str(body)),
                utf8_range: Some(crate::ai_types::SourceSpan { start, end }),
                editor_range: None,
                excerpt: "客户端伪造内容".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-valid-chinese-selection",
    )
    .expect("assembled exact selection");

    assert_eq!(context.materials.len(), 1);
    assert_eq!(context.materials[0].content, "乙😀丙");
    assert_eq!(context.materials[0].source_span_start, start as i64);
    assert_eq!(context.materials[0].source_span_end, end as i64);
}

#[test]
fn assemble_rejects_a_non_utf8_explicit_reference() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::write(vault.join("notes/invalid.md"), [0xff, 0xfe]).expect("invalid note");
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "invalid-utf8-reference".into(),
            run_id: "run-invalid-utf8-reference".into(),
            turn_id: "turn-invalid-utf8-reference".into(),
            message: "请分析附件".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "invalid".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/invalid.md".into()),
                content_hash: Some("unusable".into()),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("accepted run");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-invalid-utf8-reference",
    )
    .expect_err("non-UTF-8 notes must fail closed");

    assert_eq!(error.to_string(), "agent_run_invalid_explicit_reference");
}

fn index_scoped_note(db: &Database, path: &str, title: &str, body: &str, excerpt: &str) {
    db.with_conn(|conn| {
        let hash = crate::cas::hash::content_hash_str(body);
        let source_start = body.find(excerpt).unwrap_or(0);
        let source_end = source_start + excerpt.len();
        conn.execute(
            "INSERT INTO files
             (path, title, content_hash, word_count, created_at, updated_at)
             VALUES (?1, ?2, ?3, 1, datetime('now'), datetime('now'))",
            rusqlite::params![path, title, hash],
        )?;
        let file_id = conn.last_insert_rowid();
        conn.execute(
            "INSERT INTO chunks
             (file_id, chunk_index, content, char_count, source_start, source_end, content_hash)
             VALUES (?1, 0, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                file_id,
                excerpt,
                excerpt.chars().count() as i64,
                source_start as i64,
                source_end as i64,
                crate::cas::hash::content_hash_str(excerpt),
            ],
        )?;
        conn.execute(
            "INSERT INTO files_fts (path, title, content) VALUES (?1, ?2, ?3)",
            rusqlite::params![path, title, body],
        )?;
        Ok(())
    })
    .expect("index scoped note");
}

fn note_reference(id: &str, path: &str, body: &str) -> ContextReferenceWire {
    ContextReferenceWire {
        id: id.into(),
        kind: ContextReferenceKind::Note,
        file_path: Some(path.into()),
        content_hash: Some(crate::cas::hash::content_hash_str(body)),
        utf8_range: None,
        editor_range: None,
        excerpt: "client body must be ignored".into(),
        heading_path: None,
        anchor: None,
        stale: false,
        invalid_reason: None,
    }
}

#[test]
fn scope_only_context_performs_deterministic_retrieval_before_provider_dispatch() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(
        &db,
        "notes/in.md",
        "In",
        "scope-needle authorized evidence",
        "scope-needle authorized evidence",
    );
    index_scoped_note(
        &db,
        "outside.md",
        "Outside",
        "scope-needle forbidden evidence",
        "scope-needle forbidden evidence",
    );
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "scope-only-retrieval".into(),
            run_id: "run-scope-only-retrieval".into(),
            turn_id: "turn-scope-only-retrieval".into(),
            message: "scope-needle".into(),
            content_parts: None,
            explicit_references: vec![],
            context_scope: crate::ai_runtime::retrieval_scope::ContextScopeDto {
                path_prefixes: vec!["notes/".into()],
                ..Default::default()
            },
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::ExplicitScope,
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-scope-only-retrieval",
    )
    .expect("assembled scoped context");

    assert_eq!(context.materials.len(), 1);
    assert_eq!(context.materials[0].source_path, "notes/in.md");
    assert!(context.materials[0].content.contains("authorized evidence"));
    assert!(!context
        .prompt_with_domain_plan(&context.domain_plan())
        .contains("forbidden evidence"));
}

#[test]
fn mixed_full_material_and_scope_still_retrieves_uncovered_scoped_paths() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    std::fs::create_dir_all(vault.join("scoped")).expect("scoped directory");
    let full_body = "full explicit evidence";
    std::fs::write(vault.join("notes/full.md"), full_body).expect("full note");
    let scoped_body = "mixed-scope-needle additional evidence";
    std::fs::write(vault.join("scoped/other.md"), scoped_body).expect("scoped note");
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(&db, "scoped/other.md", "Other", scoped_body, scoped_body);
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "mixed-material-scope".into(),
            run_id: "run-mixed-material-scope".into(),
            turn_id: "turn-mixed-material-scope".into(),
            message: "mixed-scope-needle".into(),
            content_parts: None,
            explicit_references: vec![note_reference("full", "notes/full.md", full_body)],
            context_scope: crate::ai_runtime::retrieval_scope::ContextScopeDto {
                path_prefixes: vec!["scoped/".into()],
                ..Default::default()
            },
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::ExplicitScope,
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-mixed-material-scope",
    )
    .expect("mixed context");

    let paths = context
        .materials
        .iter()
        .map(|material| material.source_path.as_str())
        .collect::<Vec<_>>();
    assert!(paths.contains(&"notes/full.md"));
    assert!(paths.contains(&"scoped/other.md"));
}

#[test]
fn oversized_note_falls_back_to_exact_scope_retrieval_without_truncating_fulltext() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let body = format!("long-note-needle {}", "x".repeat(13_000));
    let indexed_excerpt = &body[..128];
    std::fs::write(vault.join("notes/long.md"), &body).expect("long note");
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(&db, "notes/long.md", "Long", &body, indexed_excerpt);
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "long-note-fallback".into(),
            run_id: "run-long-note-fallback".into(),
            turn_id: "turn-long-note-fallback".into(),
            message: "long-note-needle".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "long-note".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/long.md".into()),
                content_hash: Some(crate::cas::hash::content_hash_str(&body)),
                utf8_range: None,
                editor_range: None,
                excerpt: "client truncation must be ignored".into(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-long-note-fallback",
    )
    .expect("long note must use indexed exact-scope retrieval");

    assert_eq!(context.materials.len(), 1);
    assert_eq!(context.materials[0].source_path, "notes/long.md");
    assert_eq!(context.materials[0].content, indexed_excerpt);
    assert!(context.retrieval_scope.is_unrestricted());
    assert_ne!(context.materials[0].content, body);
}

#[test]
fn oversized_note_fallback_is_query_independent_and_does_not_expand_tool_scope() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let body = format!("indexed first chunk {}", "x".repeat(13_000));
    let excerpt = &body[..128];
    std::fs::write(vault.join("notes/query-independent.md"), &body).expect("long note");
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(
        &db,
        "notes/query-independent.md",
        "Query Independent",
        &body,
        excerpt,
    );
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "query-independent-fallback".into(),
            run_id: "run-query-independent-fallback".into(),
            turn_id: "turn-query-independent-fallback".into(),
            message: "this query does not occur in the note".into(),
            content_parts: None,
            explicit_references: vec![note_reference("long", "notes/query-independent.md", &body)],
            context_scope: crate::ai_runtime::retrieval_scope::ContextScopeDto {
                path_prefixes: vec!["allowed/".into()],
                ..Default::default()
            },
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-query-independent-fallback",
    )
    .expect("query-independent exact-path fallback");

    assert_eq!(context.materials[0].content, excerpt);
    assert!(!context
        .retrieval_scope
        .matches_path("notes/query-independent.md"));
}

#[test]
fn oversized_note_fallback_prefers_a_later_current_chunk_that_matches_the_query() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let relevant = "later-query-needle evidence";
    let body = format!("{}\n{relevant}\n{}", "a".repeat(256), "x".repeat(13_000));
    let first_excerpt = &body[..128];
    let relevant_start = body.find(relevant).expect("relevant span");
    let relevant_end = relevant_start + relevant.len();
    std::fs::write(vault.join("notes/later-chunk.md"), &body).expect("long note");
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(
        &db,
        "notes/later-chunk.md",
        "Later Chunk",
        &body,
        first_excerpt,
    );
    db.with_conn(|conn| {
        let file_id: i64 = conn.query_row(
            "SELECT id FROM files WHERE path = 'notes/later-chunk.md'",
            [],
            |row| row.get(0),
        )?;
        conn.execute(
            "INSERT INTO chunks
             (file_id, chunk_index, content, char_count, source_start, source_end, content_hash)
             VALUES (?1, 1, ?2, ?3, ?4, ?5, ?6)",
            rusqlite::params![
                file_id,
                relevant,
                relevant.chars().count() as i64,
                relevant_start as i64,
                relevant_end as i64,
                crate::cas::hash::content_hash_str(relevant),
            ],
        )?;
        Ok(())
    })
    .expect("index relevant chunk");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "later-relevant-chunk".into(),
            run_id: "run-later-relevant-chunk".into(),
            turn_id: "turn-later-relevant-chunk".into(),
            message: "later-query-needle".into(),
            content_parts: None,
            explicit_references: vec![note_reference("later", "notes/later-chunk.md", &body)],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-later-relevant-chunk",
    )
    .expect("relevant exact-path chunk");

    assert_eq!(context.materials[0].content, relevant);
    assert!(context.retrieval_scope.is_unrestricted());
}

#[test]
fn oversized_note_fallback_rejects_a_chunk_that_does_not_match_current_disk() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let body = format!("real disk content {}", "x".repeat(13_000));
    std::fs::write(vault.join("notes/dirty-index.md"), &body).expect("long note");
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(
        &db,
        "notes/dirty-index.md",
        "Dirty",
        &body,
        "fabricated indexed chunk",
    );
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "dirty-index-fallback".into(),
            run_id: "run-dirty-index-fallback".into(),
            turn_id: "turn-dirty-index-fallback".into(),
            message: "real disk content".into(),
            content_parts: None,
            explicit_references: vec![note_reference("dirty", "notes/dirty-index.md", &body)],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-dirty-index-fallback",
    )
    .expect_err("dirty indexed chunks must fail closed");

    assert_eq!(
        error.to_string(),
        "agent_run_local_reference_index_unavailable"
    );
}

#[test]
fn exact_path_fallback_supports_more_than_eight_long_references() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let db = Database::open_in_memory().expect("database");
    let mut references = Vec::new();
    for index in 0..9 {
        let path = format!("notes/long-{index}.md");
        let body = format!("shared-fallback-term-{index} {}", "x".repeat(13_000));
        let excerpt = &body[..96];
        std::fs::write(vault.join(&path), &body).expect("long note");
        index_scoped_note(&db, &path, &format!("Long {index}"), &body, excerpt);
        references.push(note_reference(&format!("long-{index}"), &path, &body));
    }
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "nine-long-fallbacks".into(),
            run_id: "run-nine-long-fallbacks".into(),
            turn_id: "turn-nine-long-fallbacks".into(),
            message: "shared-fallback-term".into(),
            content_parts: None,
            explicit_references: references,
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-nine-long-fallbacks",
    )
    .expect("all exact fallback paths must be resolved independently");

    assert_eq!(context.materials.len(), 9);
    assert_eq!(context.local_retrieval_packets.len(), 9);
}

#[test]
fn total_material_budget_falls_back_only_the_overflowing_note_to_exact_scope_retrieval() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let first = format!("first {}", "a".repeat(10_900));
    let second = format!("second {}", "b".repeat(10_900));
    let third = format!("third-budget-needle {}", "c".repeat(10_900));
    let third_indexed_excerpt = &third[..128];
    for (name, body) in [
        ("first.md", &first),
        ("second.md", &second),
        ("third.md", &third),
    ] {
        std::fs::write(vault.join("notes").join(name), body).expect("note");
    }
    let db = Database::open_in_memory().expect("database");
    index_scoped_note(
        &db,
        "notes/third.md",
        "Third",
        &third,
        third_indexed_excerpt,
    );
    let session = NormalSessionRepository::create(&db).expect("session");
    let references = [("first", &first), ("second", &second), ("third", &third)]
        .into_iter()
        .map(|(name, body)| ContextReferenceWire {
            id: name.into(),
            kind: ContextReferenceKind::Note,
            file_path: Some(format!("notes/{name}.md")),
            content_hash: Some(crate::cas::hash::content_hash_str(body)),
            utf8_range: None,
            editor_range: None,
            excerpt: "client truncation must be ignored".into(),
            heading_path: None,
            anchor: None,
            stale: false,
            invalid_reason: None,
        })
        .collect();
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "total-material-budget".into(),
            run_id: "run-total-material-budget".into(),
            turn_id: "turn-total-material-budget".into(),
            message: "third-budget-needle".into(),
            content_parts: None,
            explicit_references: references,
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-total-material-budget",
    )
    .expect("overflowing note must use exact-scope retrieval");

    assert_eq!(context.materials.len(), 3);
    assert_eq!(context.materials[0].content, first);
    assert_eq!(context.materials[1].content, second);
    assert_eq!(context.materials[2].content, third_indexed_excerpt);
}

#[test]
fn oversized_note_fails_closed_when_exact_scope_index_material_is_unavailable() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let body = format!("unindexed-long-note {}", "x".repeat(13_000));
    std::fs::write(vault.join("notes/unindexed.md"), &body).expect("long note");
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "unindexed-long-note".into(),
            run_id: "run-unindexed-long-note".into(),
            turn_id: "turn-unindexed-long-note".into(),
            message: "unindexed-long-note".into(),
            content_parts: None,
            explicit_references: vec![ContextReferenceWire {
                id: "unindexed".into(),
                kind: ContextReferenceKind::Note,
                file_path: Some("notes/unindexed.md".into()),
                content_hash: Some(crate::cas::hash::content_hash_str(&body)),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");

    let error = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-unindexed-long-note",
    )
    .expect_err("long note without indexed material must fail closed");

    assert_eq!(
        error.to_string(),
        "agent_run_local_reference_index_unavailable"
    );
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
                content_hash: Some(crate::cas::hash::content_hash_str("evidence body")),
                utf8_range: None,
                editor_range: None,
                excerpt: String::new(),
                heading_path: None,
                anchor: None,
                stale: false,
                invalid_reason: None,
            }],
            context_scope: Default::default(),
            display_mentions: vec![],
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

#[tokio::test]
async fn completed_run_never_persists_transient_fallback_reference_bodies() {
    let dir = tempfile::tempdir().expect("vault");
    let vault = dir.path().join("vault");
    std::fs::create_dir_all(vault.join("notes")).expect("notes directory");
    let data_dir = dir.path().join("data");
    std::fs::create_dir_all(&data_dir).expect("data directory");
    let marker = "TRANSIENT-FALLBACK-BODY-MUST-NOT-PERSIST";
    let body = format!("{marker} {}", "x".repeat(13_000));
    let indexed_excerpt = &body[..128];
    std::fs::write(vault.join("notes/transient.md"), &body).expect("long note");
    let state = AppState::new(data_dir).expect("app state");
    state.set_vault(vault.clone()).expect("set vault");
    let db = std::sync::Arc::clone(&state.db);
    index_scoped_note(
        &db,
        "notes/transient.md",
        "Transient",
        &body,
        indexed_excerpt,
    );
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "transient-body-run".into(),
            run_id: "run-transient-body".into(),
            turn_id: "turn-transient-body".into(),
            message: "请概述附件".into(),
            content_parts: None,
            explicit_references: vec![note_reference("transient", "notes/transient.md", &body)],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                effort: Effort::ToolLoop,
                ..envelope()
            },
        },
    )
    .expect("accepted run");
    let context = RunContextAssembler::assemble(
        &db,
        Some(&vault),
        &session.session_key,
        "run-transient-body",
    )
    .expect("assembled context");
    assert!(context.materials[0].content.contains(marker));
    let evidence_ids = RunContextAssembler::register_evidence(&db, "run-transient-body", &context)
        .expect("registered evidence");
    let preparing = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-transient-body".into(),
            state_version: 0,
            event_type: crate::ai_runtime::run_contract::RunEventType::StageChanged,
            payload: crate::ai_runtime::run_contract::RunEventPayload::StageChanged {
                state: crate::ai_runtime::run_contract::RunState::Preparing,
                stage: "正在准备".into(),
            },
        },
    )
    .expect("preparing");
    let running = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-transient-body".into(),
            state_version: preparing.state_version(),
            event_type: crate::ai_runtime::run_contract::RunEventType::StageChanged,
            payload: crate::ai_runtime::run_contract::RunEventPayload::StageChanged {
                state: crate::ai_runtime::run_contract::RunState::Running,
                stage: "正在回答".into(),
            },
        },
    )
    .expect("running");
    let tool_started = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-transient-body".into(),
            state_version: running.state_version(),
            event_type: crate::ai_runtime::run_contract::RunEventType::ToolStarted,
            payload: crate::ai_runtime::run_contract::RunEventPayload::ToolStarted {
                capability: "get_context_packets".into(),
                tool_call_id: "tool-transient-body".into(),
            },
        },
    )
    .expect("tool started");
    let dispatch_context = ToolDispatchContext {
        note_path: None,
        file_id: None,
        web_search_enabled: false,
        max_web_fetches: 5,
        cold_start_packets: &context.local_retrieval_packets,
        retrieval_scope: &context.retrieval_scope,
        runtime_documents: &[],
        app_handle: None,
        attachment_count: context.materials.len(),
        skill_activation_plan: None,
    };
    let tool_arguments = serde_json::json!({});
    let tool_entry = crate::ai_runtime::tool_catalog::catalog_find("get_context_packets")
        .expect("registered context packet tool");
    let tool_policy = ToolPolicyContext {
        autonomy_level: crate::ai_runtime::AutonomyLevel::L2,
        ..ToolPolicyContext::default()
    };
    let tool_gate = ToolExecutionGate {
        run_id: "run-transient-body",
        session_id: Some(session.session_id),
        run_step: 1,
        entry: tool_entry,
        args: &tool_arguments,
        policy_ctx: &tool_policy,
        skill_id: None,
        subagent_depth: 0,
    };
    let gate_outcome = evaluate_tool_execution(&db, tool_gate).expect("tool gate");
    assert!(gate_outcome.tool_result.is_none());
    let dispatch_result = dispatch_tool(
        state.as_ref(),
        &dispatch_context,
        "get_context_packets",
        &tool_arguments,
    )
    .await;
    assert!(dispatch_result.success);
    let transient_tool_result =
        serde_json::to_string(&dispatch_result.output).expect("serialize transient tool result");
    assert!(
        transient_tool_result.contains(marker),
        "the genuine dispatch result must prove that the transient fallback body was exercised"
    );
    assert!(transient_tool_result.contains(indexed_excerpt));
    audit_dispatched_tool(&db, &tool_gate, &gate_outcome.decision, &dispatch_result)
        .expect("audit dispatched tool");
    let tool_completed = AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "run-transient-body".into(),
            state_version: tool_started.state_version(),
            event_type: crate::ai_runtime::run_contract::RunEventType::ToolCompleted,
            payload: crate::ai_runtime::run_contract::RunEventPayload::ToolCompleted {
                capability: "get_context_packets".into(),
                tool_call_id: "tool-transient-body".into(),
                summary: "工具调用完成".into(),
            },
        },
    )
    .expect("tool completed");
    AgentRunRepository::finalize(
        &db,
        FinalizeRunInput {
            run_id: "run-transient-body".into(),
            state_version: tool_completed.state_version(),
            content: "已依据附件完成概述。".into(),
            evidence_ids,
            citation_map: serde_json::json!({}),
        },
    )
    .expect("completed run");

    db.with_read_conn(|conn| {
        let persisted: String = conn.query_row(
            "SELECT r.envelope_json || '|' ||
                    COALESCE(r.explicit_action_json, '') || '|' ||
                    r.goal_summary || '|' ||
                    COALESCE((
                        SELECT group_concat(
                            COALESCE(content, '') || ':' ||
                            COALESCE(explicit_references_json, '') || ':' ||
                            COALESCE(context_scope_json, '') || ':' ||
                            COALESCE(display_mentions_json, ''),
                            '|'
                        )
                        FROM session_messages
                        WHERE session_id = r.session_id AND turn_id = r.turn_id
                    ), '') || '|' ||
                    COALESCE((SELECT group_concat(payload_json, '|') FROM agent_run_events WHERE run_id = r.run_id), '') || '|' ||
                    COALESCE((SELECT group_concat(title || ':' || source_path || ':' || content_hash || ':' || COALESCE(retrieval_reason, ''), '|') FROM session_evidence WHERE origin_run_id = r.run_id), '') || '|' ||
                    COALESCE((SELECT group_concat(tool_name || ':' || COALESCE(arguments_summary, '') || ':' || COALESCE(result_summary, '') || ':' || success, '|') FROM tool_audit WHERE run_id = r.run_id), '') || '|' ||
                    COALESCE((SELECT group_concat(tool_name || ':' || permission_name || ':' || decision || ':' || scope_summary || ':' || risk_level || ':' || result_status, '|') FROM agent_permission_audit WHERE run_id = r.run_id), '')
             FROM agent_runs r
             WHERE r.run_id = 'run-transient-body'",
            [],
            |row| row.get(0),
        )?;
        assert!(!persisted.contains(marker));
        assert!(!persisted.contains(indexed_excerpt));
        let assistant_content: String = conn.query_row(
            "SELECT content FROM session_messages
             WHERE session_id = ?1 AND turn_id = 'turn-transient-body' AND role = 'assistant'",
            [session.session_id],
            |row| row.get(0),
        )?;
        assert_eq!(assistant_content, "已依据附件完成概述。");
        assert!(!assistant_content.contains(marker));
        let explicit_references_json: String = conn.query_row(
            "SELECT explicit_references_json FROM session_messages
             WHERE session_id = ?1 AND turn_id = 'turn-transient-body' AND role = 'user'",
            [session.session_id],
            |row| row.get(0),
        )?;
        assert!(explicit_references_json.contains("notes/transient.md"));
        assert!(!explicit_references_json.contains(marker));
        assert!(!explicit_references_json.contains(indexed_excerpt));
        let (run_state, tool_event_count, safe_tool_summary): (String, i64, String) =
            conn.query_row(
                "SELECT r.status,
                        (SELECT COUNT(*) FROM agent_run_events
             WHERE run_id = 'run-transient-body'
                           AND event_type IN ('tool_started', 'tool_completed')),
                        (SELECT payload_json FROM agent_run_events
                         WHERE run_id = 'run-transient-body' AND event_type = 'tool_completed'
                         ORDER BY event_seq DESC LIMIT 1)
                 FROM agent_runs r WHERE r.run_id = 'run-transient-body'",
                [],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)),
            )?;
        assert_eq!(run_state, "completed");
        assert_eq!(tool_event_count, 2);
        assert!(safe_tool_summary.contains("工具调用完成"));
        assert!(!safe_tool_summary.contains(marker));
        let evidence_count: i64 = conn.query_row(
            "SELECT COUNT(*) FROM session_evidence
             WHERE origin_run_id = 'run-transient-body'",
            [],
            |row| row.get(0),
        )?;
        assert_eq!(evidence_count, 1);
        let (tool_name, arguments_summary, result_summary, success): (
            String,
            String,
            String,
            i64,
        ) = conn.query_row(
            "SELECT tool_name, arguments_summary, result_summary, success
             FROM tool_audit WHERE run_id = 'run-transient-body'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )?;
        assert_eq!(tool_name, "get_context_packets");
        assert_eq!(arguments_summary, "shape=object, keys=0");
        assert_eq!(result_summary, "shape=object, keys=2");
        assert_eq!(success, 1);
        assert!(!result_summary.contains(marker));
        let (permission_count, permission_status): (i64, String) = conn.query_row(
            "SELECT COUNT(*), MIN(result_status) FROM agent_permission_audit
             WHERE run_id = 'run-transient-body' AND tool_name = 'get_context_packets'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?)),
        )?;
        assert_eq!(permission_count, 1);
        assert_eq!(permission_status, "executed");
        Ok(())
    })
    .expect("inspect completed run storage");
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
            retrieval_reason: "explicit_reference".into(),
        }],
        retrieval_scope: Default::default(),
        local_retrieval_packets: vec![],
        recent_messages: vec![],
        conversation_memory: None,
        prompt_profile: Default::default(),
        previous_run_summary: None,
    };

    let prompt = context.prompt_with_domain_plan(&context.domain_plan());
    assert!(prompt.contains("内容依据"));
    assert!(prompt.contains("写法参考"));
    assert!(prompt.contains("role=\"reference\""));
    assert!(prompt.contains("用户明确附上的事实"));
    assert!(!prompt.contains("当前活动文档"));
}

#[test]
fn normal_context_includes_six_prior_messages_but_never_duplicates_the_current_turn() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "history-first".into(),
            run_id: "history-run-first".into(),
            turn_id: "history-turn-first".into(),
            message: "Why did you search the web?".into(),
            content_parts: None,
            explicit_references: vec![],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::Conversation,
                ..envelope()
            },
        },
    )
    .expect("first accepted run");
    for (state_version, state) in [
        (0, crate::ai_runtime::run_contract::RunState::Preparing),
        (1, crate::ai_runtime::run_contract::RunState::Running),
    ] {
        AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: "history-run-first".into(),
                state_version,
                event_type: crate::ai_runtime::run_contract::RunEventType::StageChanged,
                payload: crate::ai_runtime::run_contract::RunEventPayload::StageChanged {
                    state,
                    stage: "history fixture".into(),
                },
            },
        )
        .expect("advance first run");
    }
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "history-run-first".into(),
            state_version: 2,
            event_type: crate::ai_runtime::run_contract::RunEventType::CapabilityDegraded,
            payload: crate::ai_runtime::run_contract::RunEventPayload::CapabilityDegraded {
                capability: "web.search".into(),
                code: crate::ai_runtime::run_contract::SafeRunErrorCode::WebProviderTimeout,
                retryable: true,
                attempt_count: 2,
                message: "联网核实暂不可用，已继续生成受约束答复。".into(),
            },
        },
    )
    .expect("record safe Web degradation");
    AgentRunRepository::finalize(
        &db,
        FinalizeRunInput {
            run_id: "history-run-first".into(),
            state_version: 2,
            content: "The previous web attempt timed out, so I should explain the degradation."
                .into(),
            evidence_ids: vec![],
            citation_map: serde_json::json!({}),
        },
    )
    .expect("first run finalized");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "history-current".into(),
            run_id: "history-run-current".into(),
            turn_id: "history-turn-current".into(),
            message: "What went wrong just now?".into(),
            content_parts: None,
            explicit_references: vec![],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::Conversation,
                ..envelope()
            },
        },
    )
    .expect("current accepted run");

    let context =
        RunContextAssembler::assemble(&db, None, &session.session_key, "history-run-current")
            .expect("assembled conversation context");
    assert_eq!(context.recent_messages.len(), 2);
    assert!(context
        .recent_messages
        .iter()
        .all(|message| message.seq < context.message_seq_first));
    let prior_summary = context
        .previous_run_summary
        .as_deref()
        .expect("previous Run safety summary");
    assert!(prior_summary.contains("status=completed"));
    assert!(prior_summary.contains("webResult=degraded"));
    assert!(prior_summary.contains("attemptCount=2"));
    assert!(prior_summary.contains("safeCode=agent_run_web_provider_timeout"));
    assert!(!prior_summary.contains("Why did you search the web?"));

    let messages = context.messages_with_domain_plan(&context.domain_plan());
    let serialized = serde_json::to_string(&messages).expect("messages JSON");
    assert_eq!(serialized.matches("What went wrong just now?").count(), 1);
    assert!(serialized.contains("Why did you search the web?"));
    assert!(serialized.contains("previous web attempt timed out"));
    assert!(messages[0]
        .content
        .text_content()
        .contains("When the mode is online, prefer calling web_search"));
    assert!(messages[0].content.text_content().contains("Local date"));
}

#[test]
fn previous_run_safety_does_not_treat_local_evidence_as_web_success() {
    let db = Database::open_in_memory().expect("database");
    let session = NormalSessionRepository::create(&db).expect("session");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "local-evidence-first".into(),
            run_id: "local-evidence-run-first".into(),
            turn_id: "local-evidence-turn-first".into(),
            message: "Summarize the attached local note.".into(),
            content_parts: None,
            explicit_references: vec![],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: envelope(),
        },
    )
    .expect("first accepted run");
    let message_seq_first = db
        .with_read_conn(|conn| {
            conn.query_row(
                "SELECT m.seq
                 FROM agent_runs r
                 JOIN session_messages m
                   ON m.session_id = r.session_id AND m.turn_id = r.turn_id AND m.role = 'user'
                 WHERE r.run_id = ?1",
                ["local-evidence-run-first"],
                |row| row.get::<_, i64>(0),
            )
            .map_err(Into::into)
        })
        .expect("message sequence");
    let local = AgentEvidenceRepository::register_local(
        &db,
        LocalEvidenceInput {
            session_id: session.session_id,
            run_id: "local-evidence-run-first".into(),
            message_seq_first,
            material_role: MaterialRole::Reference,
            title: "Local note".into(),
            source_path: "notes/local.md".into(),
            source_span_start: 0,
            source_span_end: 12,
            heading_path: None,
            content_hash: "local-note-hash".into(),
            retrieval_reason: Some("explicit_reference".into()),
            score: None,
        },
    )
    .expect("local evidence");
    for (state_version, state) in [
        (0, crate::ai_runtime::run_contract::RunState::Preparing),
        (1, crate::ai_runtime::run_contract::RunState::Running),
    ] {
        AgentRunRepository::append_event(
            &db,
            AppendRunEventInput {
                run_id: "local-evidence-run-first".into(),
                state_version,
                event_type: crate::ai_runtime::run_contract::RunEventType::StageChanged,
                payload: crate::ai_runtime::run_contract::RunEventPayload::StageChanged {
                    state,
                    stage: "local evidence fixture".into(),
                },
            },
        )
        .expect("advance first run");
    }
    AgentRunRepository::append_event(
        &db,
        AppendRunEventInput {
            run_id: "local-evidence-run-first".into(),
            state_version: 2,
            event_type: crate::ai_runtime::run_contract::RunEventType::EvidenceRegistered,
            payload: crate::ai_runtime::run_contract::RunEventPayload::EvidenceRegistered {
                evidence_id: local.evidence_id.to_string(),
            },
        },
    )
    .expect("local evidence event");
    AgentRunRepository::finalize(
        &db,
        FinalizeRunInput {
            run_id: "local-evidence-run-first".into(),
            state_version: 2,
            content: "Local-only summary.".into(),
            evidence_ids: vec![local.evidence_id],
            citation_map: serde_json::json!({}),
        },
    )
    .expect("first run finalized");
    AgentRunRepository::accept(
        &db,
        AcceptRunInput {
            session_id: session.session_id,
            session_key: session.session_key.clone(),
            client_request_id: "local-evidence-current".into(),
            run_id: "local-evidence-run-current".into(),
            turn_id: "local-evidence-turn-current".into(),
            message: "What happened previously?".into(),
            content_parts: None,
            explicit_references: vec![],
            context_scope: Default::default(),
            display_mentions: vec![],
            explicit_action: None,
            envelope: ExecutionEnvelope {
                context: ContextMode::Conversation,
                ..envelope()
            },
        },
    )
    .expect("current accepted run");

    let context = RunContextAssembler::assemble(
        &db,
        None,
        &session.session_key,
        "local-evidence-run-current",
    )
    .expect("assembled context");
    let prior_summary = context
        .previous_run_summary
        .as_deref()
        .expect("previous Run summary");

    assert!(prior_summary.contains("webAttempted=false"));
    assert!(prior_summary.contains("webResult=skipped"));
}
