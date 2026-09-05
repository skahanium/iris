//! Bounded, provider-neutral model/tool orchestration for one Agent Run.
//!
//! This module owns transcript integrity and loop limits only. Permission checks,
//! confirmation persistence, audit writes and the concrete tool dispatch remain in
//! the Run-bound executor supplied by the caller.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use crate::ai_runtime::final_answer_submission::{FinalAnswerSubmission, FINAL_ANSWER_TOOL_NAME};
use crate::ai_runtime::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::run_contract::RunBudgetPolicy;
use crate::ai_runtime::run_contract::SafeRunErrorCode;
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::tool_catalog::{catalog_tool_budget_class, ToolBudgetClass};
use crate::ai_runtime::{LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_REPEAT_CALLS: u32 = 2;
const MAX_DISCOVERY_CALLS_PER_MODEL_TURN: u32 = 2;
const MAX_TOOL_RESULT_CHARS: usize = 8_000;
/// Internal control-flow signal: a complete immutable change set was persisted
/// and the Run must wait for its single user confirmation.
pub(crate) const CONFIRMATION_PENDING_ERROR: &str = "agent_run_confirmation_pending";
const CHILD_PROVIDER_SCOPE_SEPARATOR: &str = "::child-provider-scope::";
/// Web evidence is deliberately allowed a larger envelope than generic tool
/// output. Twelve compact evidence excerpts need materially more room than a
/// normal tool response, but the budget remains bounded per tool turn.
pub(crate) const MAX_WEB_TOOL_RESULT_CHARS: usize = 32_000;
/// Conversation compaction is auxiliary work inside the current Run. Keep its
/// completion envelope small so it cannot consume the answer turn's output
/// allowance.
const MAX_MEMORY_COMPACTION_OUTPUT_TOKENS: u32 = 1_024;

pub(crate) fn scoped_child_provider_run_id(parent_run_id: &str, child_id: &str) -> String {
    format!("{parent_run_id}{CHILD_PROVIDER_SCOPE_SEPARATOR}{child_id}")
}

pub(crate) fn parent_run_id_for_provider_scope(run_id: &str) -> &str {
    run_id
        .split_once(CHILD_PROVIDER_SCOPE_SEPARATOR)
        .map_or(run_id, |(parent_run_id, _)| parent_run_id)
}

/// Return whether a model response is a narrowly shaped, non-factual
/// clarification rather than an answer that could evade a required-evidence
/// contract. The host only accepts this before any tool dispatch and only as a
/// single, source-free question; all other WebRequired output still needs
/// current-Run evidence.
pub(crate) fn is_natural_clarification(content: &str) -> bool {
    let question = content.trim();
    let question_count = question
        .chars()
        .filter(|character| matches!(character, '?' | '？'))
        .count();
    let asks_user_for_input = [
        "请告诉我",
        "请提供",
        "请说明",
        "请确认",
        "请选择",
        "请问你",
        "能否告诉我",
        "你在哪",
        "您在哪",
        "你希望",
        "您希望",
        "Could you",
        "Please ",
        "Which ",
        "What ",
        "Where ",
        "When ",
        "Who ",
        "How ",
        "Do you ",
        "Would you ",
        "Can you ",
    ]
    .iter()
    .any(|prefix| question.starts_with(prefix))
        || (question.starts_with("为了")
            && ["请告诉我", "请提供", "请说明", "请确认", "请选择"]
                .iter()
                .any(|request| question.contains(request)));
    asks_user_for_input
        && !question.is_empty()
        && question.chars().count() <= 512
        && question_count == 1
        && question.ends_with(['?', '？'])
        && !question.contains('\n')
        && !question.contains("http://")
        && !question.contains("https://")
        && !question.contains("[W")
        && !question.contains("[E")
        && !question.contains("[L")
        && !question.contains("[M")
        && !question
            .chars()
            .any(|character| matches!(character, '.' | '。' | '!' | '！'))
}

/// Result of a fully bounded model/tool exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentToolLoopOutcome {
    /// Final assistant content emitted only after the model has stopped calling tools.
    pub(crate) content: String,
    /// Provider stop reason associated with the final assistant content.
    pub(crate) finish_reason: String,
    /// Internal structured submission when the model used the reserved final
    /// answer tool. It never enters the model transcript or tool audit.
    pub(crate) final_submission: Option<FinalAnswerSubmission>,
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

/// A Host-executed minimum observation for a WebRequired Run. It deliberately
/// carries only the compact, model-visible observation and counted dispatches;
/// it is never encoded as an assistant tool call.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RequiredWebBootstrapObservation {
    pub(crate) tool_calls: u32,
    pub(crate) network_tool_calls: u32,
    pub(crate) observation: String,
}

/// Per-model-turn limits that the provider must forward into `GatewayRequest`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct AgentModelTurnBudget {
    pub(crate) max_prompt_tokens: Option<u32>,
    pub(crate) max_completion_tokens: Option<u32>,
    pub(crate) max_turn_output_tokens: Option<u32>,
}

impl Default for AgentModelTurnBudget {
    fn default() -> Self {
        Self {
            max_prompt_tokens: Some(64_000),
            max_completion_tokens: Some(8_000),
            max_turn_output_tokens: Some(8_000),
        }
    }
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

    /// Record the first concrete tool dispatch for this provider continuation.
    /// A model merely proposing a call does not make a cross-provider retry
    /// unsafe: the Host may still reject or defer that proposal before any
    /// external action or canonical tool transcript exists.
    fn on_tool_call_dispatched(&self, _run_id: &str) -> AppResult<()> {
        Ok(())
    }

    /// Discard a response that proposed only invalid or deferred actions.
    /// Implementations with provider-private continuations must not retain a
    /// continuation that has no matching assistant/tool exchange.
    fn on_tool_proposals_not_dispatched(&self, _run_id: &str) -> AppResult<()> {
        Ok(())
    }
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

    /// Perform the bounded initial observation required by a CurrentRunWeb
    /// contract. Default executors do not own a Web implementation.
    fn bootstrap_required_web_observation<'a>(
        &'a self,
        _run_id: &'a str,
        _remaining_tool_calls: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<Option<RequiredWebBootstrapObservation>>> + Send + 'a>>
    {
        Box::pin(async { Ok(None) })
    }

    /// Number of logical Web actions this executor will deterministically
    /// dispatch before the first model turn. Only executors that actually own
    /// the Host bootstrap opt in; generic test/read-only executors retain the
    /// ordinary model-driven Web contract.
    fn required_web_bootstrap_action_count(&self) -> u32 {
        0
    }

    /// Request one bounded no-tool compression turn for the durable
    /// conversation summary. The default keeps non-normal executors free of
    /// session-memory concerns.
    fn conversation_memory_compaction_request(
        &self,
    ) -> AppResult<
        Option<crate::ai_runtime::conversation_memory::ConversationMemoryCompactionRequest>,
    > {
        Ok(None)
    }

