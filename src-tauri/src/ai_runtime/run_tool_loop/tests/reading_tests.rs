use super::*;
use serde_json::json;
use sha2::{Digest, Sha256};

fn begin(
    state: &AppState,
    accepted: &crate::ai_runtime::run_contract::AssistantRunAccepted,
    sink: &RecordingSink,
) {
    let version =
        RunEngine::mark_preparing_with_sink(&state.db, &accepted.session, &accepted.run_id, sink)
            .unwrap();
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: version,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "reading fixture".into(),
                stage_code: None,
            },
        },
    )
    .unwrap();
}

fn seed_page(db: &Database, url: &str, body: &str) {
    use crate::llm::fetch_web_page::{PageFetchCacheScope, PAGE_FETCH_CACHE_BROKER_VERSION};
    let scope = PageFetchCacheScope::native(None, PAGE_FETCH_CACHE_BROKER_VERSION);
    let mut hash = Sha256::new();
    for part in [
        "default",
        &scope.provider_id,
        &scope.provider_kind,
        &scope.provider_config_hash,
        &scope.broker_version,
    ] {
        hash.update(part.as_bytes());
        hash.update(b"\0");
    }
    hash.update(url.as_bytes());
    let key = hex::encode(hash.finalize());
    db.with_conn(|conn| {
        conn.execute("INSERT OR REPLACE INTO web_page_cache
            (url_hash,title,body_text,fetched_at,expires_at,provider_id,provider_kind,provider_config_hash,broker_version)
            VALUES (?1,'Fixture',?2,datetime('now'),datetime('now','+1 day'),?3,?4,?5,?6)",
            rusqlite::params![key,body,scope.provider_id,scope.provider_kind,scope.provider_config_hash,scope.broker_version])?;
        Ok(())
    }).unwrap();
}

#[tokio::test]
async fn plan_a_multi_url_windows_fit_escaped_json_and_preserve_upstream_bounds() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::new(directory.path().join("data")).unwrap();
    let accepted = RunIntake::start(&state.db, request()).unwrap();
    let context = RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .unwrap();
    let sink = RecordingSink::default();
    begin(&state, &accepted, &sink);
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        vec![CapabilityId::new("web.search")],
        RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        Vec::new(),
    )
    .with_allowed_tool_names(&["web_fetch".into()]);
    let urls = (0..8)
        .map(|index| format!("https://example.com/window-{index}"))
        .collect::<Vec<_>>();
    for (index, url) in urls.iter().enumerate() {
        seed_page(
            &state.db,
            url,
            &format!(
                "{index}{}",
                "\u{0001}".repeat(if index == 0 { 13000 } else { 5000 })
            ),
        );
    }
    let call = ToolCall::new(
        "escaped-batch",
        "web_fetch",
        json!({"urls":urls}).to_string(),
    );
    let result = executor.execute(&accepted.run_id, &call, 1).await.unwrap();
    assert!(result.success, "{:?}", result.error);
    assert!(
        result.output.get("excerptWindow").is_none(),
        "a batch cannot share a continuation pointer"
    );
    assert_eq!(result.output["count"], 8);
    let (message, shortened) =
        crate::ai_runtime::agent_tool_loop::tool_result_message(&call, &result, 7, 23, 5).unwrap();
    assert!(!shortened);
    assert!(message.content.text_content().chars().count() <= 32000);
    let payload: serde_json::Value = serde_json::from_str(&message.content.text_content()).unwrap();
    for packet in payload["output"]["results"].as_array().unwrap() {
        let visible = packet["excerpt"].as_str().unwrap();
        let window = &packet["excerptWindow"];
        let length = visible.chars().count();
        assert_eq!(window["endChar"], length);
        assert_eq!(window["nextStartChar"], length);
        assert!(
            length > 0 && length < 2000,
            "JSON escape overhead must shorten each oversized window"
        );
        let id = packet["evidenceId"].as_i64().unwrap();
        let stored: String = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT bounded_excerpt FROM session_evidence WHERE id=?1",
                    [id],
                    |row| row.get(0),
                )?)
            })
            .unwrap();
        assert_eq!(stored, visible);
    }
    assert_eq!(
        result.output["results"][0]["excerptWindow"]["upstreamCompleteness"],
        "bounded"
    );
    let call = ToolCall::new(
        "bounded-tail",
        "web_fetch",
        json!({"urls":[urls[0]],"startChar":11990}).to_string(),
    );
    let tail = executor.execute(&accepted.run_id, &call, 2).await.unwrap();
    assert_eq!(tail.output["excerptWindow"]["endChar"], 12000);
    assert_eq!(
        tail.output["excerptWindow"]["nextStartChar"],
        serde_json::Value::Null
    );
    assert_eq!(
        tail.output["excerptWindow"]["upstreamCompleteness"],
        "bounded"
    );
    let mixed = executor
        .execute(
            &accepted.run_id,
            &ToolCall::new(
                "mixed",
                "web_fetch",
                json!({"urls":[urls[1],"https://example.net/missing"],"startChar":2000})
                    .to_string(),
            ),
            3,
        )
        .await
        .unwrap();
    assert!(mixed.success);
    assert_eq!(mixed.output["count"], 1);
    assert_eq!(
        mixed.output["failedUrls"],
        json!(["https://example.net/missing"])
    );
    assert_eq!(executor.web_attempt_count(), 1);
}

