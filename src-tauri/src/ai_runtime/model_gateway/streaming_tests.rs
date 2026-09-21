use super::*;

#[derive(Default)]
struct RecordingStreamObserver {
    events: Vec<StreamEvent>,
}

impl StreamEventObserver for RecordingStreamObserver {
    fn observe(&mut self, event: &StreamEvent, _token_index: u32) -> AppResult<()> {
        self.events.push(event.clone());
        Ok(())
    }
}

#[test]
fn stream_events_are_delivered_to_observer_without_tauri_handle() {
    let mut observer = RecordingStreamObserver::default();
    let mut token_index = 0;

    emit_visible_token_delta(
        &mut observer,
        "agent-run",
        "观察者令牌".to_string(),
        StreamSurface::VisibleAnswer,
        false,
        &mut token_index,
    )
    .unwrap();

    assert_eq!(token_index, 1);
    assert_eq!(observer.events.len(), 1);
    assert!(matches!(
        observer.events[0].data,
        StreamEventData::Token { ref token, .. } if token == "观察者令牌"
    ));
}

#[test]
fn sse_json_failure_tracker_tolerates_single_bad_line_but_fails_after_threshold() {
    let mut tracker = SseJsonFailureTracker::default();
    assert!(tracker.record_parse_result("req-json", "{bad json").is_ok());
    assert_eq!(tracker.consecutive_failures, 1);
    assert!(tracker
        .record_parse_result("req-json", "{still bad")
        .is_ok());
    let err = tracker
        .record_parse_result("req-json", "{third bad")
        .unwrap_err();
    assert!(err.to_string().contains("stream_invalid_json"));
}

#[test]
fn sse_json_failure_tracker_resets_after_valid_json() {
    let mut tracker = SseJsonFailureTracker::default();
    assert!(tracker.record_parse_result("req-json", "{bad json").is_ok());
    tracker.record_success();
    assert_eq!(tracker.consecutive_failures, 0);
    assert!(tracker
        .record_parse_result("req-json", "{bad again")
        .is_ok());
}
#[test]
fn sse_json_failure_tracker_ignores_empty_data_lines() {
    let mut tracker = SseJsonFailureTracker::default();
    assert!(tracker.parse_data("req-json", "   ").unwrap().is_none());
    assert_eq!(tracker.consecutive_failures, 0);
}
#[test]
fn stream_error_event_uses_public_lifecycle_contract() {
    let event = stream_error_event(
        "req-stream-error",
        "模型请求失败（500）：provider echoed prompt text",
        false,
        StreamSurface::VisibleAnswer,
        true,
    );

    assert_eq!(event.request_id, "req-stream-error");
    assert!(matches!(event.event_type, StreamEventType::Error));
    match event.data {
        StreamEventData::Error {
            message,
            final_error,
        } => {
            assert!(message.contains("模型请求失败"));
            assert!(!message.contains("prompt text"));
            assert!(final_error);
        }
        _ => panic!("expected error payload"),
    }
}

#[test]
fn visible_partial_stream_errors_force_terminal_event() {
    assert!(!should_emit_stream_error(
        false,
        StreamSurface::VisibleAnswer,
        0
    ));
    assert!(should_emit_stream_error(
        false,
        StreamSurface::VisibleAnswer,
        1
    ));
    assert!(!should_emit_stream_error(
        false,
        StreamSurface::InternalCandidate,
        1
    ));
    assert!(should_emit_stream_error(
        true,
        StreamSurface::VisibleAnswer,
        0
    ));
}

