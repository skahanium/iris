//! Payload-shaping contracts for the Agent tool loop.
//!
//! Split out of `agent_tool_loop_tests.rs` when those payload tests pushed that
//! file past the repository's file-size budget (`npm run size:check`).

use serde_json::json;

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
    use crate::ai_runtime::agent_tool_loop::{prepare_tool_result, tool_result_message};
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
        7,
        23,
        5,
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

/// Cross-layer invariant: every bounded limitation the Host can publish must be
/// recognised by the finalisation layer. This defect class (a producer and its
/// recogniser drifting apart) is what left the "already holds evidence" variant
/// failing as if it were ordinary model output.
#[test]
fn every_host_authored_bounded_limitation_is_recognised() {
    use crate::ai_runtime::agent_tool_loop::{
        is_evidence_limited_response, EVIDENCE_LIMITED_RESPONSE,
        EVIDENCE_LIMITED_RESPONSE_PREFIXES, EVIDENCE_LIMITED_WITH_EVIDENCE_PREFIX,
    };

    assert!(EVIDENCE_LIMITED_RESPONSE_PREFIXES.len() >= 2);
    for prefix in EVIDENCE_LIMITED_RESPONSE_PREFIXES {
        let published = format!("{prefix}，后续说明。");
        assert!(
            is_evidence_limited_response(&published),
            "Host-authored limitation is not recognised: {published}"
        );
    }
    assert!(is_evidence_limited_response(EVIDENCE_LIMITED_RESPONSE));
    assert!(is_evidence_limited_response(&format!(
        "{EVIDENCE_LIMITED_WITH_EVIDENCE_PREFIX}，但未能完成最终来源关联；已取得的正文不会被当作读取失败。请重试以完成答复。"
    )));
    // Ordinary prose is still not treated as a Host limitation.
    assert!(!is_evidence_limited_response(
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
        "loopObservation": { "remainingModelTurns": 3, "remainingToolCalls": 5, "remainingCategoryCalls": 2 },
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
    use crate::ai_runtime::agent_tool_loop::tool_result_message;
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
    let (message, truncated) = tool_result_message(&call, &result, 3, 5, 2).unwrap();
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
