//! Payload-shaping contracts for the Agent tool loop.
//!
//! Split out of `agent_tool_loop_tests.rs` when those payload tests pushed that
//! file past the repository's file-size budget (`npm run size:check`).

use serde_json::json;

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

    let (fitted, truncated) = fit_tool_payload(&payload, 8_000, &serialized);
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
    let (message, truncated) = tool_result_message(&call, &result, 3, 5, 2);
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
