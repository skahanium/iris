//! Payload contracts for the Run tool loop.
//!
//! Split out of `run_tool_loop.rs` when these tests pushed that file past the
//! repository file-size budget (`npm run size:check`).

use crate::ai_runtime::run_tool_loop::{apply_excerpt_window, MAX_WEB_EXCERPT_CHARS};
use serde_json::json;

/// ⑧: a long page is read in windows and each window names the next one.
#[test]
fn excerpt_window_pages_through_a_long_page() {
    let item = |body: String| {
        let mut value = json!({
            "url": "https://example.test/long",
            "canonical_url": "https://example.test/long",
            "title": "long page",
            "domain": "example.test",
            "snippet": "snippet",
            "provider_id": "provider",
            "provider_kind": "native",
            "cost_class": "network",
            "raw_result_hash": "hash",
            "extraction_method": "native",
            "trust_level": "external_web",
            "retrieval_reason": "web_evidence_broker",
            "search_backend": "provider",
            "source_rank": "unknown",
        });
        value["fetched_excerpt"] = serde_json::Value::String(body);
        serde_json::from_value::<crate::ai_runtime::web_evidence_broker::WebEvidenceItem>(value)
            .expect("item")
    };
    // Each call fetches a fresh body and asks for a later window.
    let window_for = |start_char: usize| {
        let mut items = vec![item("b".repeat(5_000))];
        let window = apply_excerpt_window(&mut items, start_char);
        let remaining = items[0]
            .fetched_excerpt
            .as_deref()
            .map(|body| body.chars().count())
            .expect("excerpt");
        (window, remaining)
    };

    let (short, _) = {
        let mut items = vec![item("x".repeat(500))];
        (apply_excerpt_window(&mut items, 0), ())
    };
    assert_eq!(short["truncated"], json!(false));
    assert_eq!(short["nextStartChar"], serde_json::Value::Null);

    let (first, _) = window_for(0);
    assert_eq!(first["truncated"], json!(true));
    assert_eq!(
        first["nextStartChar"].as_u64(),
        Some(MAX_WEB_EXCERPT_CHARS as u64)
    );

    let (second, remaining) = window_for(MAX_WEB_EXCERPT_CHARS);
    assert_eq!(remaining, 3_000);
    assert_eq!(second["truncated"], json!(true));
    let (tail, remaining_tail) = window_for(4_000);
    assert_eq!(remaining_tail, 1_000);
    assert_eq!(tail["truncated"], json!(false));
}