#[test]
fn stream_body_failure_diagnostic_is_structured_and_redacted() {
    let diagnostic = StreamReadFailureDiagnostic {
        provider_id: "deepseek".into(),
        model: "deepseek-v4-pro".into(),
        endpoint_family: EndpointFamily::OpenAiCompatibleChatCompletions,
        http_version: "HTTP/2.0".into(),
        status: 200,
        content_encoding: Some("identity".into()),
        transfer_encoding: None,
        elapsed_ms: 12_345,
        chunk_count: 7,
        byte_count: 4096,
        sse_line_count: 12,
        saw_done: false,
        visible_partial: true,
        error: StreamReadErrorDiagnostic {
            is_timeout: false,
            is_body: true,
            is_connect: false,
            is_decode: true,
            source_chain: vec![
                "body error".into(),
                "secret sk-test-123456789012 request body should not appear".into(),
            ],
        },
    };

    let value = diagnostic.to_safe_json();
    let rendered = value.to_string();

    assert_eq!(value["provider"], "deepseek");
    assert_eq!(value["model"], "deepseek-v4-pro");
    assert_eq!(
        value["endpoint_family"],
        "openai_compatible_chat_completions"
    );
    assert_eq!(value["http_version"], "HTTP/2.0");
    assert_eq!(value["status"], 200);
    assert_eq!(value["content_encoding"], "identity");
    assert_eq!(value["chunk_count"], 7);
    assert_eq!(value["byte_count"], 4096);
    assert_eq!(value["sse_line_count"], 12);
    assert_eq!(value["saw_done"], false);
    assert_eq!(value["visible_partial"], true);
    assert_eq!(value["error"]["is_body"], true);
    assert_eq!(value["error"]["is_decode"], true);
    assert_eq!(value["error"]["source_chain"][0]["detail"], "body error");
    assert!(rendered.contains("stream_body_read_failed"));
    assert!(!rendered.contains("sk-test"));
    assert!(!rendered.contains("request body"));
}