#[tokio::test]
async fn plan_a_mcp_snapshot_continuation_preserves_unknown_completeness() {
    use crate::ai_runtime::mcp_runtime_registry as registry;
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::new(directory.path().join("data")).unwrap();
    registry::upsert_web_evidence_provider(
        &state.db,
        &registry::WebEvidenceProviderInput {
            id: "plan-a-mcp".into(),
            name: "Fixture".into(),
            kind: "mcp".into(),
            enabled: true,
            transport_kind: "stdio".into(),
            transport_config_json:
                crate::ai_runtime::mcp_stdio_test_support::contract_mcp_stdio_transport_config(
                    "search-fetch",
                    "1",
                )
                .to_string(),
            credential_refs_json: "{}".into(),
            web_search_mapping_json: None,
            web_fetch_mapping_json: Some(r#"{"tool":"fetch","urlArg":"url"}"#.into()),
        },
    )
    .unwrap();
    let accepted = RunIntake::start(&state.db, request()).unwrap();
    let context = RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .unwrap();
    let sink = RecordingSink::default();
    begin(&state, &accepted, &sink);
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        vec![CapabilityId::new("web.search")],
        RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        registry::list_enabled_web_provider_mappings(&state.db).unwrap(),
    )
    .with_allowed_tool_names(&["web_fetch".into()]);
    let url = "https://source.invalid/contract";
    let first = executor
        .execute(
            &accepted.run_id,
            &ToolCall::new("mcp-first", "web_fetch", json!({"urls":[url]}).to_string()),
            1,
        )
        .await
        .unwrap();
    assert!(first.success, "{:?}", first.error);
    let next = first.output["excerptWindow"]["nextStartChar"]
        .as_u64()
        .unwrap();
    let tail = executor
        .execute(
            &accepted.run_id,
            &ToolCall::new(
                "mcp-tail",
                "web_fetch",
                json!({"urls":[url],"startChar":next}).to_string(),
            ),
            2,
        )
        .await
        .unwrap();
    assert!(tail.output["results"][0]["excerpt"]
        .as_str()
        .unwrap()
        .contains("fact-web-128=value-128"));
    assert_eq!(
        tail.output["excerptWindow"]["upstreamCompleteness"],
        "unknown"
    );
    assert_eq!(
        tail.output["excerptWindow"]["nextStartChar"],
        serde_json::Value::Null
    );
    assert_eq!(executor.web_attempt_count(), 1);
    assert_eq!(
        registry::web_evidence_provider_health(&state.db, "plan-a-mcp", "web.fetch")
            .unwrap()
            .unwrap()
            .success_count,
        1
    );
}

