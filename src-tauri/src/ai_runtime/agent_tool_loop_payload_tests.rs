//! Payload-shaping contracts for the Agent tool loop.
//!
//! Split out of `agent_tool_loop_tests.rs` when those payload tests pushed that
//! file past the repository's file-size budget (`npm run size:check`).

use serde_json::json;

/// The model-visible projection must carry a decision, not the Host's
/// accounting. Exact remaining quotas and Provider usage are what the model
/// used to repeat internal budget back to the user.
#[test]
fn loop_projection_reports_a_decision_not_host_accounting() {
    use crate::ai_runtime::agent_tool_loop::{
        observation_failure_type, LoopProjection, NEXT_ACTION_CONTINUE, NEXT_ACTION_SYNTHESIZE,
    };
    use crate::ai_runtime::{ToolCall, ToolCallResult};

    let call = ToolCall::new("read-1", "read_note", "{}");
    let result = |success: bool, error: Option<&str>| ToolCallResult {
        tool_name: "read_note".into(),
        success,
        output: json!({"path": "notes/a.md", "content": "正文"}),
        error: error.map(str::to_string),
        duration_ms: 4,
        tokens_used: None,
    };

    let outcome = |result: &ToolCallResult, projection: LoopProjection| {
        let (message, _) =
            crate::ai_runtime::agent_tool_loop::tool_result_message(&call, result, projection)
                .unwrap();
        serde_json::from_str::<serde_json::Value>(&message.content.text_content()).unwrap()
    };

    let ok = result(true, None);
    let payload = outcome(
        &ok,
        LoopProjection::for_observation(true, observation_failure_type(&ok), false),
    );
    assert_eq!(payload["success"], true);
    assert_eq!(payload["loopObservation"]["canContinue"], true);
    assert_eq!(payload["loopObservation"]["mustSynthesize"], false);
    assert_eq!(
        payload["loopObservation"]["failureType"],
        serde_json::Value::Null
    );
    assert_eq!(
        payload["loopObservation"]["nextAction"],
        NEXT_ACTION_CONTINUE
    );
    assert_eq!(payload["loopObservation"]["historicalObservation"], false);

    // A failed observation keeps its failure and reports a bounded kind.
    let failed = result(false, Some("web_read_unavailable"));
    let payload = outcome(
        &failed,
        LoopProjection::for_observation(false, observation_failure_type(&failed), false),
    );
    assert_eq!(payload["success"], false);
    assert_eq!(payload["error"], "web_read_unavailable");
    assert_eq!(
        payload["loopObservation"]["failureType"],
        "read_unavailable"
    );
    assert_eq!(payload["loopObservation"]["mustSynthesize"], false);
    assert_eq!(
        payload["loopObservation"]["nextAction"],
        NEXT_ACTION_SYNTHESIZE
    );
    assert_eq!(payload["loopObservation"]["canContinue"], false);

    // No internal accounting may reappear under any projection.
    for key in [
        "remainingModelTurns",
        "remainingToolCalls",
        "remainingCategoryCalls",
        "remainingBudgetMs",
        "webUsage",
    ] {
        assert!(
            payload["loopObservation"].get(key).is_none(),
            "{key} must never be model-visible"
        );
        assert!(
            payload.get(key).is_none(),
            "{key} must never be model-visible at the envelope level"
        );
    }
    // The widest projection still cannot widen the message past its budget.
    let widest = outcome(&ok, LoopProjection::widest());
    assert_eq!(widest["loopObservation"]["historicalObservation"], true);
    assert!(
        serde_json::to_string(&widest).unwrap().chars().count()
            <= serde_json::to_string(&payload).unwrap().chars().count() + 512,
        "the reserved projection must stay close to the published one"
    );
}

#[test]
fn plan_a_known_prose_slots_shrink_without_inventing_read_ranges() {
    for field in ["content", "excerpt", "text", "body"] {
        let payload = json!({"success":true,"output":{field:"\u{0001}中🦀\\\"".repeat(2000),"resource_id":"fixture"},"error":null});
        let (text, shortened) = crate::ai_runtime::agent_tool_loop::fit_tool_payload(
            &payload,
            800,
            &payload.to_string(),
        )
        .unwrap();
        assert!(shortened);
        assert!(text.chars().count() <= 800);
        let parsed: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(parsed["success"], true);
        assert_eq!(parsed["output"]["resource_id"], "fixture");
        assert!(parsed["output"][field]
            .as_str()
            .is_some_and(|text| !text.is_empty()));
        assert!(parsed["output"].get("nextStartByte").is_none());
    }
}

