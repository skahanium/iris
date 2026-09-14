//! Reading progress boundary regressions. Executor tests cover actual paging.
use crate::ai_runtime::agent_tool_loop::safe_progress_identities;
use serde_json::json;

#[test]
fn empty_web_window_does_not_create_reading_progress() {
    let value = json!({"canonicalUrl":"https://example.com/a","contentHash":"hash",
        "excerptWindow":{"startChar":5000,"endChar":5000,"nextStartChar":null}});
    assert!(safe_progress_identities(&value).is_empty());
}
