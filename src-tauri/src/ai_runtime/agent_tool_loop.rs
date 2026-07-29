//! Bounded, provider-neutral model/tool orchestration for one Agent Run.
//!
//! This module owns transcript integrity and loop limits only. Permission checks,
//! confirmation persistence, audit writes and the concrete tool dispatch remain in
//! the Run-bound executor supplied by the caller.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use crate::ai_runtime::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::run_contract::RunBudgetPolicy;
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::{LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_REPEAT_CALLS: u32 = 2;
const MAX_TOOL_RESULT_CHARS: usize = 8_000;
const CHILD_PROVIDER_SCOPE_SEPARATOR: &str = "::child-provider-scope::";
/// Web evidence is deliberately allowed a larger envelope than generic tool
/// output. Eight compact evidence excerpts need materially more room than a
/// normal tool response, but the budget remains bounded per tool turn.
pub(crate) const MAX_WEB_TOOL_RESULT_CHARS: usize = 32_000;

pub(crate) fn scoped_child_provider_run_id(parent_run_id: &str, child_id: &str) -> String {
    format!("{parent_run_id}{CHILD_PROVIDER_SCOPE_SEPARATOR}{child_id}")
}

pub(crate) fn parent_run_id_for_provider_scope(run_id: &str) -> &str {
    run_id
        .split_once(CHILD_PROVIDER_SCOPE_SEPARATOR)
        .map_or(run_id, |(parent_run_id, _)| parent_run_id)
}

/// Result of a fully bounded model/tool exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentToolLoopOutcome {
    /// Final assistant content emitted only after the model has stopped calling tools.
    pub(crate) content: String,
    /// Number of model turns used by this Run.
    pub(crate) model_turns: u32,
    /// Number of concrete tool dispatch attempts made by this Run.
    pub(crate) tool_calls: u32,
    /// Provider-reported input tokens consumed across all model turns.
    pub(crate) prompt_tokens: u32,
    /// Provider-reported output tokens consumed across all model turns.
    pub(crate) completion_tokens: u32,
    /// Provider-reported total tokens consumed across all model turns.
    pub(crate) total_tokens: u32,
}

/// Budget consumed so far, including execution paths that end in an error.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AgentToolLoopUsage {
    pub(crate) model_turns: u32,
    pub(crate) tool_calls: u32,
    pub(crate) prompt_tokens: u32,
    pub(crate) completion_tokens: u32,
    pub(crate) total_tokens: u32,
}

/// Per-model-turn limits that the provider must forward into `GatewayRequest`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct AgentModelTurnBudget {
    pub(crate) input_token_budget: Option<u32>,
    pub(crate) max_output_tokens: Option<u32>,
}

/// Provider-facing side of a model/tool loop.
pub(crate) trait ToolLoopProvider: Send + Sync {
    /// Execute one model turn against the current canonical transcript.
    fn answer_turn<'a>(
        &'a self,
        run_id: &'a str,
        messages: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        budget: AgentModelTurnBudget,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>>;
}

/// Run-bound side of a tool loop.
pub(crate) trait ToolLoopExecutor: Send + Sync {
    /// Validate, authorize, audit and execute one model-requested tool call.
    fn execute<'a>(
        &'a self,
        run_id: &'a str,
        call: &'a ToolCall,
        step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>>;

    /// Evidence registered by this Run's tool calls for final-message binding.
    fn evidence_ids(&self) -> Vec<i64> {
        Vec::new()
    }

    /// Whether this Run has registered usable Web evidence (for deferred degradation).
    fn has_web_evidence(&self) -> bool {
        false
    }

    /// Whether a final model answer is invalid until this executor has
    /// registered usable Web evidence for the Run.
    fn requires_web_evidence(&self) -> bool {
        false
    }