#[test]
fn plan_a_external_observation_is_sized_before_its_evidence_prefix() {
    use crate::ai_runtime::agent_tool_loop::{
        prepare_tool_result, tool_result_message, LoopProjection,
    };
    use crate::ai_runtime::{ToolCall, ToolCallResult};
    let body = "\u{0001}".repeat(4000);
    let mut result = ToolCallResult {
        tool_name: "mcp_fixture_read".into(),
        success: true,
        output: json!({"content":body,"sourceRef":format!("E{}",i64::MAX)}),
        error: None,
        duration_ms: 1,
        tokens_used: None,
    };
    prepare_tool_result(&mut result).unwrap();
    let excerpt: String = result.output["content"]
        .as_str()
        .unwrap()
        .chars()
        .take(2000)
        .collect();
    assert!(
        excerpt.chars().count() < 2000,
        "escaping shrinks even the evidence prefix"
    );
    result.output["sourceRef"] = json!("E1");
    let (message, changed) = tool_result_message(
        &ToolCall::new("read", "mcp_fixture_read", "{}"),
        &result,
        LoopProjection::default(),
    )
    .unwrap();
    assert!(!changed);
    let payload: serde_json::Value = serde_json::from_str(&message.content.text_content()).unwrap();
    assert!(payload["output"]["content"]
        .as_str()
        .unwrap()
        .starts_with(&excerpt));
    assert_eq!(payload["output"]["truncated"], true);
}

#[test]
fn plan_a_minimum_metadata_overflow_is_a_projection_error() {
    let original = json!({"success":true,"output":{"receiptId":"x".repeat(1000)},"error":null});
    let text = original.to_string();
    assert!(crate::ai_runtime::agent_tool_loop::fit_tool_payload(&original, 40, &text).is_err());
    assert_eq!(original["success"], true);
}

#[test]
fn plan_a_compaction_preserves_write_receipt_and_latest_user_constraint() {
    use crate::ai_runtime::agent_tool_loop::{compact_tool_observations, AgentModelTurnBudget};
    use crate::ai_runtime::{LlmMessage, MessageRole, ToolCall};
    let receipt=json!({"success":true,"output":{"receiptId":"write-1","content":"x".repeat(9000),"targetFileHash":"committed"},"error":null}).to_string();
    let mut messages = vec![
        LlmMessage {
            role: MessageRole::Assistant,
            content: "".into(),
            tool_calls: Some(vec![ToolCall::new("write-1", "memory_write", "{}")]),
            tool_call_id: None,
            reasoning_content: None,
        },
        LlmMessage {
            role: MessageRole::Tool,
            content: receipt.clone().into(),
            tool_call_id: Some("write-1".into()),
            tool_calls: None,
            reasoning_content: None,
        },
        LlmMessage {
            role: MessageRole::Assistant,
            content: "".into(),
            tool_calls: Some(vec![ToolCall::new("latest", "read_note", "{}")]),
            tool_call_id: None,
            reasoning_content: None,
        },
        LlmMessage {
            role: MessageRole::Tool,
            content: json!({"success":false,"error":"unavailable"})
                .to_string()
                .into(),
            tool_call_id: Some("latest".into()),
            tool_calls: None,
            reasoning_content: None,
        },
        LlmMessage {
            role: MessageRole::User,
            content: "最新纠正必须保留".into(),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        },
    ];
    let before = serde_json::to_value(&messages).unwrap();
    assert!(compact_tool_observations(
        &mut messages,
        &[],
        AgentModelTurnBudget {
            max_prompt_tokens: Some(100),
            ..Default::default()
        }
    )
    .is_empty());
    assert_eq!(serde_json::to_value(&messages).unwrap(), before);
}
#[path = "agent_tool_loop/replay_tests.rs"]
mod replay_tests;

