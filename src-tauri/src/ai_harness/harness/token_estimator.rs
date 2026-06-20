//! Conservative token-count estimation for providers that do not return usage.
//!
//! Only used as fallback when `TokenUsage` fields are all zero (or not present).
//! The estimates are intentionally conservative to prevent budget overshoot.

use crate::ai_runtime::model_gateway::{LlmMessage, TokenUsage};
use crate::ai_types::MessageContent;

/// Source of token count — recorded in traces to make fallback visible.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UsageSource {
    #[default]
    Provider,
    Estimated,
}

/// Estimate token count for a single string.
///
/// Rules (conservative):
/// - CJK character → 0.8 token
/// - ASCII text → chars / 4 (rough 4 chars per token for English prose)
/// - Tool-call JSON → chars / 3 (structural overhead)
pub fn estimate_tokens(text: &str) -> u32 {
    let mut tokens = 0.0_f64;
    for ch in text.chars() {
        if is_cjk(ch) {
            tokens += 0.8;
        } else {
            tokens += 0.25; // 1/4
        }
    }
    tokens.ceil() as u32
}

/// Estimate tokens for a tool-call JSON string (assumes structural overhead).
#[allow(dead_code)]
pub fn estimate_tool_json_tokens(json: &str) -> u32 {
    let chars = json.chars().count() as f64;
    (chars / 3.0).ceil() as u32
}

fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}'   // CJK Unified Ideographs
        | '\u{3400}'..='\u{4DBF}' // CJK Unified Ideographs Extension A
        | '\u{2E80}'..='\u{2FDF}' // CJK Radicals Supplement
        | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
        | '\u{FF00}'..='\u{FFEF}' // Halfwidth and Fullwidth Forms
        | '\u{3040}'..='\u{309F}' // Hiragana
        | '\u{30A0}'..='\u{30FF}' // Katakana
        | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
    )
}

/// Determine whether `usage` is effectively empty (provider returned nothing useful).
pub fn usage_is_empty(usage: &TokenUsage) -> bool {
    usage.prompt_tokens == 0 && usage.completion_tokens == 0 && usage.total_tokens == 0
}

/// Accumulate `delta` into `total` and track whether we are using estimates.
///
/// If `delta` contains useful provider data, add it directly.
/// Otherwise estimate prompt + completion tokens from the messages / content
/// and add the result to `total`.
#[allow(dead_code)]
pub(crate) fn accumulate_usage_with_fallback(
    total: &mut TokenUsage,
    delta: &TokenUsage,
    usage_source: &mut UsageSource,
) {
    if usage_is_empty(delta) {
        *usage_source = UsageSource::Estimated;
    }
    total.prompt_tokens += delta.prompt_tokens;
    total.completion_tokens += delta.completion_tokens;
    total.total_tokens += delta.total_tokens;
    total.prompt_cache_hit_tokens += delta.prompt_cache_hit_tokens;
    total.prompt_cache_miss_tokens += delta.prompt_cache_miss_tokens;
}

/// Estimate prompt + completion tokens from a request–response pair and add
/// them to `total`. Returns the (prompt_est, completion_est) tuple.
pub(crate) fn estimate_and_accumulate(
    total: &mut TokenUsage,
    messages: &[LlmMessage],
    response_content: &str,
) -> (u32, u32) {
    let prompt_est = messages.iter().map(estimate_message_tokens).sum();
    let completion_est = estimate_tokens(response_content);
    total.prompt_tokens += prompt_est;
    total.completion_tokens += completion_est;
    total.total_tokens += prompt_est + completion_est;
    (prompt_est, completion_est)
}