    /// Emit a deferred Web degradation notice after the tool loop succeeds.
    /// Returns `true` when a `capability_degraded` event was emitted for this Run.
    /// Default executors have nothing to report.
    fn emit_deferred_web_degradation_if_needed(
        &self,
        _db: &Database,
        _sink: &dyn RunEventSink,
    ) -> AppResult<bool> {
        Ok(false)
    }
}

/// Executes the only permitted shape of an Agent tool loop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentToolLoop {
    max_model_turns: u32,
    max_tool_calls: u32,
    turn_budget: AgentModelTurnBudget,
}

impl AgentToolLoop {
    /// Build the exact parent-loop limits frozen at Request Intake.
    pub(crate) fn from_policy(policy: &RunBudgetPolicy) -> Self {
        Self {
            max_model_turns: policy.max_model_turns,
            max_tool_calls: policy.max_tool_calls,
            turn_budget: AgentModelTurnBudget::default(),
        }
    }

    /// Build the fixed depth-one ChildRun limits frozen in a delegated parent policy.
    pub(crate) fn from_child_policy(policy: &RunBudgetPolicy) -> Self {
        Self {
            max_model_turns: policy.child_max_model_turns,
            max_tool_calls: policy.child_max_tool_calls,
            turn_budget: AgentModelTurnBudget {
                input_token_budget: Some(policy.child_input_tokens_per_turn),
                max_output_tokens: Some(policy.child_output_tokens_per_turn),
            },
        }
    }