/// Cross-layer invariant: the Host identity of a terminal body is decided by the
/// loop's own classification and never by the text. A model answer that copies
/// the Host limitation opening verbatim is still model output, and the real Host
/// fallback keeps its typed identity. This is the defect class that let prose
/// earn a validation exemption by prefix.
#[test]
fn model_prose_cannot_impersonate_a_host_authored_limitation() {
    use crate::ai_runtime::agent_tool_loop::{
        AgentTerminalType, AgentToolLoopOutcome, EVIDENCE_LIMITED_RESPONSE,
    };
    use crate::ai_runtime::run_engine::legacy_terminal_records::legacy_record_is_host_authored;

    let model_outcome = AgentToolLoopOutcome {
        content: EVIDENCE_LIMITED_RESPONSE.to_string(),
        terminal: AgentTerminalType::ModelAnswer,
        finish_reason: "stop".into(),
        final_submission: None,
        model_turns: 1,
        tool_calls: 0,
        prompt_tokens: 1,
        completion_tokens: 1,
        total_tokens: 2,
    };
    assert!(
        !model_outcome.terminal.is_host_authored(),
        "identical text must not change who produced it"
    );
    // Every Producer in the loop assigns a model terminal for Provider prose.
    assert!(!AgentTerminalType::RepairedModelAnswer.is_host_authored());

    let host_outcome = AgentToolLoopOutcome {
        terminal: AgentTerminalType::HostEvidenceLimited,
        ..model_outcome
    };
    assert!(host_outcome.terminal.is_host_authored());
    assert_eq!(
        AgentTerminalType::HostEvidenceLimited.as_str(),
        "host_evidence_limited"
    );

    // The legacy read adapter still recognises a persisted Host record, but it
    // demands the whole disclosure shape rather than the bare opening phrase.
    assert!(legacy_record_is_host_authored(EVIDENCE_LIMITED_RESPONSE));
    for prefix_only in [
        "本轮未取得足够的可核验来源正文。",
        "本轮未取得足够的可核验来源正文，不过我认为事实就是这样。",
        "本轮已取得可核验资料，结论如下。",
    ] {
        assert!(
            !legacy_record_is_host_authored(prefix_only),
            "the bare opening phrase must not grant a validation exemption: {prefix_only}"
        );
    }
    assert!(!legacy_record_is_host_authored(
        "《劳动合同法》于 2012 年修订。"
    ));
}

/// A Host-authored limitation must never carry a synthetic Provider stop reason
/// that the finalisation layer would have to decode.
#[test]
fn host_limitation_reports_no_synthetic_provider_stop_reason() {
    use crate::ai_runtime::agent_tool_loop::AgentTerminalType;

    let limitation = crate::ai_runtime::agent_tool_loop::evidence_limited_outcome_for_test(
        "本轮未取得足够的可核验来源正文，无法确认。".into(),
        2,
        3,
        10,
        20,
        30,
    );
    assert_eq!(limitation.terminal, AgentTerminalType::HostEvidenceLimited);
    assert_eq!(limitation.finish_reason, "stop");
    assert!(limitation.final_submission.is_none());
    assert_eq!(limitation.model_turns, 2);
    assert_eq!(limitation.tool_calls, 3);
    assert_eq!(limitation.total_tokens, 30);
}