fn estimate_message_tokens(message: &LlmMessage) -> u32 {
    let mut tokens = match &message.content {
        MessageContent::Text(text) => estimate_tokens(text),
        MessageContent::Parts(parts) => {
            estimate_tool_json_tokens(&serde_json::to_string(parts).unwrap_or_default())
        }
    };

    if let Some(tool_call_id) = &message.tool_call_id {
        tokens += estimate_tokens(tool_call_id);
    }
    if let Some(tool_calls) = &message.tool_calls {
        tokens += estimate_tool_json_tokens(&serde_json::to_string(tool_calls).unwrap_or_default());
    }
    if let Some(reasoning) = &message.reasoning_content {
        tokens += estimate_tokens(reasoning);
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cjk_detection() {
        assert!(is_cjk('砚'));
        assert!(is_cjk('人'));
        assert!(!is_cjk('a'));
        assert!(!is_cjk('1'));
        assert!(!is_cjk(' '));
    }

    #[test]
    fn estimate_ascii_roughly_quarter() {
        let tokens = estimate_tokens("hello world test message with 40 chars");
        // 39 chars → ~10 tokens
        assert!((8..=12).contains(&tokens), "got {tokens}");
    }

    #[test]
    fn estimate_cjk_higher() {
        let tokens = estimate_tokens("人工智能笔记系统");
        // 8 chars × 0.8 = 6.4 → 7
        assert!((5..=9).contains(&tokens), "got {tokens}");
    }

    #[test]
    fn usage_is_empty_all_zero() {
        assert!(usage_is_empty(&TokenUsage::default()));
    }

    #[test]
    fn usage_is_empty_nonzero() {
        let u = TokenUsage {
            total_tokens: 100,
            ..Default::default()
        };
        assert!(!usage_is_empty(&u));
    }

    #[test]
    fn accumulate_with_fallback_tracks_source() {
        let mut total = TokenUsage::default();
        let mut source = UsageSource::Provider;
        accumulate_usage_with_fallback(&mut total, &TokenUsage::default(), &mut source);
        assert!(matches!(source, UsageSource::Estimated));
    }

    #[test]
    fn estimate_and_accumulate_adds_to_total() {
        let mut total = TokenUsage::default();
        let msg = LlmMessage {
            role: crate::ai_runtime::model_gateway::MessageRole::User,
            content: "hello world".into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        };
        let (p, c) = estimate_and_accumulate(&mut total, &[msg], "ok response");
        assert!(total.total_tokens > 0);
        assert!(p > 0);
        assert!(c > 0);
    }

    #[test]
    fn estimate_and_accumulate_prompt_scales_with_real_message_text() {
        let short_msg = LlmMessage {
            role: crate::ai_runtime::model_gateway::MessageRole::User,
            content: "short".into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        };
        let long_msg = LlmMessage {
            role: crate::ai_runtime::model_gateway::MessageRole::User,
            content: "长文本".repeat(400).into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        };
        let mut short_total = TokenUsage::default();
        let mut long_total = TokenUsage::default();

        let (short_prompt, _) = estimate_and_accumulate(&mut short_total, &[short_msg], "response");
        let (long_prompt, _) = estimate_and_accumulate(&mut long_total, &[long_msg], "response");

        assert!(
            long_prompt > short_prompt * 50,
            "long prompt estimate {long_prompt} should scale beyond short estimate {short_prompt}",
        );
    }

    #[test]
    fn accumulate_with_fallback_preserves_provider_data() {
        let mut total = TokenUsage::default();
        let mut source = UsageSource::Provider;
        let delta = TokenUsage {
            prompt_tokens: 100,
            completion_tokens: 50,
            total_tokens: 150,
            ..Default::default()
        };
        accumulate_usage_with_fallback(&mut total, &delta, &mut source);
        assert!(matches!(source, UsageSource::Provider));
        assert_eq!(total.total_tokens, 150);
    }

    #[test]
    fn budget_overflow_scenario() {
        // When provider returns 0, estimation kicks in.
        // After estimation total_tokens should be positive,
        // and a budget check against a small budget should detect overflow.
        let mut total = TokenUsage::default();
        let msg = LlmMessage {
            role: crate::ai_runtime::model_gateway::MessageRole::User,
            content: "一段较长的中文内容用于测试 token 预算溢出场景".into(),
            tool_call_id: None,
            tool_calls: None,
            ..Default::default()
        };
        let _ = estimate_and_accumulate(&mut total, &[msg], "这是回复内容");
        let small_budget = 5u32;
        assert!(
            total.total_tokens >= small_budget,
            "estimated {} should exceed tiny budget {small_budget}",
            total.total_tokens,
        );
    }

    #[test]
    fn empty_messages_estimates_only_completion() {
        let mut total = TokenUsage::default();
        let (p, c) = estimate_and_accumulate(&mut total, &[], "ok");
        // Prompt from empty messages should be 0 or near-0; completion > 0
        assert_eq!(p, 0);
        assert!(c > 0);
    }

    #[test]
    fn tool_json_estimates_are_positive() {
        let json = r#"{"query":"test","limit":10}"#;
        let tokens = estimate_tool_json_tokens(json);
        assert!(tokens > 0, "got {tokens}");
    }
}