    /// Run model turns until a non-empty final answer is received or a bound is reached.
    pub(crate) async fn execute(
        &self,
        provider: &(impl ToolLoopProvider + ?Sized),
        executor: &impl ToolLoopExecutor,
        run_id: &str,
        messages: Vec<LlmMessage>,
        tools: Vec<ToolSpec>,
        observer: &mut dyn StreamEventObserver,
    ) -> AppResult<AgentToolLoopOutcome> {
        self.execute_internal(
            provider, executor, run_id, run_id, messages, tools, observer, None, None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_child(
        &self,
        provider: &(impl ToolLoopProvider + ?Sized),
        executor: &impl ToolLoopExecutor,
        parent_run_id: &str,
        provider_run_id: &str,
        messages: Vec<LlmMessage>,
        tools: Vec<ToolSpec>,
        observer: &mut dyn StreamEventObserver,
        usage: &mut AgentToolLoopUsage,
    ) -> AppResult<AgentToolLoopOutcome> {
        *usage = AgentToolLoopUsage::default();
        self.execute_internal(
            provider,
            executor,
            parent_run_id,
            provider_run_id,
            messages,
            tools,
            observer,
            None,
            Some(usage),
        )
        .await
    }

    /// Evaluation-only seam that observes the real bounded loop without
    /// persisting measurements or changing production dispatch behavior.
    #[allow(clippy::too_many_arguments)]
    pub(crate) async fn execute_with_eval_telemetry(
        &self,
        provider: &(impl ToolLoopProvider + ?Sized),
        executor: &impl ToolLoopExecutor,
        run_id: &str,
        messages: Vec<LlmMessage>,
        tools: Vec<ToolSpec>,
        observer: &mut dyn StreamEventObserver,
        telemetry: &crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap,
    ) -> AppResult<AgentToolLoopOutcome> {
        self.execute_internal(
            provider,
            executor,
            run_id,
            run_id,
            messages,
            tools,
            observer,
            Some(telemetry),
            None,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn execute_internal(
        &self,
        provider: &(impl ToolLoopProvider + ?Sized),
        executor: &impl ToolLoopExecutor,
        run_id: &str,
        provider_run_id: &str,
        mut messages: Vec<LlmMessage>,
        tools: Vec<ToolSpec>,
        observer: &mut dyn StreamEventObserver,
        telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
        mut usage: Option<&mut AgentToolLoopUsage>,
    ) -> AppResult<AgentToolLoopOutcome> {
        let allowed_tools = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<HashSet<_>>();
        let mut model_turns = 0;
        let mut tool_calls = 0;
        let mut prompt_tokens = 0_u32;
        let mut completion_tokens = 0_u32;
        let mut total_tokens = 0_u32;
        let mut fingerprints = HashMap::<String, u32>::new();

        while model_turns < self.max_model_turns {
            ensure_run_not_cancelled(run_id)?;
            model_turns += 1;
            if let Some(usage) = usage.as_deref_mut() {
                usage.model_turns = model_turns;
            }
            let model_started_at = std::time::Instant::now();
            let response = provider
                .answer_turn(
                    provider_run_id,
                    &messages,
                    &tools,
                    self.turn_budget,
                    observer,
                )
                .await?;
            prompt_tokens = prompt_tokens.saturating_add(response.usage.prompt_tokens);
            completion_tokens = completion_tokens.saturating_add(response.usage.completion_tokens);
            total_tokens = total_tokens.saturating_add(response.usage.total_tokens);
            if let Some(usage) = usage.as_deref_mut() {
                usage.prompt_tokens = prompt_tokens;
                usage.completion_tokens = completion_tokens;
                usage.total_tokens = total_tokens;
            }
            if let Some(telemetry) = telemetry {
                telemetry.record_model_turn(&response, model_started_at);
            }

            if response.tool_calls.is_empty() {
                if executor.requires_web_evidence() && !executor.has_web_evidence() {
                    return Err(AppError::msg("agent_run_web_evidence_required"));
                }
                let content = response.content.unwrap_or_default();
                if content.trim().is_empty() {
                    return Err(AppError::msg("agent_run_invalid_model_response"));
                }
                return Ok(AgentToolLoopOutcome {
                    content,
                    model_turns,
                    tool_calls,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                });
            }

            if tool_calls.saturating_add(response.tool_calls.len() as u32) > self.max_tool_calls {
                if let Some(telemetry) = telemetry {
                    telemetry.record_budget(
                        crate::ai_runtime::agent_capacity_eval::BudgetOutcome::ToolCallsExhausted,
                    );
                }
                return Err(AppError::msg("agent_run_tool_loop_limit"));
            }

            observer.on_tools_starting()?;
            messages.push(assistant_tool_message(&response));
            for call in &response.tool_calls {
                ensure_run_not_cancelled(run_id)?;
                tool_calls += 1;
                if let Some(usage) = usage.as_deref_mut() {
                    usage.tool_calls = tool_calls;
                }
                let valid_arguments = valid_call_arguments(call);
                let executor_owns_invalid_arguments =
                    call.function.name == "spawn_subagent" && valid_call_identity(call);
                let result = if !allowed_tools.contains(call.function.name.as_str()) {
                    rejected_result(call, "tool_not_in_run_surface")
                } else if !valid_arguments && !executor_owns_invalid_arguments {
                    rejected_result(call, "tool_arguments_invalid")
                } else {
                    let fingerprint = tool_fingerprint(call);
                    let count = fingerprints.entry(fingerprint).or_insert(0);
                    *count += 1;
                    if *count > MAX_REPEAT_CALLS {
                        rejected_result(call, "tool_call_repeated")
                    } else {
                        executor.execute(run_id, call, tool_calls).await?
                    }
                };
                let (message, truncated) = tool_result_message(call, &result);
                if truncated {
                    if let Some(telemetry) = telemetry {
                        telemetry.record_truncation(
                            crate::ai_runtime::agent_capacity_eval::TruncationOutcome::ToolResultTruncated,
                        );
                    }
                }
                messages.push(message);
            }
            observer.on_tools_finished()?;
        }

        if let Some(telemetry) = telemetry {
            telemetry.record_budget(
                crate::ai_runtime::agent_capacity_eval::BudgetOutcome::ModelTurnsExhausted,
            );
        }
        Err(AppError::msg("agent_run_tool_loop_limit"))
    }
}

fn ensure_run_not_cancelled(run_id: &str) -> AppResult<()> {
    if crate::ai_runtime::model_gateway::is_abort_requested(run_id) {
        Err(AppError::msg("agent_run_cancelled"))
    } else {
        Ok(())
    }
}

fn assistant_tool_message(response: &GatewayResponse) -> LlmMessage {
    LlmMessage {
        role: MessageRole::Assistant,
        content: response.content.clone().unwrap_or_default().into(),
        tool_call_id: None,
        tool_calls: Some(response.tool_calls.clone()),
        reasoning_content: response.reasoning_content.clone(),
    }
}

fn tool_result_message(call: &ToolCall, result: &ToolCallResult) -> (LlmMessage, bool) {
    let payload = serde_json::json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
    });
    let serialized = serde_json::to_string(&payload).unwrap_or_else(|_| {
        "{\"success\":false,\"error\":\"tool_result_serialization_failed\"}".into()
    });
    let budget = tool_result_char_budget(&call.function.name);
    let truncated = serialized.chars().count() > budget;
    // Web evidence is a structured protocol packet, not prose. Slicing it
    // would turn a capacity problem into malformed JSON and let the model
    // reason over a partial, unverifiable result. The normal Web executor
    // packs its output below this limit; this branch is a fail-closed guard
    // for every other executor (including harness implementations).
    if truncated && call.function.name == "web_search" {
        let overflow = serde_json::json!({
            "success": false,
            "output": serde_json::Value::Null,
            "error": "web_evidence_pack_overflow",
        });
        let content = serde_json::to_string(&overflow).unwrap_or_else(|_| {
            "{\"success\":false,\"error\":\"web_evidence_pack_overflow\"}".into()
        });
        return (
            LlmMessage {
                role: MessageRole::Tool,
                content: content.into(),
                tool_call_id: Some(call.id.clone()),
                tool_calls: None,
                reasoning_content: None,
            },
            false,
        );
    }
    let content = truncate_chars(&serialized, budget);
    (
        LlmMessage {
            role: MessageRole::Tool,
            content: content.into(),
            tool_call_id: Some(call.id.clone()),
            tool_calls: None,
            reasoning_content: None,
        },
        truncated,
    )
}

fn tool_result_char_budget(tool_name: &str) -> usize {
    if tool_name == "web_search" {
        MAX_WEB_TOOL_RESULT_CHARS
    } else {
        MAX_TOOL_RESULT_CHARS
    }
}

fn valid_call_arguments(call: &ToolCall) -> bool {
    valid_call_identity(call)
        && serde_json::from_str::<serde_json::Value>(&call.function.arguments)
            .is_ok_and(|value| value.is_object())
}

fn valid_call_identity(call: &ToolCall) -> bool {
    !call.id.trim().is_empty() && !call.function.name.trim().is_empty()
}

fn tool_fingerprint(call: &ToolCall) -> String {
    let arguments = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
        .ok()
        .and_then(|value| canonical_json(&value))
        .unwrap_or_else(|| call.function.arguments.clone());
    format!("{}:{arguments}", call.function.name)
}

fn canonical_json(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let values = keys
                .into_iter()
                .map(|key| Some(format!("{key}:{}", canonical_json(&map[key])?)))
                .collect::<Option<Vec<_>>>()?;
            Some(format!("{{{}}}", values.join(",")))
        }
        serde_json::Value::Array(values) => Some(format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Option<Vec<_>>>()?
                .join(",")
        )),
        _ => serde_json::to_string(value).ok(),
    }
}

fn rejected_result(call: &ToolCall, reason: &str) -> ToolCallResult {
    ToolCallResult {
        tool_name: call.function.name.clone(),
        success: false,
        output: serde_json::json!({ "error": reason }),
        duration_ms: 0,
        tokens_used: None,
        error: Some(reason.to_string()),
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}