/// Cross-layer invariant that replaces the former prefix-drift check: every
/// bounded limitation the Host can publish is still produced from the shared
/// legacy constants, so persisted records stay recognisable, while a *new* Run
/// is identified by its terminal type alone.
#[test]
fn every_host_authored_bounded_limitation_stays_recognisable_in_records() {
    use crate::ai_runtime::agent_tool_loop::EVIDENCE_LIMITED_RESPONSE;
    use crate::ai_runtime::run_engine::legacy_terminal_records::{
        legacy_record_is_host_authored, EVIDENCE_LIMITED_RESPONSE_PREFIX,
        EVIDENCE_LIMITED_WITH_EVIDENCE_PREFIX,
    };

    // Every body `NormalRunToolExecutor::evidence_limited_response` can build
    // that carries a structural verdict. The no-failure-code branch
    // (`现有材料不足以支持答复中的事实…`) has no fixed closing verdict, so a
    // persisted record of it cannot be told apart from model prose; it is
    // deliberately absent from this list rather than given a loose match.
    for published in [
        EVIDENCE_LIMITED_RESPONSE.to_string(),
        format!("{EVIDENCE_LIMITED_RESPONSE_PREFIX}。检索服务请求超时，未取得可用结果。"),
        format!("{EVIDENCE_LIMITED_RESPONSE_PREFIX}。当前没有可用的联网服务，未取得外部资料。"),
        format!("{EVIDENCE_LIMITED_RESPONSE_PREFIX}。联网服务的身份验证失败，未取得外部资料。"),
        format!("{EVIDENCE_LIMITED_RESPONSE_PREFIX}。返回的材料没有可核实的正文，无法据此确认当前情况。"),
        format!(
            "{EVIDENCE_LIMITED_RESPONSE_PREFIX}。搜索取得了以下未核实线索，但正文读取或核验未成功，无法据此确认当前情况：\n\n- 标题（example.com）\n  https://example.com/x"
        ),
        format!(
            "{EVIDENCE_LIMITED_WITH_EVIDENCE_PREFIX}，但未能完成最终来源关联；已取得的正文不会被当作读取失败。请重试以完成答复。"
        ),
    ] {
        assert!(
            legacy_record_is_host_authored(&published),
            "legacy Host limitation is no longer recognised: {published}"
        );
    }
    // Ordinary prose is still not treated as a Host limitation.
    assert!(!legacy_record_is_host_authored(
        "《劳动合同法》于 2012 年修订。"
    ));
}

/// An over-budget ranged read must stay valid JSON and keep the pointer that
/// lets the model continue reading.
#[test]
fn over_budget_ranged_read_payload_stays_valid_and_keeps_its_continuation_pointer() {
    use crate::ai_runtime::agent_tool_loop::fit_tool_payload;

    let body: String = "中华人民共和国劳动合同法".repeat(1_000);
    let payload = json!({
        "success": true,
        "output": {
            "path": "notes/law.md",
            "content": body,
            "truncated": true,
            "contentHash": "hash-1",
            "sourceSpan": { "start": 4096, "end": 4096 + body.len() },
            "nextStartByte": 4096 + body.len(),
        },
        "error": serde_json::Value::Null,
        "loopObservation": {
            "canContinue": true, "mustSynthesize": false, "failureType": null,
            "nextAction": "continue_with_available_tools", "historicalObservation": false
        },
    });
    let serialized = serde_json::to_string(&payload).expect("serialize");
    assert!(serialized.chars().count() > 8_000);

    let (fitted, truncated) = fit_tool_payload(&payload, 8_000, &serialized).unwrap();
    assert!(truncated);
    assert!(fitted.chars().count() <= 8_000, "still over budget");

    let parsed: serde_json::Value = serde_json::from_str(&fitted).expect("valid JSON");
    let output = parsed.get("output").expect("output survives");
    let content = output
        .get("content")
        .and_then(|v| v.as_str())
        .expect("content");
    assert!(!content.is_empty());
    assert_eq!(
        output.get("truncated"),
        Some(&serde_json::Value::Bool(true))
    );
    let start = output
        .get("sourceSpan")
        .and_then(|span| span.get("start"))
        .and_then(serde_json::Value::as_u64)
        .expect("span start");
    let next = output
        .get("nextStartByte")
        .and_then(serde_json::Value::as_u64)
        .expect("continuation pointer survives");
    // The pointer must describe the prefix the model can actually see.
    assert_eq!(next, start + content.len() as u64);
}

/// Paging through one note is progress: the range is part of the identity.
#[test]
fn consecutive_pages_of_one_note_are_not_reported_as_no_progress() {
    use crate::ai_runtime::agent_tool_loop::safe_progress_identities;

    let page = |next: u64| {
        json!({
            "success": true,
            "output": {
                "path": "notes/long.md",
                "content": "page body",
                "contentHash": "same-hash-for-every-page",
                "sourceSpan": { "start": next - 12_000, "end": next },
                "nextStartByte": next,
            },
        })
    };
    let first = safe_progress_identities(&page(12_000));
    let second = safe_progress_identities(&page(24_000));
    assert_ne!(first, second, "a new page must not look like no progress");
    assert!(!first.is_empty());
}

