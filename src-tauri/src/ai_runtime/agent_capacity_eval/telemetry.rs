//! telemetry — part of the evaluator contract split out of `agent_capacity_eval.rs`.
//!
//! Moved verbatim; the parent re-exports it so existing paths keep working.

use super::*;

#[derive(Debug, Default)]
struct EvaluationTelemetryState {
    model_turns: u32,
    tool_calls: u32,
    web_tool_calls: u32,
    prompt_tokens: u64,
    completion_tokens: u64,
    total_tokens: u64,
    cache_hit_tokens: u64,
    cache_miss_tokens: u64,
    first_visible_token_ms: Option<u64>,
    total_model_time_ms: u64,
    finish_stop: u32,
    finish_tool_calls: u32,
    finish_length: u32,
    finish_other: u32,
    truncation_none: u32,
    truncation_tool_result: u32,
    truncation_final_output: u32,
    budget_within: u32,
    budget_model_turns: u32,
    budget_tool_calls: u32,
    budget_output: u32,
    final_output_recorded: bool,
}

/// Cloneable, evaluation-only in-memory tap. It owns no database handle and
/// exposes no raw provider, prompt, answer, token, tool-argument, or path data.
#[derive(Debug, Clone)]
pub(crate) struct EvaluationTelemetryTap {
    state: std::sync::Arc<std::sync::Mutex<EvaluationTelemetryState>>,
    started_at: std::sync::Arc<std::time::Instant>,
}

impl Default for EvaluationTelemetryTap {
    fn default() -> Self {
        Self {
            state: std::sync::Arc::new(std::sync::Mutex::new(EvaluationTelemetryState::default())),
            started_at: std::sync::Arc::new(std::time::Instant::now()),
        }
    }
}

impl EvaluationTelemetryTap {
    pub(crate) fn record_model_turn_at(
        &self,
        response: &crate::ai_runtime::model_gateway::GatewayResponse,
        elapsed_ms: u64,
    ) {
        if let Ok(mut state) = self.state.lock() {
            state.model_turns = state.model_turns.saturating_add(1);
            state.prompt_tokens = state
                .prompt_tokens
                .saturating_add(u64::from(response.usage.prompt_tokens));
            state.completion_tokens = state
                .completion_tokens
                .saturating_add(u64::from(response.usage.completion_tokens));
            state.total_tokens = state
                .total_tokens
                .saturating_add(u64::from(response.usage.total_tokens));
            state.cache_hit_tokens = state
                .cache_hit_tokens
                .saturating_add(u64::from(response.usage.prompt_cache_hit_tokens));
            state.cache_miss_tokens = state
                .cache_miss_tokens
                .saturating_add(u64::from(response.usage.prompt_cache_miss_tokens));
            state.total_model_time_ms = state.total_model_time_ms.saturating_add(elapsed_ms);
            match classify_finish_reason(&response.finish_reason) {
                FinishReasonClass::Stop => state.finish_stop = state.finish_stop.saturating_add(1),
                FinishReasonClass::ToolCalls => {
                    state.finish_tool_calls = state.finish_tool_calls.saturating_add(1);
                }
                FinishReasonClass::Length => {
                    state.finish_length = state.finish_length.saturating_add(1);
                }
                FinishReasonClass::Other => {
                    state.finish_other = state.finish_other.saturating_add(1);
                }
            }
        }
    }

    /// Record one Host-dispatched business tool action. Proposed, deferred, or
    /// rejected model calls are intentionally excluded: Campaign budgets meter
    /// real external work, not an untrusted response shape.
    pub(crate) fn record_executed_tool_call(&self, tool_name: &str) {
        if let Ok(mut state) = self.state.lock() {
            state.tool_calls = state.tool_calls.saturating_add(1);
            if matches!(
                tool_name,
                "web_search" | "web_fetch" | "web.search" | "web.fetch"
            ) {
                state.web_tool_calls = state.web_tool_calls.saturating_add(1);
            }
        }
    }

    /// Record the fixed WebRequired Host bootstrap, whose actions are not
    /// model-proposed tool calls but are still real Web logical actions.
    pub(crate) fn record_bootstrap_web_tool_calls(&self, count: u32) {
        if let Ok(mut state) = self.state.lock() {
            state.tool_calls = state.tool_calls.saturating_add(count);
            state.web_tool_calls = state.web_tool_calls.saturating_add(count);
        }
    }

    pub(crate) fn record_model_turn(
        &self,
        response: &crate::ai_runtime::model_gateway::GatewayResponse,
        started_at: std::time::Instant,
    ) {
        self.record_model_turn_at(
            response,
            started_at.elapsed().as_millis().min(u128::from(u64::MAX)) as u64,
        );
    }

    pub(crate) fn record_stream_event_at(
        &self,
        event: &crate::ai_runtime::model_gateway::StreamEvent,
        elapsed_ms: u64,
    ) {
        if !matches!(
            event.surface,
            crate::ai_runtime::model_gateway::StreamSurface::VisibleAnswer
                | crate::ai_runtime::model_gateway::StreamSurface::VisibleAnswerSanitized
        ) || !matches!(
            event.data,
            crate::ai_runtime::model_gateway::StreamEventData::Token { .. }
        ) {
            return;
        }
        if let Ok(mut state) = self.state.lock() {
            state.first_visible_token_ms = Some(
                state
                    .first_visible_token_ms
                    .map_or(elapsed_ms, |current| current.min(elapsed_ms)),
            );
        }
    }

    pub(crate) fn record_stream_event(
        &self,
        event: &crate::ai_runtime::model_gateway::StreamEvent,
    ) {
        self.record_stream_event_at(
            event,
            self.started_at
                .elapsed()
                .as_millis()
                .min(u128::from(u64::MAX)) as u64,
        );
    }