    /// Apply the model compression output, or its deterministic fallback.
    /// Failure here is intentionally non-fatal: compaction must never block
    /// the active user Run.
    fn apply_conversation_memory_compaction(
        &self,
        _request: &crate::ai_runtime::conversation_memory::ConversationMemoryCompactionRequest,
        _output: Option<&str>,
    ) -> AppResult<Option<crate::ai_runtime::conversation_memory::ConversationMemory>> {
        Ok(None)
    }

    /// Persist one complete confirmation-bound change set. The default rejects
    /// the request so test/read-only executors cannot accidentally gain write
    /// behaviour by merely implementing the ordinary dispatch method.
    fn request_change_set<'a>(
        &'a self,
        _run_id: &'a str,
        _calls: &'a [ToolCall],
        _first_step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<()>> + Send + 'a>> {
        Box::pin(async { Err(AppError::msg("confirmation_batch_not_supported")) })
    }

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

    /// Whether a final model answer is invalid until this executor has
    /// registered evidence from an explicitly granted external read tool.
    fn requires_external_evidence(&self) -> bool {
        false
    }

    /// Whether this Run registered usable evidence through `external.read`.
    fn has_external_evidence(&self) -> bool {
        false
    }

    /// Whether a natural final answer must bind to evidence produced by this
    /// Run.  The executor owns the concrete source syntax because the loop is
    /// deliberately domain- and provider-neutral.
    fn requires_natural_source_binding(&self) -> bool {
        false
    }

    /// Validate source bindings in a natural final answer against this Run's
    /// evidence.  A `false` result asks the loop for one no-tool repair turn;
    /// it is not an internal execution failure.
    fn natural_source_binding_is_valid(&self, _content: &str) -> bool {
        true
    }

    /// Validate a reserved structured final submission against the exact
    /// current-Run source policy before the loop exits. A `false` result uses
    /// the same single no-tool repair slot as a missing terminal submission.
    fn final_submission_is_valid(&self, _submission: &FinalAnswerSubmission) -> bool {
        true
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

    /// Safe Host-authored fallback when a strict evidence contract cannot be
    /// completed. It is never model prose and therefore never gets cited.
    fn evidence_limited_response(&self) -> String {
        EVIDENCE_LIMITED_RESPONSE.to_string()
    }
}

pub(crate) const EVIDENCE_LIMITED_RESPONSE_PREFIX: &str =
    "本轮未取得足够的可核验来源正文来可靠支持具体结论";
pub(crate) const EVIDENCE_LIMITED_RESPONSE: &str =
    "本轮未取得足够的可核验来源正文来可靠支持具体结论，因此不展示未经核实的答复。你可以稍后重试，或提供可核验的来源。";

/// Executes the only permitted shape of an Agent tool loop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentToolLoop {
    max_model_turns: u32,
    max_tool_calls: u32,
    max_local_tool_calls: u32,
    max_network_tool_calls: u32,
    max_external_read_tool_calls: u32,
    max_runtime_tool_calls: u32,
    max_confirmed_change_calls: u32,
    turn_budget: AgentModelTurnBudget,
}

impl AgentToolLoop {
    /// Build the exact parent-loop limits frozen at Request Intake.
    pub(crate) fn from_policy(policy: &RunBudgetPolicy) -> Self {
        Self {
            max_model_turns: policy.max_model_turns,
            max_tool_calls: policy.max_tool_calls,
            max_local_tool_calls: policy.max_local_tool_calls,
            max_network_tool_calls: policy.max_network_tool_calls,
            max_external_read_tool_calls: policy.max_external_read_tool_calls,
            max_runtime_tool_calls: policy.max_runtime_tool_calls,
            max_confirmed_change_calls: policy.max_confirmed_change_calls,
            turn_budget: AgentModelTurnBudget {
                max_prompt_tokens: Some(policy.max_prompt_tokens),
                max_completion_tokens: Some(policy.max_completion_tokens),
                max_turn_output_tokens: Some(policy.max_turn_output_tokens),
            },
        }
    }

    /// Build the fixed depth-one ChildRun limits frozen in a delegated parent policy.
    pub(crate) fn from_child_policy(policy: &RunBudgetPolicy) -> Self {
        Self {
            max_model_turns: policy.child_max_model_turns,
            max_tool_calls: policy.child_max_tool_calls,
            max_local_tool_calls: policy.child_max_tool_calls,
            max_network_tool_calls: policy.child_max_tool_calls,
            max_external_read_tool_calls: policy.child_max_tool_calls,
            max_runtime_tool_calls: policy.child_max_tool_calls,
            max_confirmed_change_calls: policy.child_max_tool_calls,
            turn_budget: AgentModelTurnBudget {
                max_prompt_tokens: Some(policy.child_input_tokens_per_turn),
                max_completion_tokens: Some(
                    policy
                        .child_max_model_turns
                        .saturating_mul(policy.child_output_tokens_per_turn),
                ),
                max_turn_output_tokens: Some(policy.child_output_tokens_per_turn),
            },
        }
    }

    /// Build the only permitted post-confirmation loop. It reuses transcript
    /// and provider limits, but cannot spend normal-run Web/external/runtime or
    /// write budget.
    pub(crate) fn from_post_confirmation_policy(policy: &RunBudgetPolicy) -> Self {
        Self {
            max_model_turns: policy.post_confirmation_max_model_turns,
            max_tool_calls: policy.post_confirmation_max_local_tool_calls,
            max_local_tool_calls: policy.post_confirmation_max_local_tool_calls,
            max_network_tool_calls: 0,
            max_external_read_tool_calls: 0,
            max_runtime_tool_calls: 0,
            max_confirmed_change_calls: 0,
            turn_budget: AgentModelTurnBudget {
                max_prompt_tokens: Some(policy.max_prompt_tokens),
                max_completion_tokens: Some(policy.max_completion_tokens),
                max_turn_output_tokens: Some(policy.max_turn_output_tokens),
            },
        }
    }