/// The integration point, not just the helper: an over-budget `read_note` result
/// must reach the model as valid JSON that still carries its continuation
/// pointer. This is the test that fails when the loop slices the payload.
#[test]
fn over_budget_read_note_result_reaches_the_model_as_valid_json() {
    use crate::ai_runtime::agent_tool_loop::{tool_result_message, LoopProjection};
    use crate::ai_types::{FunctionCall, ToolCall, ToolCallResult};

    let body: String = "中华人民共和国劳动合同法".repeat(1_000);
    let call = ToolCall {
        id: "call-1".into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: "read_note".into(),
            arguments: "{}".into(),
        },
    };
    let result = ToolCallResult {
        tool_name: "read_note".into(),
        success: true,
        output: json!({
            "path": "notes/law.md",
            "content": body,
            "truncated": true,
            "contentHash": "hash-1",
            "sourceSpan": { "start": 4096, "end": 4096 + body.len() },
            "nextStartByte": 4096 + body.len(),
        }),
        duration_ms: 3,
        tokens_used: None,
        error: None,
    };
    let (message, truncated) =
        tool_result_message(&call, &result, LoopProjection::default()).unwrap();
    let content = match &message.content {
        crate::ai_types::MessageContent::Text(text) => text.clone(),
        other => panic!("expected text content, got {other:?}"),
    };
    assert!(truncated, "an over-budget payload must report truncation");
    assert!(
        content.chars().count() <= 8_000,
        "payload still over budget"
    );

    let parsed: serde_json::Value =
        serde_json::from_str(&content).expect("the model must receive valid JSON");
    let output = parsed.get("output").expect("output survives");
    let visible = output
        .get("content")
        .and_then(|value| value.as_str())
        .expect("content survives");
    let start = output
        .get("sourceSpan")
        .and_then(|span| span.get("start"))
        .and_then(serde_json::Value::as_u64)
        .expect("span start");
    let next = output
        .get("nextStartByte")
        .and_then(serde_json::Value::as_u64)
        .expect("continuation pointer survives");
    assert_eq!(
        next,
        start + visible.len() as u64,
        "the continuation pointer must describe what the model can actually see"
    );
}