    pub(crate) fn record_truncation(&self, outcome: TruncationOutcome) {
        if let Ok(mut state) = self.state.lock() {
            match outcome {
                TruncationOutcome::ToolResultTruncated => {
                    state.truncation_tool_result = state.truncation_tool_result.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn record_budget(&self, outcome: BudgetOutcome) {
        if let Ok(mut state) = self.state.lock() {
            match outcome {
                BudgetOutcome::ModelTurnsExhausted => {
                    state.budget_model_turns = state.budget_model_turns.saturating_add(1);
                }
                BudgetOutcome::ToolCallsExhausted => {
                    state.budget_tool_calls = state.budget_tool_calls.saturating_add(1);
                }
                BudgetOutcome::OutputBudgetReached => {
                    state.budget_output = state.budget_output.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn record_final_output_validation(
        &self,
        accepted: bool,
        output_budget_reached: bool,
    ) {
        if let Ok(mut state) = self.state.lock() {
            if state.final_output_recorded {
                return;
            }
            state.final_output_recorded = true;
            if accepted {
                state.truncation_none = state.truncation_none.saturating_add(1);
                state.budget_within = state.budget_within.saturating_add(1);
            } else {
                state.truncation_final_output = state.truncation_final_output.saturating_add(1);
                if output_budget_reached {
                    state.budget_output = state.budget_output.saturating_add(1);
                } else {
                    state.budget_within = state.budget_within.saturating_add(1);
                }
            }
        }
    }

    pub(crate) fn snapshot(&self) -> EvaluationTelemetrySummary {
        let state = self.state.lock().unwrap_or_else(|error| error.into_inner());
        EvaluationTelemetrySummary {
            model_turns: state.model_turns,
            tool_calls: state.tool_calls,
            web_tool_calls: state.web_tool_calls,
            prompt_tokens: state.prompt_tokens,
            completion_tokens: state.completion_tokens,
            total_tokens: state.total_tokens,
            cache_hit_tokens: state.cache_hit_tokens,
            cache_miss_tokens: state.cache_miss_tokens,
            first_visible_token_ms: state.first_visible_token_ms,
            total_model_time_ms: state.total_model_time_ms,
            finish_reasons: FinishReasonCounts {
                stop: state.finish_stop,
                tool_calls: state.finish_tool_calls,
                length: state.finish_length,
                other: state.finish_other,
            },
            truncations: TruncationCounts {
                none: state.truncation_none,
                tool_result: state.truncation_tool_result,
                final_output: state.truncation_final_output,
            },
            budgets: BudgetCounts {
                within: state.budget_within,
                model_turns: state.budget_model_turns,
                tool_calls: state.budget_tool_calls,
                output: state.budget_output,
            },
        }
    }
}

fn classify_finish_reason(value: &str) -> FinishReasonClass {
    match value.trim().to_ascii_lowercase().as_str() {
        "stop" | "end_turn" | "completed" => FinishReasonClass::Stop,
        "tool_calls" | "tool_use" => FinishReasonClass::ToolCalls,
        "length" | "max_tokens" => FinishReasonClass::Length,
        _ => FinishReasonClass::Other,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct FinishReasonCounts {
    pub(super) stop: u32,
    pub(super) tool_calls: u32,
    pub(super) length: u32,
    pub(super) other: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct TruncationCounts {
    pub(super) none: u32,
    pub(super) tool_result: u32,
    pub(super) final_output: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct BudgetCounts {
    pub(super) within: u32,
    pub(super) model_turns: u32,
    pub(super) tool_calls: u32,
    pub(super) output: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct EvaluationTelemetrySummary {
    pub(super) model_turns: u32,
    pub(super) tool_calls: u32,
    pub(super) web_tool_calls: u32,
    pub(super) prompt_tokens: u64,
    pub(super) completion_tokens: u64,
    pub(super) total_tokens: u64,
    pub(super) cache_hit_tokens: u64,
    pub(super) cache_miss_tokens: u64,
    pub(super) first_visible_token_ms: Option<u64>,
    pub(super) total_model_time_ms: u64,
    pub(super) finish_reasons: FinishReasonCounts,
    pub(super) truncations: TruncationCounts,
    pub(super) budgets: BudgetCounts,
}

impl EvaluationTelemetrySummary {
    pub(crate) const fn model_turns(&self) -> u32 {
        self.model_turns
    }

    pub(crate) const fn tool_calls(&self) -> u32 {
        self.tool_calls
    }

    pub(crate) const fn web_tool_calls(&self) -> u32 {
        self.web_tool_calls
    }

    pub(crate) const fn total_tokens(&self) -> u64 {
        self.total_tokens
    }

    pub(crate) const fn first_visible_token_ms(&self) -> Option<u64> {
        self.first_visible_token_ms
    }

    pub(crate) const fn total_model_time_ms(&self) -> u64 {
        self.total_model_time_ms
    }

    pub(crate) const fn tool_result_truncations(&self) -> u32 {
        self.truncations.tool_result
    }

    pub(crate) const fn final_output_successes(&self) -> u32 {
        self.truncations.none
    }

    pub(crate) const fn final_output_rejections(&self) -> u32 {
        self.truncations.final_output
    }

    pub(crate) const fn output_budget_reached(&self) -> u32 {
        self.budgets.output
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvalRunMode {
    Smoke,
    Full,
}

/// Strength of the evidence behind one result file. The headless harness
/// validates Iris orchestration with deterministic external peers; it does not
/// claim live model or vendor capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum EvaluationEvidenceLevel {
    HeadlessDeterministic,
}
