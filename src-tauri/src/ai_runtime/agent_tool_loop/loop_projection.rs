//! What the model is allowed to know about the loop's own execution state.
//!
//! The Host keeps the exact accounting: model turns, per-category tool calls,
//! remaining deadline and Provider attempt counts. The model gets only what it
//! needs to choose its next action: whether it may continue, whether it must
//! synthesize, what failed, and which action is still available.
//!
//! Publishing exact remaining quotas made the model narrate internal budgets to
//! the user and treat a numeric allowance as a target. Both projections are
//! derived from the same frozen Host counters, so they cannot disagree.

/// The model-visible half of one loop state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LoopProjection {
    /// Whether at least one more exploratory tool action remains affordable.
    pub(crate) can_continue: bool,
    /// Whether the Host has closed the business surface and this turn must
    /// synthesize from what is already observed.
    pub(crate) must_synthesize: bool,
    /// Stable bounded failure kind for the observation being reported, when the
    /// result itself failed. It is a Host vocabulary value, never Provider text.
    pub(crate) failure_type: Option<&'static str>,
    /// The one action the model should take next.
    pub(crate) next_action: &'static str,
    /// Whether this observation is a replay of an earlier bounded result rather
    /// than a fresh read.
    pub(crate) historical_observation: bool,
}

impl Default for LoopProjection {
    fn default() -> Self {
        Self {
            can_continue: true,
            must_synthesize: false,
            failure_type: None,
            next_action: NEXT_ACTION_CONTINUE,
            historical_observation: false,
        }
    }
}

impl LoopProjection {
    /// Build the projection published in one tool result observation.
    ///
    /// The Host does not force synthesis from inside a dispatch round: the
    /// business surface is closed at the *start* of the next model turn, so a
    /// round that just produced observations leaves the surface open and asks
    /// the model to continue. `next_action` therefore follows `can_continue`
    /// here, and the closed-surface case reaches the model through the turn's
    /// own instruction and `widest()` envelope.
    pub(crate) fn for_observation(
        can_continue: bool,
        failure_type: Option<&'static str>,
        historical_observation: bool,
    ) -> Self {
        Self {
            can_continue,
            must_synthesize: false,
            failure_type,
            next_action: if can_continue {
                NEXT_ACTION_CONTINUE
            } else {
                NEXT_ACTION_SYNTHESIZE
            },
            historical_observation,
        }
    }

    /// Attach the bounded failure kind of the observation being reported.
    pub(crate) fn with_failure_type(mut self, failure_type: Option<&'static str>) -> Self {
        self.failure_type = failure_type;
        self
    }

    /// The widest legal shape of this projection.
    ///
    /// Executors size a payload before the loop knows the final counters, so
    /// they must reserve at least this much envelope. A projection that is never
    /// wider than `widest()` therefore cannot re-truncate content the executor
    /// already registered.
    pub(crate) fn widest() -> Self {
        Self {
            can_continue: true,
            must_synthesize: true,
            failure_type: Some("tool_call_budget_exhausted"),
            next_action: NEXT_ACTION_SYNTHESIZE,
            historical_observation: true,
        }
    }

    /// Serialize the model-visible projection as one JSON object.
    pub(crate) fn to_json(self) -> serde_json::Value {
        serde_json::json!({
            "canContinue": self.can_continue,
            "mustSynthesize": self.must_synthesize,
            "failureType": self.failure_type,
            "nextAction": self.next_action,
            "historicalObservation": self.historical_observation,
        })
    }
}

pub(crate) const NEXT_ACTION_CONTINUE: &str = "continue_with_available_tools";
pub(crate) const NEXT_ACTION_SYNTHESIZE: &str = "synthesize_from_current_observations";

/// Stable bounded failure kind for one tool result. Only closed Host vocabulary
/// values are returned; an unmapped failure keeps the generic kind so Provider
/// or resource text can never become the model-visible classification.
pub(crate) fn observation_failure_type(
    result: &crate::ai_runtime::ToolCallResult,
) -> Option<&'static str> {
    if result.success {
        return None;
    }
    Some(match result.error.as_deref().unwrap_or_default() {
        "web_provider_timeout" | "agent_run_web_provider_timeout" => "timeout",
        "web_provider_unavailable" | "agent_run_mcp_unavailable" => "provider_unavailable",
        "web_provider_auth_failed" | "agent_run_web_provider_auth_failed" => "authorization_failed",
        "web_evidence_invalid" | "agent_run_web_evidence_invalid" => "no_verifiable_body",
        "web_read_unavailable" => "read_unavailable",
        "tool_call_budget_exhausted" | "tool_category_budget_exhausted" => "budget_exhausted",
        "tool_call_already_succeeded" => "already_succeeded",
        "tool_call_repeated" => "repeated_call",
        "tool_arguments_invalid" | "arguments_schema_mismatch" => "arguments_invalid",
        "tool_not_in_run_surface" => "not_in_run_surface",
        "deferred_for_feedback" => "deferred_for_feedback",
        _ => "failed",
    })
}