/// ⑩: an over-budget tool trace is compacted instead of failing the turn, and
/// the newest observation survives.
#[test]
fn an_over_budget_tool_trace_is_compacted_rather_than_failing_the_turn() {
    use crate::ai_runtime::agent_tool_loop::{
        compact_tool_observations, enforce_prompt_budget, AgentModelTurnBudget,
    };
    use crate::ai_types::{LlmMessage, MessageContent, MessageRole};

    let budget = AgentModelTurnBudget {
        max_prompt_tokens: Some(1_000),
        ..Default::default()
    };
    let huge = json!({"success":false,"error":"source_unavailable","output":{"content":"x".repeat(40_000)}}).to_string();
    let tool_message = |id: &str, body: &str| LlmMessage {
        role: MessageRole::Tool,
        content: MessageContent::Text(body.to_string()),
        tool_call_id: Some(id.to_string()),
        tool_calls: None,
        reasoning_content: None,
    };
    let tool_specs: Vec<crate::ai_runtime::ToolSpec> = Vec::new();
    let mut messages = vec![
        LlmMessage {
            role: MessageRole::System,
            content: MessageContent::Text("system".into()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        },
        tool_message("call-old", &huge),
        tool_message("call-new", &huge),
    ];

    // Over budget before compaction.
    assert!(enforce_prompt_budget(&messages, &tool_specs, budget).is_err());
    let before = messages[1].content.text_content().chars().count();

    compact_tool_observations(&mut messages, &tool_specs, budget);

    assert!(
        messages[1].content.text_content().chars().count() < before,
        "the oldest tool observation must be compacted"
    );
    assert!(
        messages[1].content.text_content().contains("compacted"),
        "the marker must survive so the tool call stays answerable"
    );
    assert!(
        messages[1].tool_call_id.as_deref() == Some("call-old"),
        "compaction must not disturb tool_call identity"
    );
    assert_eq!(messages[0].content.text_content(), "system");
}

#[test]
fn plan_a_escaped_read_keeps_valid_json_and_exact_byte_span() {
    use crate::ai_runtime::agent_tool_loop::fit_tool_payload;
    let body = "\"\\\n中🦀".repeat(2_000);
    let payload = json!({"success": true, "error": null, "output": {
        "path": "notes/test.md", "content": body, "contentHash": "revision",
        "sourceSpan": {"start": 7, "end": 7 + body.len()},
        "nextStartByte": null, "truncated": false
    }});
    let (text, _) = fit_tool_payload(&payload, 800, &payload.to_string()).unwrap();
    let fitted: serde_json::Value = serde_json::from_str(&text).expect("valid observation JSON");
    let output = &fitted["output"];
    let visible = output["content"].as_str().expect("visible content");
    assert!(!visible.is_empty());
    assert!(body.starts_with(visible));
    assert!(text.chars().count() <= 800);
    assert_eq!(output["sourceSpan"]["end"], 7 + visible.len());
    assert_eq!(output["nextStartByte"], 7 + visible.len());
}

#[test]
fn plan_a_list_payload_never_becomes_a_json_fragment() {
    use crate::ai_runtime::agent_tool_loop::fit_tool_payload;
    let payload = json!({"success": true, "error": null, "output": {
        "results": (0..20).map(|i| json!({"resourceId": format!("r{i}"), "excerpt": "\"中".repeat(500)})).collect::<Vec<_>>()
    }});
    let (text, _) = fit_tool_payload(&payload, 800, &payload.to_string()).unwrap();
    let fitted: serde_json::Value = serde_json::from_str(&text).expect("valid observation JSON");
    assert_eq!(fitted["success"], true);
    assert!(text.chars().count() <= 800);
    assert!(fitted["output"]["truncated"].as_bool().unwrap_or(false));
}

#[test]
fn plan_a_read_progress_binds_resource_revision_and_range() {
    use crate::ai_runtime::agent_tool_loop::safe_progress_identities;
    let page = |path: &str| {
        json!({
            "path": path, "contentHash": "identical-content", "content": "abc",
            "sourceSpan": {"start": 0, "end": 3}, "nextStartByte": 3
        })
    };
    assert_ne!(
        safe_progress_identities(&page("a.md")),
        safe_progress_identities(&page("b.md"))
    );
    assert_eq!(
        safe_progress_identities(&page("a.md")),
        safe_progress_identities(&page("a.md"))
    );
}

#[test]
fn plan_a_compaction_preserves_failure_and_the_latest_tool_batch() {
    use crate::ai_runtime::agent_tool_loop::{compact_tool_observations, AgentModelTurnBudget};
    use crate::ai_types::{FunctionCall, LlmMessage, MessageRole, ToolCall};
    let calls = |ids: &[&str]| LlmMessage {
        role: MessageRole::Assistant,
        content: "".into(),
        tool_call_id: None,
        tool_calls: Some(
            ids.iter()
                .map(|id| ToolCall {
                    id: id.to_string(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: "read_note".into(),
                        arguments: "{}".into(),
                    },
                })
                .collect(),
        ),
        reasoning_content: None,
    };
    let result = |id: &str, success: bool| {
        LlmMessage {
        role: MessageRole::Tool,
        content: json!({"success": success, "error": if success { None } else { Some("source_unavailable") },
            "output": {"content": "x".repeat(8_000), "resourceId": id}}).to_string().into(),
        tool_call_id: Some(id.into()), tool_calls: None, reasoning_content: None,
    }
    };
    let mut messages = vec![
        calls(&["old"]),
        result("old", false),
        calls(&["new-a", "new-b"]),
        result("new-a", true),
        result("new-b", true),
    ];
    let newest = messages[2..].to_vec();
    compact_tool_observations(
        &mut messages,
        &[],
        AgentModelTurnBudget {
            max_prompt_tokens: Some(500),
            ..Default::default()
        },
    );
    let failed: serde_json::Value =
        serde_json::from_str(&messages[1].content.text_content()).unwrap();
    assert_eq!(
        failed["success"], false,
        "compaction cannot manufacture success"
    );
    assert_eq!(failed["error"], "source_unavailable");
    assert_eq!(failed["output"]["resourceId"], "old");
    assert_eq!(
        serde_json::to_value(&messages[2..]).unwrap(),
        serde_json::to_value(&newest).unwrap()
    );
}