#[tokio::test]
async fn plan_a_executor_reads_tail_without_refetch_and_binds_exact_excerpt() {
    let directory = tempfile::tempdir().unwrap();
    let state = AppState::new(directory.path().join("data")).unwrap();
    let accepted = RunIntake::start(&state.db, request()).unwrap();
    let context = RunContextAssembler::assemble(
        &state.db,
        None,
        &accepted.session.session_key,
        &accepted.run_id,
    )
    .unwrap();
    let sink = RecordingSink::default();
    let version =
        RunEngine::mark_preparing_with_sink(&state.db, &accepted.session, &accepted.run_id, &sink)
            .unwrap();
    AgentRunRepository::append_event(
        &state.db,
        AppendRunEventInput {
            run_id: accepted.run_id.clone(),
            state_version: version,
            event_type: RunEventType::StageChanged,
            payload: RunEventPayload::StageChanged {
                state: RunState::Running,
                stage: "reading fixture".into(),
                stage_code: None,
            },
        },
    )
    .unwrap();
    let executor = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        vec![CapabilityId::new("web.search")],
        RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        Vec::new(),
    )
    .with_allowed_tool_names(&["web_fetch".into()]);
    let url = "https://example.com/plan-a-page";
    let body = format!(
        "{}{}tail-only-answer",
        "中🦀\"\\\n".repeat(500),
        "后🦀\"\\\n".repeat(500)
    );
    seed_page(&state.db, url, &body);
    let mut start = 0;
    let mut read = String::new();
    let mut labels = BTreeSet::new();
    loop {
        let call = ToolCall::new(
            format!("page-{start}"),
            "web_fetch",
            json!({"urls":[url],"startChar":start}).to_string(),
        );
        let result = executor.execute(&accepted.run_id, &call, 1).await.unwrap();
        assert!(result.success, "{:?}", result.error);
        let (message, shortened) =
            crate::ai_runtime::agent_tool_loop::tool_result_message(&call, &result, 7, 23, 5)
                .unwrap();
        assert!(!shortened, "ledger excerpts cannot change in the loop");
        let visible: serde_json::Value =
            serde_json::from_str(&message.content.text_content()).unwrap();
        assert_eq!(visible["output"], result.output);
        let packet = &result.output["results"][0];
        let excerpt = packet["excerpt"].as_str().unwrap();
        assert!(!excerpt.is_empty());
        let id = result.output["evidenceIds"][0].as_i64().unwrap();
        let (stored, label): (String, String) = state
            .db
            .with_read_conn(|conn| {
                Ok(conn.query_row(
                    "SELECT bounded_excerpt,citation_label FROM session_evidence WHERE id=?1",
                    [id],
                    |row| Ok((row.get(0)?, row.get(1)?)),
                )?)
            })
            .unwrap();
        assert_eq!(stored, excerpt);
        let binding=crate::ai_runtime::agent_evidence_repository::AgentEvidenceRepository::list_selected_current_run_web_citation_links(&state.db,&accepted.run_id,&[id]).unwrap();
        assert_eq!(packet["citation_label"], binding[0].label);
        assert_eq!(
            packet["content_hash"],
            crate::cas::hash::content_hash_str(&body)
        );
        assert!(packet.get("contentHash").is_none());
        assert!(
            packet.get("citationLabel").is_none(),
            "only the existing citation field may carry a label"
        );
        assert!(labels.insert(label));
        read.push_str(excerpt);
        seed_page(
            &state.db,
            url,
            "changed page must not replace the Run snapshot",
        );
        let Some(next) = result.output["excerptWindow"]["nextStartChar"].as_u64() else {
            break;
        };
        assert_eq!(next as usize, start + excerpt.chars().count());
        start = next as usize;
    }
    assert_eq!(
        read, body,
        "the actual model observations must include the tail"
    );
    assert_eq!(
        executor.web_attempt_count(),
        1,
        "continuation never invokes Broker again"
    );
    assert_eq!(executor.run_web_evidence.lock().unwrap().domains.len(), 1);
    let count = executor.run_web_evidence.lock().unwrap().evidence_ids.len();
    for (offset, status) in [
        (body.chars().count(), "eof"),
        (body.chars().count() + 1, "range_out_of_bounds"),
    ] {
        let call = ToolCall::new(
            format!("end-{offset}"),
            "web_fetch",
            json!({"urls":[url],"startChar":offset}).to_string(),
        );
        let result = executor.execute(&accepted.run_id, &call, 2).await.unwrap();
        assert_eq!(result.output["results"][0]["status"], status);
        assert_eq!(result.success, status == "eof");
        assert_eq!(result.output["count"], 0);
    }
    let repeated = executor
        .execute(
            &accepted.run_id,
            &ToolCall::new("repeat", "web_fetch", json!({"urls":[url]}).to_string()),
            3,
        )
        .await
        .unwrap();
    assert!(repeated.success);
    assert_eq!(
        executor.run_web_evidence.lock().unwrap().evidence_ids.len(),
        count
    );
    assert_eq!(executor.web_attempt_count(), 1);
    let fresh = NormalRunToolExecutor::new(
        &state,
        None,
        &accepted,
        &context,
        vec![CapabilityId::new("web.search")],
        RunBudgetPolicy::for_envelope(&context.envelope),
        &sink,
        Vec::new(),
    )
    .with_allowed_tool_names(&["web_fetch".into()]);
    let missing = fresh
        .execute(
            &accepted.run_id,
            &ToolCall::new(
                "missing-snapshot",
                "web_fetch",
                json!({"urls":[url],"startChar":2000}).to_string(),
            ),
            4,
        )
        .await
        .unwrap();
    assert!(!missing.success);
    assert_eq!(
        missing.output["results"][0]["status"],
        "snapshot_unavailable"
    );
    assert_eq!(fresh.web_attempt_count(), 0);
}