#[test]
fn visible_stream_sanitizer_holds_split_think_tag_until_safe_text() {
    let mut sanitizer = VisibleStreamSanitizer::new();

    assert_eq!(
        sanitizer.sanitize_delta("答复<thi", false).as_test_delta(),
        "答复"
    );
    assert_eq!(
        sanitizer.sanitize_delta("nk>hidden", false).as_test_delta(),
        ""
    );
    assert_eq!(
        sanitizer
            .sanitize_delta("</think>正文开始", false)
            .as_test_delta(),
        "正文开始"
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn minimax_stream_sanitizer_withholds_split_control_tokens() {
    let mut sanitizer = VisibleStreamSanitizer::for_provider("minimax");

    assert_eq!(
        sanitizer.sanitize_delta("<|mini", false).as_test_delta(),
        ""
    );
    assert_eq!(
        sanitizer
            .sanitize_delta("max|><think>private</think>Visible", false)
            .as_test_delta(),
        "Visible"
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn minimax_stream_sanitizer_discards_partial_control_token_at_end_of_stream() {
    let mut sanitizer = VisibleStreamSanitizer::for_provider("minimax");

    assert_eq!(
        sanitizer
            .sanitize_delta("Visible<|mini", false)
            .as_test_delta(),
        "Visible"
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn minimax_stream_reasoning_details_are_kept_only_for_the_tool_continuation() {
    let mut details = Vec::new();
    append_minimax_reasoning_details(
        &serde_json::json!([{"type":"reasoning.text","text":"private"}]),
        &mut details,
    );

    assert_eq!(
        minimax_reasoning_continuation(details, String::new()).as_deref(),
        Some(r#"[{"text":"private","type":"reasoning.text"}]"#)
    );
}

#[test]
fn minimax_stream_reasoning_details_merge_cumulative_snapshots() {
    let mut details = Vec::new();
    append_minimax_reasoning_details(
        &serde_json::json!([{
            "index": 0,
            "type": "reasoning.text",
            "text": "private"
        }]),
        &mut details,
    );
    append_minimax_reasoning_details(
        &serde_json::json!([{
            "index": 0,
            "type": "reasoning.text",
            "text": "private plan"
        }]),
        &mut details,
    );

    assert_eq!(
        minimax_reasoning_continuation(details, String::new()).as_deref(),
        Some(r#"[{"index":0,"text":"private plan","type":"reasoning.text"}]"#)
    );
}

#[test]
fn minimax_stream_reasoning_details_merge_incremental_fragments() {
    let mut details = Vec::new();
    append_minimax_reasoning_details(
        &serde_json::json!({
            "id": "reasoning-1",
            "type": "reasoning.text",
            "text": "private "
        }),
        &mut details,
    );
    append_minimax_reasoning_details(
        &serde_json::json!({
            "id": "reasoning-1",
            "type": "reasoning.text",
            "text": "plan"
        }),
        &mut details,
    );

    assert_eq!(
        minimax_reasoning_continuation(details, String::new()).as_deref(),
        Some(r#"[{"id":"reasoning-1","text":"private plan","type":"reasoning.text"}]"#)
    );
}

#[test]
fn minimax_stream_reasoning_details_preserve_distinct_same_type_items() {
    let mut details = Vec::new();
    append_minimax_reasoning_details(
        &serde_json::json!([
            {"type": "reasoning.text", "text": "first"},
            {"type": "reasoning.text", "text": "second"}
        ]),
        &mut details,
    );

    assert_eq!(
        minimax_reasoning_continuation(details, String::new()).as_deref(),
        Some(
            r#"[{"text":"first","type":"reasoning.text"},{"text":"second","type":"reasoning.text"}]"#
        )
    );
}

#[test]
fn visible_stream_sanitizer_suppresses_unclosed_reasoning_tail() {
    let mut sanitizer = VisibleStreamSanitizer::new();

    assert_eq!(
        sanitizer
            .sanitize_delta("可以先看结论。", false)
            .as_test_delta(),
        "可以先看结论。"
    );
    assert_eq!(
        sanitizer
            .sanitize_delta("<reasoning>internal", false)
            .as_test_delta(),
        ""
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn visible_stream_sanitizer_never_releases_a_long_meta_analysis_prefix() {
    let mut sanitizer = VisibleStreamSanitizer::new();
    let first_meta_paragraph = format!(
        "The user is asking for current sports information. {}",
        "I should inspect the system instructions before answering. ".repeat(12)
    );
    assert!(first_meta_paragraph.chars().count() > 500);

    assert_eq!(
        sanitizer
            .sanitize_delta(&first_meta_paragraph, false)
            .as_test_delta(),
        ""
    );
    assert_eq!(
        sanitizer
            .sanitize_delta(
                "\n\nThe system prompt requires verified evidence before a final response.",
                false,
            )
            .as_test_delta(),
        ""
    );
    assert_eq!(
        sanitizer
            .sanitize_delta("\n\n这是基于联网证据的最终答复。", false)
            .as_test_delta(),
        "这是基于联网证据的最终答复。"
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn visible_stream_sanitizer_withholds_a_short_capability_routing_opener() {
    let mut sanitizer = VisibleStreamSanitizer::new();

    assert_eq!(
        sanitizer
            .sanitize_delta("这是一个不需要联网的主观分析问题。直接给你拆解：", false)
            .as_test_delta(),
        ""
    );
    assert_eq!(
        sanitizer
            .sanitize_delta("\n\n一、核心结论：调侃 ≠ 真的关系差", false)
            .as_test_delta(),
        "一、核心结论：调侃 ≠ 真的关系差"
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn visible_stream_sanitizer_preserves_normal_answers_with_common_openers() {
    let mut sanitizer = VisibleStreamSanitizer::new();

    assert_eq!(
        sanitizer
            .sanitize_delta(
                "Given sufficient context, the answer can be concise.",
                false
            )
            .as_test_delta(),
        "Given sufficient context, the answer can be concise."
    );
    assert_eq!(sanitizer.finish().as_test_delta(), "");
}

#[test]
fn sanitized_surface_is_visible_to_the_frontend() {
    assert!(StreamSurface::VisibleAnswerSanitized.is_visible());
}

#[test]
fn sse_rejects_illegal_utf8_instead_of_replacing_it() {
    let mut pending = Vec::new();
    decode_sse_utf8(&mut pending, &[0xff]).expect_err("illegal utf-8");
    pending.clear();
    let first = decode_sse_utf8(&mut pending, &[0xe4]).expect("incomplete prefix");
    assert!(first.is_empty());
    let rest = decode_sse_utf8(&mut pending, &[0xb8, 0xad]).expect("complete 中");
    assert_eq!(rest, "中");
    finish_sse_utf8(&pending).expect("complete sequence");
    decode_sse_utf8(&mut pending, &[0xe4]).expect("incomplete prefix");
    finish_sse_utf8(&pending).expect_err("truncated sequence at eof");
}