    fn tool_call_limit(&self, class: ToolBudgetClass) -> u32 {
        match class {
            ToolBudgetClass::Local => self.max_local_tool_calls,
            ToolBudgetClass::Network => self.max_network_tool_calls,
            ToolBudgetClass::ExternalRead => self.max_external_read_tool_calls,
            ToolBudgetClass::Runtime => self.max_runtime_tool_calls,
            ToolBudgetClass::ConfirmedChange => self.max_confirmed_change_calls,
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
        let loop_state_position = messages
            .iter()
            .rposition(|message| matches!(message.role, MessageRole::User))
            .unwrap_or(messages.len());
        messages.insert(
            loop_state_position,
            initial_loop_budget_instruction(
                self.max_model_turns,
                self.max_tool_calls,
                self.max_local_tool_calls,
                self.max_network_tool_calls,
                self.max_external_read_tool_calls,
                self.max_runtime_tool_calls,
            ),
        );
        let mut model_turns = 0_u32;
        let mut tool_calls = 0;
        let mut prompt_tokens = 0_u32;
        let mut completion_tokens = 0_u32;
        let mut total_tokens = 0_u32;
        let mut fingerprints = HashMap::<String, u32>::new();
        let mut successful_fingerprints = HashSet::<String>::new();
        let mut tool_calls_by_class = HashMap::<ToolBudgetClass, u32>::new();
        let mut observed_progress = HashSet::<String>::new();
        let mut final_submission_repair_used = false;
        let mut missing_evidence_repair_used = false;
        let mut source_binding_repair_used = false;
        let mut source_binding_repair_required = false;
        let mut incomplete_final_answer_repair_used = false;
        let mut incomplete_final_draft = None::<String>;
        // Distinguish an answer that skipped a required Web lookup from one
        // produced after an attempted lookup yielded no usable evidence. The
        // former receives one tool-enabled repair; asking the model to repair
        // the latter cannot create a source and used to turn an empty or
        // failed search into a provider/internal failure.
        let mut web_search_attempted_without_evidence = false;
        let mut no_progress_rounds = 0_u8;
        let mut synthesis_required = false;
        let synthesis_tools = tools
            .iter()
            .filter(|tool| tool.name == FINAL_ANSWER_TOOL_NAME)
            .cloned()
            .collect::<Vec<_>>();
        let requires_factual_completion =
            executor.requires_web_evidence() || executor.requires_external_evidence();

        if model_turns.saturating_add(1) < self.max_model_turns {
            if let Some(compaction) = executor.conversation_memory_compaction_request()? {
                let compaction_messages = vec![
                    LlmMessage {
                        role: MessageRole::System,
                        content: "You are compressing durable conversation memory. Return only the requested JSON; do not answer the user or call tools.".into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    },
                    LlmMessage {
                        role: MessageRole::User,
                        content: compaction.prompt().to_string().into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    },
                ];
                let compaction_budget = AgentModelTurnBudget {
                    max_prompt_tokens: self.turn_budget.max_prompt_tokens,
                    max_completion_tokens: self
                        .turn_budget
                        .max_completion_tokens
                        .map(|limit| limit.min(MAX_MEMORY_COMPACTION_OUTPUT_TOKENS)),
                    max_turn_output_tokens: self
                        .turn_budget
                        .max_turn_output_tokens
                        .map(|limit| limit.min(MAX_MEMORY_COMPACTION_OUTPUT_TOKENS)),
                };
                enforce_prompt_budget(&compaction_messages, &[], compaction_budget)?;
                let mut silent_observer = SilentStreamObserver;
                model_turns = model_turns.saturating_add(1);
                if let Some(usage) = usage.as_deref_mut() {
                    usage.model_turns = model_turns;
                }
                let started = std::time::Instant::now();
                let result = provider
                    .answer_turn(
                        provider_run_id,
                        &compaction_messages,
                        &[],
                        compaction_budget,
                        &mut silent_observer,
                    )
                    .await;
                let output = result
                    .as_ref()
                    .ok()
                    .filter(|response| response.tool_calls.is_empty())
                    .and_then(|response| response.content.as_deref());
                match executor.apply_conversation_memory_compaction(&compaction, output) {
                    Ok(Some(updated_memory)) => {
                        replace_conversation_memory_in_current_messages(
                            &mut messages,
                            compaction.prior_prompt_fragment(),
                            &updated_memory.to_prompt_fragment(),
                        );
                    }
                    Ok(None) => {}
                    Err(error) => {
                        tracing::warn!(
                            run_id,
                            reason = "conversation_memory_compaction_persist_failed",
                            error = %error,
                            "conversation memory compaction was skipped"
                        );
                    }
                }
                if let Ok(response) = result {
                    let (turn_prompt, turn_completion, turn_total) =
                        resolved_turn_usage(&response, &compaction_messages, &[]);
                    prompt_tokens = prompt_tokens.saturating_add(turn_prompt);
                    completion_tokens = completion_tokens.saturating_add(turn_completion);
                    total_tokens = total_tokens.saturating_add(turn_total);
                    if let Some(usage) = usage.as_deref_mut() {
                        usage.prompt_tokens = prompt_tokens;
                        usage.completion_tokens = completion_tokens;
                        usage.total_tokens = total_tokens;
                    }
                    if let Some(telemetry) = telemetry {
                        telemetry.record_model_turn(&response, started);
                    }
                }
            }
        }

        if executor.requires_web_evidence() {
            let bootstrap_actions = executor.required_web_bootstrap_action_count();
            let remaining = self.max_tool_calls.saturating_sub(tool_calls);
            let network_used = tool_calls_by_class
                .get(&ToolBudgetClass::Network)
                .copied()
                .unwrap_or_default();
            let remaining_network = self
                .tool_call_limit(ToolBudgetClass::Network)
                .saturating_sub(network_used);
            // A required-Web Run has a deterministic minimum observation:
            // one search and one bounded fetch batch. Reject before invoking
            // the executor when the frozen contract cannot afford both; doing
            // so after a network side effect used to leave an unauditable
            // half-bootstrap behind.
            if remaining < bootstrap_actions || remaining_network < bootstrap_actions {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
            if let Some(bootstrap) = executor
                .bootstrap_required_web_observation(run_id, remaining)
                .await?
            {
                if bootstrap.tool_calls > remaining
                    || bootstrap.network_tool_calls > bootstrap.tool_calls
                    || network_used.saturating_add(bootstrap.network_tool_calls)
                        > self.tool_call_limit(ToolBudgetClass::Network)
                {
                    return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
                }
                tool_calls = tool_calls.saturating_add(bootstrap.tool_calls);
                *tool_calls_by_class
                    .entry(ToolBudgetClass::Network)
                    .or_default() = network_used.saturating_add(bootstrap.network_tool_calls);
                if let Some(usage) = usage.as_deref_mut() {
                    usage.tool_calls = tool_calls;
                }
                if let Some(telemetry) = telemetry {
                    telemetry.record_bootstrap_web_tool_calls(bootstrap.tool_calls);
                }
                web_search_attempted_without_evidence = true;
                let position = messages
                    .iter()
                    .rposition(|message| matches!(message.role, MessageRole::User))
                    .unwrap_or(messages.len());
                messages.insert(
                    position,
                    LlmMessage {
                        role: MessageRole::System,
                        content: bootstrap.observation.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    },
                );
            }
        }

        while model_turns < self.max_model_turns {
            ensure_run_not_cancelled(run_id)?;
            let is_final_model_turn = model_turns.saturating_add(1) >= self.max_model_turns;
            let remaining_completion_tokens = self
                .turn_budget
                .max_completion_tokens
                .map(|limit| limit.saturating_sub(completion_tokens));
            let synthesis_output_reserve = match (
                self.turn_budget.max_completion_tokens,
                self.turn_budget.max_turn_output_tokens,
            ) {
                (Some(completion_limit), Some(turn_limit)) => completion_limit.min(turn_limit),
                _ => 0,
            };
            // Do not let exploratory turns consume the final synthesis
            // envelope. Once only that reserve remains, close business tools
            // before building the Gateway request so the provider receives a
            // real, enforceable output cap rather than a post-hoc rejection.
            if !synthesis_required
                && !source_binding_repair_required
                && incomplete_final_draft.is_none()
                && !is_final_model_turn
                && remaining_completion_tokens
                    .is_some_and(|remaining| remaining <= synthesis_output_reserve)
            {
                synthesis_required = true;
                messages.push(tool_surface_closed_instruction());
            }
            let active_tools: &[ToolSpec] = if incomplete_final_draft.is_some()
                || synthesis_required
                || source_binding_repair_required
            {
                &synthesis_tools
            } else {
                &tools
            };
            let mut model_turn_budget = self.turn_budget;
            if let Some(remaining) = remaining_completion_tokens {
                if remaining == 0 {
                    return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
                }
                let is_synthesis_turn = synthesis_required
                    || source_binding_repair_required
                    || incomplete_final_draft.is_some()
                    || is_final_model_turn;
                let allowed_output = if is_synthesis_turn {
                    remaining
                } else {
                    remaining.saturating_sub(synthesis_output_reserve)
                };
                if allowed_output == 0 {
                    return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
                }
                model_turn_budget.max_completion_tokens = Some(allowed_output);
                model_turn_budget.max_turn_output_tokens = Some(
                    self.turn_budget
                        .max_turn_output_tokens
                        .map_or(allowed_output, |limit| limit.min(allowed_output)),
                );
            }
            enforce_prompt_budget(&messages, active_tools, model_turn_budget)?;
            if model_turn_budget.max_completion_tokens == Some(0) {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
            model_turns += 1;
            if let Some(usage) = usage.as_deref_mut() {
                usage.model_turns = model_turns;
            }
            let model_started_at = std::time::Instant::now();
            let provider_turn = provider.answer_turn(
                provider_run_id,
                &messages,
                active_tools,
                model_turn_budget,
                observer,
            );
            let response = match provider_turn.await {
                Ok(response) => response,
                Err(error) => {
                    let visible_draft = observer.visible_content_snapshot();
                    if !can_recover_visible_stream_error(&error, visible_draft.as_deref())
                        || incomplete_final_answer_repair_used
                        || model_turns >= self.max_model_turns
                    {
                        return Err(error);
                    }
                    incomplete_final_answer_repair_used = true;
                    let draft = visible_draft.expect("recovery guard requires a visible draft");
                    let draft_completion_tokens = estimate_tokens(&draft);
                    let exceeds_turn_output = model_turn_budget
                        .max_turn_output_tokens
                        .is_some_and(|limit| draft_completion_tokens > limit);
                    let exceeds_run_completion =
                        self.turn_budget.max_completion_tokens.is_some_and(|limit| {
                            completion_tokens.saturating_add(draft_completion_tokens) > limit
                        });
                    if exceeds_turn_output || exceeds_run_completion {
                        return Err(AppError::run(SafeRunErrorCode::OutputTooLong));
                    }
                    completion_tokens = completion_tokens.saturating_add(draft_completion_tokens);
                    total_tokens = total_tokens.saturating_add(draft_completion_tokens);
                    if let Some(usage) = usage.as_deref_mut() {
                        usage.completion_tokens = completion_tokens;
                        usage.total_tokens = total_tokens;
                    }
                    incomplete_final_draft = Some(draft.clone());
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: draft.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    messages.push(incomplete_answer_continuation_instruction());
                    continue;
                }
            };
            let (turn_prompt_tokens, turn_completion_tokens, turn_total_tokens) =
                resolved_turn_usage(&response, &messages, active_tools);
            if model_turn_budget
                .max_prompt_tokens
                .is_some_and(|limit| turn_prompt_tokens > limit)
            {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
            let exceeds_turn_output = model_turn_budget
                .max_turn_output_tokens
                .is_some_and(|limit| turn_completion_tokens > limit);
            let exceeds_run_completion =
                self.turn_budget.max_completion_tokens.is_some_and(|limit| {
                    completion_tokens.saturating_add(turn_completion_tokens) > limit
                });
            if exceeds_turn_output || exceeds_run_completion {
                let code = if response.tool_calls.is_empty() {
                    "agent_run_output_too_long"
                } else {
                    "agent_run_tool_loop_limit"
                };
                return Err(AppError::msg(code));
            }
            prompt_tokens = prompt_tokens.saturating_add(turn_prompt_tokens);
            completion_tokens = completion_tokens.saturating_add(turn_completion_tokens);
            total_tokens = total_tokens.saturating_add(turn_total_tokens);
            if let Some(usage) = usage.as_deref_mut() {
                usage.prompt_tokens = prompt_tokens;
                usage.completion_tokens = completion_tokens;
                usage.total_tokens = total_tokens;
            }
            if let Some(telemetry) = telemetry {
                telemetry.record_model_turn(&response, model_started_at);
            }

            if incomplete_final_draft.is_some() && !response.tool_calls.is_empty() {
                return Err(AppError::run(SafeRunErrorCode::IncompleteOutput));
            }

            if response.tool_calls.is_empty() {
                let response_content = response.content.unwrap_or_default();
                let content = match incomplete_final_draft.take() {
                    Some(draft) => append_final_answer_continuation(draft, response_content)?,
                    None => response_content,
                };
                if content.trim().is_empty() {
                    return Err(AppError::msg("agent_run_invalid_model_response"));
                }
                // A necessary clarification remains a normal completion even
                // when the Host has already gathered a bounded observation.
                // Bootstrap dispatches are not evidence that the missing user
                // constraint was available, and must not turn a question into
                // a misleading evidence-limited refusal.
                let natural_clarification = is_natural_clarification(&content);
                if executor.requires_web_evidence()
                    && !executor.has_web_evidence()
                    && !natural_clarification
                {
                    if web_search_attempted_without_evidence
                        || missing_evidence_repair_used
                        || model_turns >= self.max_model_turns
                    {
                        return Ok(evidence_limited_outcome(
                            executor.evidence_limited_response(),
                            model_turns,
                            tool_calls,
                            prompt_tokens,
                            completion_tokens,
                            total_tokens,
                        ));
                    }
                    missing_evidence_repair_used = true;
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: content.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    messages.push(missing_evidence_repair_instruction());
                    continue;
                }
                if executor.requires_external_evidence()
                    && !executor.has_external_evidence()
                    && !natural_clarification
                {
                    if missing_evidence_repair_used || model_turns >= self.max_model_turns {
                        return Ok(evidence_limited_outcome(
                            executor.evidence_limited_response(),
                            model_turns,
                            tool_calls,
                            prompt_tokens,
                            completion_tokens,
                            total_tokens,
                        ));
                    }
                    missing_evidence_repair_used = true;
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: content.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    messages.push(missing_evidence_repair_instruction());
                    continue;
                }
                if allowed_tools.contains(FINAL_ANSWER_TOOL_NAME) {
                    if final_submission_repair_used {
                        return Err(AppError::msg("agent_run_final_submission_required"));
                    }
                    final_submission_repair_used = true;
                    // The withheld draft is continuation context only. It is
                    // never persisted or emitted, and the correction surface
                    // exposes no business tools beyond the reserved terminal
                    // submission tool already present in `tools`.
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: content.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    messages.push(LlmMessage {
                        role: MessageRole::System,
                        content: "The previous draft cannot be shown because this Run requires verified source bindings. Source binding identifies the current Run's permitted sources; it does not establish support for individual claims. Submit the same answer only through submit_final_answer now.".into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    continue;
                }
                if crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity::needs_recovery(
                    &content,
                    &response.finish_reason,
                    requires_factual_completion,
                ) {
                    if incomplete_final_answer_repair_used || model_turns >= self.max_model_turns {
                        return Err(AppError::run(SafeRunErrorCode::IncompleteOutput));
                    }
                    incomplete_final_answer_repair_used = true;
                    incomplete_final_draft = Some(content.clone());
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: content.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    messages.push(incomplete_answer_continuation_instruction());
                    continue;
                }
                if executor.requires_natural_source_binding()
                    && !natural_clarification
                    && !executor.natural_source_binding_is_valid(&content)
                {
                    if source_binding_repair_used || model_turns >= self.max_model_turns {
                        return Ok(evidence_limited_outcome(
                            executor.evidence_limited_response(),
                            model_turns,
                            tool_calls,
                            prompt_tokens,
                            completion_tokens,
                            total_tokens,
                        ));
                    }
                    source_binding_repair_used = true;
                    source_binding_repair_required = true;
                    messages.push(LlmMessage {
                        role: MessageRole::Assistant,
                        content: content.into(),
                        tool_call_id: None,
                        tool_calls: None,
                        reasoning_content: None,
                    });
                    messages.push(source_binding_repair_instruction());
                    continue;
                }
                return Ok(AgentToolLoopOutcome {
                    content,
                    finish_reason: response.finish_reason,
                    final_submission: None,
                    model_turns,
                    tool_calls,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                });
            }

            let active_allowed_tools = active_tools
                .iter()
                .map(|tool| tool.name.as_str())
                .collect::<HashSet<_>>();
            let final_submission = final_answer_submission(&response, &active_allowed_tools);
            if let Ok(Some(submission)) = final_submission.as_ref() {
                if executor.final_submission_is_valid(submission) {
                    return Ok(AgentToolLoopOutcome {
                        content: submission.visible_content(),
                        finish_reason: response.finish_reason,
                        final_submission: Some(submission.clone()),
                        model_turns,
                        tool_calls,
                        prompt_tokens,
                        completion_tokens,
                        total_tokens,
                    });
                }
            }
            let single_final_call = response.tool_calls.len() == 1
                && response.tool_calls[0].function.name == FINAL_ANSWER_TOOL_NAME;
            if single_final_call {
                if final_submission_repair_used || model_turns >= self.max_model_turns {
                    return Ok(evidence_limited_outcome(
                        executor.evidence_limited_response(),
                        model_turns,
                        tool_calls,
                        prompt_tokens,
                        completion_tokens,
                        total_tokens,
                    ));
                }
                final_submission_repair_used = true;
                synthesis_required = true;
                messages.push(assistant_tool_message(&response));
                let call = &response.tool_calls[0];
                let result = rejected_result(call, "final_submission_validation_failed");
                let (message, _) = tool_result_message(
                    call,
                    &result,
                    self.max_model_turns.saturating_sub(model_turns),
                    self.max_tool_calls.saturating_sub(tool_calls),
                    self.tool_call_limit(ToolBudgetClass::ExternalRead),
                );
                messages.push(message);
                continue;
            }
            final_submission?;

            let confirmation_calls = response
                .tool_calls
                .iter()
                .filter(|call| {
                    active_tools
                        .iter()
                        .find(|tool| tool.name == call.function.name)
                        .is_some_and(|tool| tool.requires_confirmation)
                })
                .count();
            if confirmation_calls > 0 {
                let all_confirmation_calls = confirmation_calls == response.tool_calls.len();
                let all_valid = response.tool_calls.iter().all(|call| {
                    active_allowed_tools.contains(call.function.name.as_str())
                        && valid_call_arguments(call)
                });
                let requested = u32::try_from(response.tool_calls.len())
                    .map_err(|_| AppError::run(SafeRunErrorCode::ToolLoopLimit))?;
                let class_used = tool_calls_by_class
                    .get(&ToolBudgetClass::ConfirmedChange)
                    .copied()
                    .unwrap_or_default();
                if !all_confirmation_calls || !all_valid {
                    return Err(AppError::msg("mixed_confirmation_batch"));
                }
                if tool_calls.saturating_add(requested) > self.max_tool_calls
                    || class_used.saturating_add(requested)
                        > self.tool_call_limit(ToolBudgetClass::ConfirmedChange)
                {
                    return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
                }
                observer.on_tools_starting()?;
                executor
                    .request_change_set(run_id, &response.tool_calls, tool_calls.saturating_add(1))
                    .await?;
                return Err(AppError::msg(CONFIRMATION_PENDING_ERROR));
            }

            let mut discovery_calls_this_turn = 0_u32;
            let proposal_dispositions = plan_tool_proposals(
                &response.tool_calls,
                &active_allowed_tools,
                discovery_calls_this_turn,
                tool_calls,
                &tool_calls_by_class,
                &successful_fingerprints,
                &fingerprints,
                self,
            );
            if proposal_dispositions
                .iter()
                .all(|(_, disposition)| *disposition != ToolCallDisposition::Dispatched)
            {
                provider.on_tool_proposals_not_dispatched(provider_run_id)?;
                messages.push(tool_proposal_feedback_instruction(&proposal_dispositions));
                no_progress_rounds = no_progress_rounds.saturating_add(1);
                if no_progress_rounds >= 2 || model_turns.saturating_add(1) >= self.max_model_turns
                {
                    synthesis_required = true;
                    messages.push(tool_surface_closed_instruction());
                }
                continue;
            }

            let mut dispatched_response = response.clone();
            dispatched_response.tool_calls = proposal_dispositions
                .iter()
                .filter_map(|(call, disposition)| {
                    (*disposition == ToolCallDisposition::Dispatched).then_some((*call).clone())
                })
                .collect();
            let non_dispatched = proposal_dispositions
                .iter()
                .filter(|(_, disposition)| *disposition != ToolCallDisposition::Dispatched)
                .copied()
                .collect::<Vec<_>>();
            observer.on_tools_starting()?;
            messages.push(assistant_tool_message(&dispatched_response));
            let mut round_made_progress = false;
            for call in &dispatched_response.tool_calls {
                ensure_run_not_cancelled(run_id)?;
                let valid_arguments = valid_call_arguments(call);
                let executor_owns_invalid_arguments =
                    call.function.name == "spawn_subagent" && valid_call_identity(call);
                let result = if !active_allowed_tools.contains(call.function.name.as_str()) {
                    rejected_result(call, "tool_not_in_run_surface")
                } else if !valid_arguments && !executor_owns_invalid_arguments {
                    rejected_result(call, "tool_arguments_invalid")
                } else if is_discovery_call(call)
                    && discovery_calls_this_turn >= MAX_DISCOVERY_CALLS_PER_MODEL_TURN
                {
                    deferred_result(call)
                } else {
                    let fingerprint = tool_fingerprint(call);
                    if successful_fingerprints.contains(&fingerprint) {
                        rejected_result(call, "tool_call_already_succeeded")
                    } else {
                        let count = fingerprints.entry(fingerprint.clone()).or_insert(0);
                        *count += 1;
                        if *count > MAX_REPEAT_CALLS {
                            rejected_result(call, "tool_call_repeated")
                        } else if tool_calls >= self.max_tool_calls {
                            if let Some(telemetry) = telemetry {
                                telemetry.record_budget(
                                    crate::ai_runtime::agent_capacity_eval::BudgetOutcome::ToolCallsExhausted,
                                );
                            }
                            rejected_result(call, "tool_call_budget_exhausted")
                        } else {
                            let class = catalog_tool_budget_class(&call.function.name)
                                .unwrap_or(ToolBudgetClass::ExternalRead);
                            let used = tool_calls_by_class.entry(class).or_insert(0);
                            if *used >= self.tool_call_limit(class) {
                                rejected_result(call, "tool_category_budget_exhausted")
                            } else {
                                if is_discovery_call(call) {
                                    discovery_calls_this_turn =
                                        discovery_calls_this_turn.saturating_add(1);
                                }
                                *used += 1;
                                tool_calls += 1;
                                if let Some(usage) = usage.as_deref_mut() {
                                    usage.tool_calls = tool_calls;
                                }
                                if let Some(telemetry) = telemetry {
                                    telemetry.record_executed_tool_call(&call.function.name);
                                }
                                provider.on_tool_call_dispatched(provider_run_id)?;
                                let result = executor.execute(run_id, call, tool_calls).await?;
                                if matches!(
                                    call.function.name.as_str(),
                                    "web_search" | "web.search"
                                ) {
                                    web_search_attempted_without_evidence = true;
                                }
                                if result.success {
                                    successful_fingerprints.insert(fingerprint);
                                }
                                round_made_progress |=
                                    register_safe_progress(&mut observed_progress, &result);
                                result
                            }
                        }
                    }
                };
                let class = catalog_tool_budget_class(&call.function.name)
                    .unwrap_or(ToolBudgetClass::ExternalRead);
                let class_used = tool_calls_by_class.get(&class).copied().unwrap_or_default();
                let (message, truncated) = tool_result_message(
                    call,
                    &result,
                    self.max_model_turns.saturating_sub(model_turns),
                    self.max_tool_calls.saturating_sub(tool_calls),
                    self.tool_call_limit(class).saturating_sub(class_used),
                );
                if truncated {
                    if let Some(telemetry) = telemetry {
                        telemetry.record_truncation(
                            crate::ai_runtime::agent_capacity_eval::TruncationOutcome::ToolResultTruncated,
                        );
                    }
                }
                messages.push(message);
            }
            if !non_dispatched.is_empty() {
                messages.push(tool_proposal_feedback_instruction(&non_dispatched));
            }
            observer.on_tools_finished()?;
            if round_made_progress {
                no_progress_rounds = 0;
            } else {
                no_progress_rounds = no_progress_rounds.saturating_add(1);
            }
            // Never spend the final model turn on another exploratory tool
            // request. Once this round has left only one turn, close the
            // business surface and reserve that final opportunity for
            // synthesis (or the terminal structured submission tool).
            let final_turn_must_be_reserved = model_turns.saturating_add(1) >= self.max_model_turns;
            if no_progress_rounds >= 2
                || tool_calls >= self.max_tool_calls
                || final_turn_must_be_reserved
            {
                synthesis_required = true;
                messages.push(tool_surface_closed_instruction());
            }
        }

        if let Some(telemetry) = telemetry {
            telemetry.record_budget(
                crate::ai_runtime::agent_capacity_eval::BudgetOutcome::ModelTurnsExhausted,
            );
        }
        Err(AppError::msg(if incomplete_final_draft.is_some() {
            "agent_run_incomplete_output"
        } else {
            "agent_run_tool_loop_limit"
        }))
    }
}

/// The Host's pre-dispatch classification is deliberately narrow. It does not
/// replace executor authorization; it only prevents a completely rejected or
/// deferred model proposal from becoming a fake provider-bound tool turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCallDisposition {
    Rejected,
    Deferred,
    Dispatched,
}

struct SilentStreamObserver;

impl StreamEventObserver for SilentStreamObserver {
    fn observe(
        &mut self,
        _event: &crate::ai_runtime::model_gateway::StreamEvent,
        _token_index: u32,
    ) -> AppResult<()> {
        Ok(())
    }
}

#[allow(clippy::too_many_arguments)]
fn plan_tool_proposals<'a>(
    calls: &'a [ToolCall],
    active_allowed_tools: &HashSet<&str>,
    discovery_calls_this_turn: u32,
    tool_calls: u32,
    tool_calls_by_class: &HashMap<ToolBudgetClass, u32>,
    successful_fingerprints: &HashSet<String>,
    fingerprints: &HashMap<String, u32>,
    loop_policy: &AgentToolLoop,
) -> Vec<(&'a ToolCall, ToolCallDisposition)> {
    let mut planned_discovery = discovery_calls_this_turn;
    let mut planned_total = tool_calls;
    let mut planned_by_class = tool_calls_by_class.clone();
    let mut planned_fingerprints = fingerprints.clone();
    calls
        .iter()
        .map(|call| {
            let executor_owns_invalid_arguments =
                call.function.name == "spawn_subagent" && valid_call_identity(call);
            if !active_allowed_tools.contains(call.function.name.as_str())
                || (!valid_call_arguments(call) && !executor_owns_invalid_arguments)
                || planned_total >= loop_policy.max_tool_calls
            {
                return (call, ToolCallDisposition::Rejected);
            }
            let fingerprint = tool_fingerprint(call);
            if successful_fingerprints.contains(&fingerprint) {
                return (call, ToolCallDisposition::Rejected);
            }
            if is_discovery_call(call) && planned_discovery >= MAX_DISCOVERY_CALLS_PER_MODEL_TURN {
                return (call, ToolCallDisposition::Deferred);
            }
            let count = planned_fingerprints.entry(fingerprint).or_insert(0);
            *count = count.saturating_add(1);
            if *count > MAX_REPEAT_CALLS {
                return (call, ToolCallDisposition::Rejected);
            }
            let class = catalog_tool_budget_class(&call.function.name)
                .unwrap_or(ToolBudgetClass::ExternalRead);
            let used = planned_by_class.entry(class).or_default();
            if *used >= loop_policy.tool_call_limit(class) {
                return (call, ToolCallDisposition::Rejected);
            }
            if is_discovery_call(call) {
                planned_discovery = planned_discovery.saturating_add(1);
            }
            planned_total = planned_total.saturating_add(1);
            *used = used.saturating_add(1);
            (call, ToolCallDisposition::Dispatched)
        })
        .collect()
}

fn tool_proposal_feedback_instruction(
    proposals: &[(&ToolCall, ToolCallDisposition)],
) -> LlmMessage {
    let feedback = proposals
        .iter()
        .map(|(call, disposition)| {
            let reason = match disposition {
                ToolCallDisposition::Rejected => "rejected_before_dispatch",
                ToolCallDisposition::Deferred => "deferred_for_feedback",
                ToolCallDisposition::Dispatched => "dispatched",
            };
            format!("{}:{reason}", call.function.name)
        })
        .collect::<Vec<_>>()
        .join(", ");
    LlmMessage {
        role: MessageRole::System,
        content: format!(
            "The previous tool proposal was not dispatched by the Host ({feedback}). It created no tool result and no external action. Correct the request using the exposed surface or answer from existing observations; do not claim that it ran."
        )
        .into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn register_safe_progress(progress: &mut HashSet<String>, result: &ToolCallResult) -> bool {
    let resource_progress = result.success
        && safe_progress_identities(&result.output)
            .into_iter()
            .map(|identity| format!("{}:{identity}", result.tool_name))
            .any(|identity| progress.insert(identity));
    let error_progress = (!result.success)
        .then_some(result.error.as_deref())
        .flatten()
        .filter(|error| !error.trim().is_empty())
        .is_some_and(|error| {
            progress.insert(format!(
                "{}:error:{}",
                result.tool_name,
                safe_tool_error_category(error)
            ))
        });
    resource_progress || error_progress
}

fn safe_tool_error_category(error: &str) -> &'static str {
    let lower = error.to_ascii_lowercase();
    if lower.contains("timeout") || lower.contains("timed out") {
        "timeout"
    } else if lower.contains("unauthorized") || lower.contains("credential") {
        "unauthorized"
    } else if lower.contains("forbidden") || lower.contains("permission") {
        "forbidden"
    } else if lower.contains("not_found") || lower.contains("not found") {
        "not_found"
    } else if lower.contains("rate") || lower.contains("quota") {
        "rate_limited"
    } else if lower.contains("invalid") || lower.contains("malformed") {
        "invalid_response"
    } else if lower.contains("unavailable") || lower.contains("connection") {
        "unavailable"
    } else if lower.contains("cancel") {
        "cancelled"
    } else {
        "other"
    }
}

fn initial_loop_budget_instruction(
    max_model_turns: u32,
    max_tool_calls: u32,
    max_local_tool_calls: u32,
    max_network_tool_calls: u32,
    max_external_read_tool_calls: u32,
    max_runtime_tool_calls: u32,
) -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: format!(
            "This Run uses one bounded observation-action loop. Budgets are maxima, not targets: modelTurns={max_model_turns}, totalTools={max_tool_calls}, localReads={max_local_tool_calls}, network={max_network_tool_calls}, externalReads={max_external_read_tool_calls}, runtime={max_runtime_tool_calls}. Choose only actions that can add information, inspect each returned observation before dependent actions, and finish as soon as the user goal is adequately supported. Do not reveal private reasoning."
        )
        .into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn safe_progress_identities(output: &serde_json::Value) -> Vec<String> {
    const SAFE_IDENTITY_KEYS: &[&str] = &[
        "resourceId",
        "resource_id",
        "resourceIds",
        "resource_ids",
        "canonicalUrl",
        "canonical_url",
        "canonicalUrls",
        "canonical_urls",
        "contentHash",
        "content_hash",
        "revision",
        "fileHash",
        "file_hash",
        "targetFileHash",
        "target_file_hash",
    ];

    fn collect_identity_values(value: &serde_json::Value, identities: &mut HashSet<String>) {
        match value {
            serde_json::Value::String(value) if !value.trim().is_empty() => {
                identities.insert(value.clone());
            }
            serde_json::Value::Number(value) => {
                identities.insert(value.to_string());
            }
            serde_json::Value::Array(values) => {
                for value in values {
                    collect_identity_values(value, identities);
                }
            }
            _ => {}
        }
    }

    fn collect(value: &serde_json::Value, identities: &mut HashSet<String>) {
        match value {
            serde_json::Value::Array(values) => {
                for value in values {
                    collect(value, identities);
                }
            }
            serde_json::Value::Object(values) => {
                for (key, value) in values {
                    if SAFE_IDENTITY_KEYS.contains(&key.as_str()) {
                        collect_identity_values(value, identities);
                    }
                    collect(value, identities);
                }
            }
            _ => {}
        }
    }

    let mut identities = HashSet::new();
    collect(output, &mut identities);
    let mut identities = identities.into_iter().collect::<Vec<_>>();
    identities.sort();
    identities
}

fn tool_surface_closed_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "Tool work is now closed because it has reached a bounded limit or produced no new safe resources in two complete rounds. Synthesize the best answer from the current transcript, state material uncertainty plainly, and do not request another business tool.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn missing_evidence_repair_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "The previous draft cannot be shown because this Run requires current evidence and no usable evidence has been registered. Use an available authorized read tool now to verify the answer. Do not invent facts or claim that you searched when no tool result is present.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn source_binding_repair_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "The previous draft cannot be shown because it does not bind its factual claims to this Run's sources. Revise it using only the existing transcript and cite the supplied Run-local source labels precisely. Do not call further business tools or invent a source.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn evidence_limited_outcome(
    content: String,
    model_turns: u32,
    tool_calls: u32,
    prompt_tokens: u32,
    completion_tokens: u32,
    total_tokens: u32,
) -> AgentToolLoopOutcome {
    AgentToolLoopOutcome {
        content,
        finish_reason: "evidence_limited".to_string(),
        final_submission: None,
        model_turns,
        tool_calls,
        prompt_tokens,
        completion_tokens,
        total_tokens,
    }
}

pub(crate) fn is_evidence_limited_response(content: &str) -> bool {
    content
        .trim_start()
        .starts_with(EVIDENCE_LIMITED_RESPONSE_PREFIX)
}

/// Update the already-compiled system message after the bounded compaction
/// turn succeeds. The final answer must see the same durable summary that was
/// just persisted; deferring it until the next Run made the compression call
/// consume budget without improving the active answer.
fn replace_conversation_memory_in_current_messages(
    messages: &mut [LlmMessage],
    prior_fragment: &str,
    updated_fragment: &str,
) {
    let Some(system_message) = messages
        .iter_mut()
        .find(|message| matches!(message.role, MessageRole::System))
    else {
        return;
    };
    let Some(content) = system_message.content.as_mut_str() else {
        return;
    };
    if content.contains(prior_fragment) {
        *content = content.replacen(prior_fragment, updated_fragment, 1);
    }
}

fn enforce_prompt_budget(
    messages: &[LlmMessage],
    tools: &[ToolSpec],
    budget: AgentModelTurnBudget,
) -> AppResult<()> {
    if budget
        .max_prompt_tokens
        .is_some_and(|limit| estimate_prompt_tokens(messages, tools) > limit)
    {
        return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
    }
    Ok(())
}

pub(crate) fn resolved_turn_usage(
    response: &GatewayResponse,
    messages: &[LlmMessage],
    tools: &[ToolSpec],
) -> (u32, u32, u32) {
    let prompt_tokens = nonzero_or_estimate(
        response.usage.prompt_tokens,
        estimate_prompt_tokens(messages, tools),
    );
    let completion_tokens = nonzero_or_estimate(
        response.usage.completion_tokens,
        estimate_completion_tokens(response),
    );
    let total_tokens = nonzero_or_estimate(
        response.usage.total_tokens,
        prompt_tokens.saturating_add(completion_tokens),
    );
    (prompt_tokens, completion_tokens, total_tokens)
}

fn nonzero_or_estimate(reported: u32, estimate: u32) -> u32 {
    if reported == 0 {
        estimate
    } else {
        reported
    }
}

fn estimate_prompt_tokens(messages: &[LlmMessage], tools: &[ToolSpec]) -> u32 {
    let message_tokens = messages.iter().fold(0_u32, |total, message| {
        total.saturating_add(estimate_tokens(&message.content.text_content()))
    });
    let tool_tokens = serde_json::to_string(tools)
        .ok()
        .map(|serialized| estimate_tokens(&serialized))
        .unwrap_or_default();
    message_tokens.saturating_add(tool_tokens)
}

fn estimate_completion_tokens(response: &GatewayResponse) -> u32 {
    let content_tokens = response
        .content
        .as_deref()
        .map(estimate_tokens)
        .unwrap_or_default();
    let tool_tokens = if response.tool_calls.is_empty() {
        0
    } else {
        serde_json::to_string(&response.tool_calls)
            .ok()
            .map(|serialized| estimate_tokens(&serialized))
            .unwrap_or_default()
    };
    let reasoning_tokens = response
        .reasoning_content
        .as_deref()
        .map(estimate_tokens)
        .unwrap_or_default();
    content_tokens
        .saturating_add(tool_tokens)
        .saturating_add(reasoning_tokens)
}

fn estimate_tokens(value: &str) -> u32 {
    crate::ai_runtime::text_support::estimate_tokens(value)
        .try_into()
        .unwrap_or(u32::MAX)
}

fn can_recover_visible_stream_error(error: &AppError, visible_draft: Option<&str>) -> bool {
    visible_draft.is_some_and(|draft| !draft.trim().is_empty())
        && matches!(error, AppError::StreamInterrupted(_))
}

fn incomplete_answer_continuation_instruction() -> LlmMessage {
    LlmMessage {
        role: MessageRole::System,
        content: "The prior answer stopped before it was complete. Continue it now by outputting only the missing continuation. Do not repeat, replace, title, summarize, cite, or call tools.".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn append_final_answer_continuation(draft: String, continuation: String) -> AppResult<String> {
    if continuation.trim().is_empty() || continuation.trim_start().starts_with(draft.trim()) {
        return Err(AppError::run(SafeRunErrorCode::IncompleteOutput));
    }
    let separator =
        if draft.ends_with(char::is_whitespace) || continuation.starts_with(char::is_whitespace) {
            ""
        } else {
            "\n\n"
        };
    Ok(format!("{draft}{separator}{continuation}"))
}

fn final_answer_submission(
    response: &GatewayResponse,
    allowed_tools: &HashSet<&str>,
) -> AppResult<Option<FinalAnswerSubmission>> {
    let has_final_tool = response
        .tool_calls
        .iter()
        .any(|call| call.function.name == FINAL_ANSWER_TOOL_NAME);
    if !has_final_tool {
        return Ok(None);
    }
    if response.tool_calls.len() != 1
        || !response
            .content
            .as_deref()
            .unwrap_or_default()
            .trim()
            .is_empty()
        || !allowed_tools.contains(FINAL_ANSWER_TOOL_NAME)
    {
        return Err(AppError::run(SafeRunErrorCode::FinalSubmissionInvalid));
    }
    FinalAnswerSubmission::from_tool_call(&response.tool_calls[0]).map(Some)
}

fn ensure_run_not_cancelled(run_id: &str) -> AppResult<()> {
    if crate::ai_runtime::model_gateway::is_abort_requested(run_id) {
        Err(AppError::run(SafeRunErrorCode::Cancelled))
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

fn tool_result_message(
    call: &ToolCall,
    result: &ToolCallResult,
    remaining_model_turns: u32,
    remaining_tool_calls: u32,
    remaining_category_calls: u32,
) -> (LlmMessage, bool) {
    let payload = serde_json::json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
        "loopObservation": {
            "remainingModelTurns": remaining_model_turns,
            "remainingToolCalls": remaining_tool_calls,
            "remainingCategoryCalls": remaining_category_calls,
        },
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
    if truncated && matches!(call.function.name.as_str(), "web_search" | "web_fetch") {
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
    if matches!(tool_name, "web_search" | "web_fetch") {
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

fn is_discovery_call(call: &ToolCall) -> bool {
    let Some(entry) = crate::ai_runtime::tool_catalog::catalog_find(&call.function.name) else {
        return false;
    };
    if !entry.is_discovery() {
        return false;
    }
    if call.function.name != "web_search" {
        return true;
    }
    serde_json::from_str::<serde_json::Value>(&call.function.arguments)
        .ok()
        .and_then(|arguments| arguments.get("urls").cloned())
        .and_then(|urls| urls.as_array().cloned())
        .is_none_or(|urls| urls.is_empty())
}

fn deferred_result(call: &ToolCall) -> ToolCallResult {
    ToolCallResult {
        tool_name: call.function.name.clone(),
        success: true,
        output: serde_json::json!({
            "status": "deferred_for_feedback",
            "reason": "discovery_batch_limit",
        }),
        duration_ms: 0,
        tokens_used: None,
        error: None,
    }
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
