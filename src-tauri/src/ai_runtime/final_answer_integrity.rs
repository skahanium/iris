//! Shared structural admission rules for model final answers.
//!
//! This gate intentionally checks only protocol-complete termination and the
//! minimal shape of visible prose. Evidence ownership remains the separate
//! provenance gate, while style and factual quality remain model-evaluation
//! concerns.

/// One canonical integrity gate shared by tool-loop recovery and finalization.
pub(crate) struct FinalAnswerIntegrity;

impl FinalAnswerIntegrity {
    /// Whether a provider declared a normal, completed final response.
    pub(crate) fn has_normal_finish_reason(value: &str) -> bool {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "stop" | "end_turn" | "completed"
        )
    }

    /// Whether assembled tool calls may be dispatched.
    ///
    /// `tool_calls` / `tool_use` are the protocol terminals for a tool turn.
    /// A normal finish (`stop` / `end_turn` / `completed`) may still carry
    /// MiniMax content-embedded tools. Truncation (`length`, `unknown`, …)
    /// must not execute those calls as complete actions.
    pub(crate) fn may_execute_tool_calls(finish_reason: &str) -> bool {
        matches!(
            finish_reason.trim().to_ascii_lowercase().as_str(),
            "tool_calls" | "tool_use"
        ) || Self::has_normal_finish_reason(finish_reason)
    }

    /// Whether visible text has enough structure for a route that requires a
    /// factual answer. Creative, rewrite, code and simple conversation routes
    /// deliberately do not impose this extra shape rule.
    pub(crate) fn has_complete_visible_answer(
        content: &str,
        requires_factual_completion: bool,
    ) -> bool {
        !requires_factual_completion
            || !crate::ai_runtime::text_support::is_title_only_visible_answer(content)
    }

    /// Whether the model should receive its single append-only recovery turn.
    pub(crate) fn needs_recovery(
        content: &str,
        finish_reason: &str,
        requires_factual_completion: bool,
    ) -> bool {
        !Self::has_normal_finish_reason(finish_reason)
            || !Self::has_complete_visible_answer(content, requires_factual_completion)
    }
}

#[cfg(test)]
mod tests {
    use super::FinalAnswerIntegrity;

    #[test]
    fn accepts_only_normal_finish_reasons_and_non_title_answers() {
        assert!(FinalAnswerIntegrity::has_normal_finish_reason("stop"));
        assert!(FinalAnswerIntegrity::has_normal_finish_reason("END_TURN"));
        assert!(!FinalAnswerIntegrity::has_normal_finish_reason(
            "max_tokens"
        ));
        assert!(!FinalAnswerIntegrity::needs_recovery(
            "完整回答。",
            "stop",
            true,
        ));
        assert!(FinalAnswerIntegrity::needs_recovery(
            "特朗普 最新新闻 2026年8月",
            "stop",
            true,
        ));
        assert!(!FinalAnswerIntegrity::needs_recovery(
            "简短问候",
            "stop",
            false
        ));
        assert!(!FinalAnswerIntegrity::has_normal_finish_reason("length"));
        assert!(!FinalAnswerIntegrity::has_normal_finish_reason("unknown"));
        assert!(FinalAnswerIntegrity::needs_recovery(
            "截断正文",
            "length",
            false
        ));
        assert!(FinalAnswerIntegrity::may_execute_tool_calls("tool_calls"));
        assert!(FinalAnswerIntegrity::may_execute_tool_calls("tool_use"));
        assert!(FinalAnswerIntegrity::may_execute_tool_calls("stop"));
        assert!(!FinalAnswerIntegrity::may_execute_tool_calls("length"));
        assert!(!FinalAnswerIntegrity::may_execute_tool_calls("unknown"));
        assert!(!FinalAnswerIntegrity::may_execute_tool_calls("max_tokens"));
    }
}
