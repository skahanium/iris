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
use crate::ai_runtime::run_context::{
    conversation_memory_prompt_fragment, CONVERSATION_HISTORY_COVERAGE_WARNING,
};
use crate::ai_runtime::run_contract::RunBudgetPolicy;
use crate::ai_runtime::run_contract::SafeRunErrorCode;
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::tool_catalog::{catalog_tool_budget_class, ToolBudgetClass};
use crate::ai_runtime::{LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

#[path = "agent_tool_loop/observations.rs"]
mod observations;
pub(crate) use observations::safe_progress_identities;
use observations::ExecutionRecord;

#[path = "agent_tool_loop/loop_projection.rs"]
mod loop_projection;
pub(crate) use loop_projection::{
    observation_failure_type, LoopProjection, NEXT_ACTION_CONTINUE, NEXT_ACTION_SYNTHESIZE,
};

#[path = "agent_tool_loop/payload_fit.rs"]
mod payload_fit;
pub(crate) use payload_fit::{prepare_tool_result, tool_result_message};

#[path = "agent_tool_loop/prompt_budget.rs"]
mod prompt_budget;
#[cfg(test)]
pub(crate) use payload_fit::fit_tool_payload;
#[cfg(test)]
pub(crate) use prompt_budget::compact_tool_observations;

const MAX_REPEAT_CALLS: u32 = 2;
const MAX_DISCOVERY_CALLS_PER_MODEL_TURN: u32 = 2;
const MAX_TOOL_RESULT_CHARS: usize = 8_000;
/// Internal control-flow signal: a complete immutable change set was persisted
/// and the Run must wait for its single user confirmation.
#[path = "agent_tool_loop/prompt_assembly.rs"]
pub(crate) mod prompt_assembly;
pub(crate) use prompt_assembly::*;

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
/// contract. The host accepts it as a single, source-free question even after
/// a bounded Host bootstrap: a search cannot supply a user constraint that is
/// genuinely missing. All other WebRequired output still needs current-Run
/// evidence.
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

/// Which producer owns the terminal assistant body of one bounded Run, plus the
/// outcome envelope that carries it.
#[path = "agent_tool_loop/terminal.rs"]
mod terminal;
#[cfg(test)]
pub(crate) use terminal::evidence_limited_outcome as evidence_limited_outcome_for_test;
use terminal::{evidence_limited_outcome, model_terminal_type};
pub(crate) use terminal::{AgentTerminalType, AgentToolLoopOutcome, EVIDENCE_LIMITED_RESPONSE};

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

/// What one Host bootstrap added to the Run's own accounting.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HostWebBootstrapAccounting {
    tool_calls: u32,
    network_tool_calls: u32,
}

/// Result of the single Host bootstrap handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct HostWebBootstrapOutcome {
    /// The executor's own observation state after the handshake.
    observation_performed: bool,
    /// Present only when a dispatch really happened; the single bootstrap is
    /// consumed exactly then.
    applied: Option<HostWebBootstrapAccounting>,
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
    /// Recheck access before replaying a bounded historical observation. This
    /// performs no tool action; executors without this gate fail closed.
    fn observation_replay_rejection(
        &self,
        _run_id: &str,
        _call: &ToolCall,
        _step: u32,
    ) -> AppResult<Option<&'static str>> {
        Ok(Some("observation_replay_unavailable"))
    }
    /// Accept only Host-created, content-free counters and enum codes.
    fn record_tool_loop_diagnostic(&self, _event: serde_json::Value) {}

    /// True when this parsed name is a Run-surface mapping, not a catalog tool.
    fn mapped_tool_name(&self, _parsed_name: &str) -> bool {
        false
    }

    /// Read-only, Run-specific validation after JSON/schema validation and
    /// before provider binding, execution counters or tool lifecycle events.
    fn proposal_rejection_reason(&self, _call: &ToolCall) -> AppResult<Option<&'static str>> {
        Ok(None)
    }

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

    /// Re-evaluate whether the exact short-term history selected for this Run
    /// still leaves an uncovered range after a successful compaction. Generic
    /// executors have no session context and therefore conservatively report
    /// a complete block; the normal executor overrides this with its frozen
    /// context snapshot.
    fn conversation_history_coverage_is_incomplete(
        &self,
        _memory: &crate::ai_runtime::conversation_memory::ConversationMemory,
    ) -> bool {
        false
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

    fn requires_web_observation(&self) -> bool {
        self.requires_web_evidence()
    }

    /// Executors may distinguish a real observation from a known capability
    /// block before any search service could be reached.
    fn web_observation_performed(&self) -> Option<bool> {
        None
    }

    fn web_capability_blocked(&self) -> bool {
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

    /// Project the Host's exact counters onto what the model may know.
    ///
    /// The Host keeps the real numbers; the model only learns whether another
    /// action is affordable and which action to take next. Publishing exact
    /// remaining quotas made the model narrate internal budget to the user and
    /// treat a numeric allowance as a target.
    fn loop_projection(
        &self,
        remaining_model_turns: u32,
        tool_calls: u32,
        tool_calls_by_class: &HashMap<ToolBudgetClass, u32>,
    ) -> LoopProjection {
        let category_remaining = [
            ToolBudgetClass::Local,
            ToolBudgetClass::Network,
            ToolBudgetClass::ExternalRead,
            ToolBudgetClass::Runtime,
            ToolBudgetClass::ConfirmedChange,
        ]
        .into_iter()
        .all(|class| {
            let used = tool_calls_by_class.get(&class).copied().unwrap_or_default();
            used < self.tool_call_limit(class)
        });
        LoopProjection::for_observation(
            remaining_model_turns > 1 && tool_calls < self.max_tool_calls && category_remaining,
            None,
            false,
        )
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

    /// Dispatch the Host's one bounded Web observation, if the frozen
    /// contract still owes one.
    ///
    /// Returns the executor's observation state plus, when a dispatch really
    /// happened, the counters the caller must add to its own Host accounting. A
    /// model that answers without observing and a model that proposes tools
    /// before observing reach the same handshake, so neither can publish an
    /// unsupported answer and neither can trigger a second bootstrap.
    #[allow(clippy::too_many_arguments)]
    async fn dispatch_host_web_bootstrap(
        &self,
        executor: &impl ToolLoopExecutor,
        run_id: &str,
        tool_calls: u32,
        tool_calls_by_class: &HashMap<ToolBudgetClass, u32>,
        telemetry: Option<&crate::ai_runtime::agent_capacity_eval::EvaluationTelemetryTap>,
        observer: &mut dyn StreamEventObserver,
        messages: &mut Vec<LlmMessage>,
        observation_position: usize,
    ) -> AppResult<HostWebBootstrapOutcome> {
        let remaining = self.max_tool_calls.saturating_sub(tool_calls);
        observer.on_tools_starting()?;
        let dispatched = executor
            .bootstrap_required_web_observation(run_id, remaining)
            .await?;
        let Some(bootstrap) = dispatched else {
            // An executor that owns no bootstrap returns nothing at all. The
            // model keeps its own tool-enabled repair instead.
            observer.on_tools_finished()?;
            return Ok(HostWebBootstrapOutcome {
                observation_performed: executor.web_observation_performed().unwrap_or(false),
                applied: None,
            });
        };
        let network_used = tool_calls_by_class
            .get(&ToolBudgetClass::Network)
            .copied()
            .unwrap_or_default();
        if bootstrap.tool_calls > remaining
            || bootstrap.network_tool_calls > bootstrap.tool_calls
            || network_used.saturating_add(bootstrap.network_tool_calls)
                > self.tool_call_limit(ToolBudgetClass::Network)
        {
            return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
        }
        if let Some(telemetry) = telemetry {
            telemetry.record_bootstrap_web_tool_calls(bootstrap.tool_calls);
        }
        let observation_performed = executor
            .web_observation_performed()
            .unwrap_or(bootstrap.tool_calls > 0);
        messages.insert(
            observation_position,
            LlmMessage {
                role: MessageRole::System,
                content: bootstrap.observation.into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            },
        );
        observer.on_tools_finished()?;
        Ok(HostWebBootstrapOutcome {
            observation_performed,
            applied: Some(HostWebBootstrapAccounting {
                tool_calls: tool_calls.saturating_add(bootstrap.tool_calls),
                network_tool_calls: network_used.saturating_add(bootstrap.network_tool_calls),
            }),
        })
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
        messages.insert(loop_state_position, initial_loop_budget_instruction());
        let mut model_turns = 0_u32;
        let mut tool_calls = 0;
        let mut prompt_tokens = 0_u32;
        let mut completion_tokens = 0_u32;
        let mut total_tokens = 0_u32;
        let mut fingerprints = HashMap::<String, u32>::new();
        let mut executions = HashMap::<String, ExecutionRecord>::new();
        let mut tool_calls_by_class = HashMap::<ToolBudgetClass, u32>::new();
        let mut observed_progress = HashSet::<String>::new();
        let mut final_submission_repair_used = false;
        let mut missing_evidence_repair_used = false;
        let mut source_binding_repair_used = false;
        let mut source_binding_repair_required = false;
        let mut incomplete_final_answer_repair_used = false;
        let mut incomplete_final_draft = None::<String>;
        // The Host can withhold a model draft and request one bounded repair or
        // a forced synthesis turn. A publishable answer reached that way is
        // still Provider prose, so it records only how the turn was reached.
        let mut model_answer_repaired = false;
        // Distinguish an answer that skipped a required Web lookup from one
        // produced after an attempted lookup yielded no usable evidence. The
        // former receives one tool-enabled repair; asking the model to repair
        // the latter cannot create a source and used to turn an empty or
        // failed search into a provider/internal failure.
        let mut web_observation_performed = executor
            .web_observation_performed()
            .unwrap_or_else(|| executor.has_web_evidence());
        // The Host bootstrap is a single bounded action: it is dispatched at
        // most once per Run, from the first boundary that lacks the observation
        // the frozen contract requires.
        let mut host_web_bootstrap_dispatched = false;
        let mut no_progress_rounds = 0_u8;
        let mut rejected_rounds = 0_u8;
        let mut failed_service_rounds = 0_u8;
        let mut synthesis_required = false;
        let synthesis_tools = tools
            .iter()
            .filter(|tool| tool.name == FINAL_ANSWER_TOOL_NAME)
            .cloned()
            .collect::<Vec<_>>();
        let requires_factual_completion =
            executor.requires_web_evidence() || executor.requires_external_evidence();

        let outcome = async {
        ensure_run_not_cancelled(run_id)?;
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
                            &conversation_memory_prompt_fragment(
                                &updated_memory,
                                executor
                                    .conversation_history_coverage_is_incomplete(&updated_memory),
                            ),
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

        ensure_run_not_cancelled(run_id)?;
        // A required-Web Run has a deterministic minimum observation: one
        // search and one bounded fetch batch. The frozen contract must be able
        // to afford it before the Run starts any model or network side effect.
        // The dispatch itself happens later, at the first boundary that lacks
        // the observation, so the model reads the question and chooses its own
        // queries first.
        if executor.requires_web_observation() {
            let bootstrap_actions = executor.required_web_bootstrap_action_count();
            let remaining = self.max_tool_calls.saturating_sub(tool_calls);
            let remaining_network = self
                .tool_call_limit(ToolBudgetClass::Network)
                .saturating_sub(
                    tool_calls_by_class
                        .get(&ToolBudgetClass::Network)
                        .copied()
                        .unwrap_or_default(),
                );
            if remaining < bootstrap_actions || remaining_network < bootstrap_actions {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
        }

        // Where a Host observation belongs in the transcript: directly before
        // the current user message, so it is context for the turn about to run
        // rather than a new instruction.
        let observation_position = messages
            .iter()
            .rposition(|message| matches!(message.role, MessageRole::User))
            .unwrap_or(messages.len());
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
            // R12: remaining at or below the completion reserve is an
            // output-cap envelope. Withhold business tools and give the
            // remaining tokens to the expression turn; do not inject a
            // recipe closure instruction.
            if !synthesis_required
                && !source_binding_repair_required
                && incomplete_final_draft.is_none()
                && !is_final_model_turn
                && remaining_completion_tokens
                    .is_some_and(|remaining| remaining <= synthesis_output_reserve)
            {
                synthesis_required = true;
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
                    || is_final_model_turn
                    || remaining <= synthesis_output_reserve;
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
            let compacted = prompt_budget::compact_tool_observations(&mut messages, active_tools, model_turn_budget);
            for record in executions.values_mut() {
                if compacted.contains(&record.call_id) { record.compacted = true; }
            }
            if enforce_prompt_budget(&messages, active_tools, model_turn_budget).is_err() {
                if tool_calls == 0 { return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit)); }
                executor.record_tool_loop_diagnostic(serde_json::json!({"event":"projection_limit","reason":"prompt_budget"}));
                return Ok(evidence_limited_outcome(executor.evidence_limited_response(),model_turns,tool_calls,prompt_tokens,completion_tokens,total_tokens));
            }
            if model_turn_budget.max_completion_tokens == Some(0) {
                return Err(AppError::run(SafeRunErrorCode::ToolLoopLimit));
            }
            model_turns += 1;
            executor.record_tool_loop_diagnostic(serde_json::json!({"event":"model", "modelTurns":model_turns}));
            if let Some(usage) = usage.as_deref_mut() {
                usage.model_turns = model_turns;
            }
            let model_started_at = std::time::Instant::now();
            if executor.requires_web_observation() && !web_observation_performed {
                observer.on_tools_starting()?;
            }
            let provider_turn = provider.answer_turn(
                provider_run_id,
                &messages,
                active_tools,
                model_turn_budget,
                observer,
            );
            let mut response = match provider_turn.await {
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
            if !crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity::may_execute_tool_calls(
                &response.finish_reason,
            ) {
                response.tool_calls.clear();
            }

            if incomplete_final_draft.is_some() && !response.tool_calls.is_empty() {
                // The continuation contract was broken. Publish the Host-authored
                // bounded limitation instead of failing the Run: a terminal error
                // reaches the user as nothing at all.
                return Ok(evidence_limited_outcome(
                    executor.evidence_limited_response(),
                    model_turns,
                    tool_calls,
                    prompt_tokens,
                    completion_tokens,
                    total_tokens,
                ));
            }

            if response.tool_calls.is_empty() {
                let response_content = response.content.unwrap_or_default();
                let content = match incomplete_final_draft.take() {
                    Some(draft) => match append_final_answer_continuation(draft, response_content) {
                        Ok(content) => content,
                        // A continuation that repeats or omits the draft cannot be
                        // published as a finished answer. Fall back to the bounded
                        // limitation rather than a terminal error.
                        Err(_) => {
                            return Ok(evidence_limited_outcome(
                                executor.evidence_limited_response(),
                                model_turns,
                                tool_calls,
                                prompt_tokens,
                                completion_tokens,
                                total_tokens,
                            ))
                        }
                    },
                    None => response_content,
                };
                if content.trim().is_empty()
                    && crate::ai_runtime::final_answer_integrity::FinalAnswerIntegrity::has_normal_finish_reason(
                        &response.finish_reason,
                    )
                {
                    return Err(AppError::msg("agent_run_invalid_model_response"));
                }
                // A necessary clarification remains a normal completion even
                // when the Host has already gathered a bounded observation.
                // Bootstrap dispatches are not evidence that the missing user
                // constraint was available, and must not turn a question into
                // a misleading evidence-limited refusal.
                let natural_clarification = is_natural_clarification(&content);
                // The model reached a publishable body without the observation
                // the frozen contract requires, and no Web attempt has happened
                // yet to blame. That is where the single Host bootstrap runs:
                // the draft stays unpublished, the Host dispatches the minimum
                // observation, and the same loop continues with it in context.
                if executor.requires_web_observation()
                    && !web_observation_performed
                    && !host_web_bootstrap_dispatched
                {
                    let bootstrap = self
                        .dispatch_host_web_bootstrap(
                            executor,
                            run_id,
                            tool_calls,
                            &tool_calls_by_class,
                            telemetry,
                            observer,
                            &mut messages,
                            observation_position,
                        )
                        .await?;
                    web_observation_performed = bootstrap.observation_performed;
                    if let Some(applied) = bootstrap.applied {
                        host_web_bootstrap_dispatched = true;
                        tool_calls = applied.tool_calls;
                        tool_calls_by_class
                            .insert(ToolBudgetClass::Network, applied.network_tool_calls);
                        if let Some(usage) = usage.as_deref_mut() {
                            usage.tool_calls = tool_calls;
                        }
                    }
                }
                if (executor.requires_web_evidence() && !executor.has_web_evidence()
                    || executor.requires_web_observation()
                        && !web_observation_performed
                        && !executor.web_capability_blocked())
                    && !natural_clarification
                {
                    if synthesis_required
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
                    model_answer_repaired = true;
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
                    model_answer_repaired = true;
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
                        // The strict submission protocol was offered twice and the
                        // model answered in prose both times. A Run with a strict
                        // current-evidence contract must not publish unsourced
                        // prose, so close with the Host-authored limitation
                        // instead of a terminal error that shows nothing.
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
                    model_answer_repaired = true;
                    synthesis_required = true;
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
                        // An incomplete answer is not publishable as an answer, but
                        // the Run must still end with something the user can read.
                        return Ok(evidence_limited_outcome(
                            executor.evidence_limited_response(),
                            model_turns,
                            tool_calls,
                            prompt_tokens,
                            completion_tokens,
                            total_tokens,
                        ));
                    }
                    incomplete_final_answer_repair_used = true;
                    model_answer_repaired = true;
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
                let needs_binding = executor.requires_natural_source_binding();
                let binding_valid = !needs_binding || executor.natural_source_binding_is_valid(&content);
                if needs_binding
                    && !natural_clarification
                    && !binding_valid
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
                    model_answer_repaired = true;
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
                    terminal: model_terminal_type(model_answer_repaired),
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
                        terminal: model_terminal_type(model_answer_repaired),
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
                    self.loop_projection(self.max_model_turns.saturating_sub(model_turns), tool_calls, &tool_calls_by_class),
                )?;
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

            // A model that proposes tools instead of answering still owes the
            // Run its minimum observation. The same single bootstrap runs
            // before the first dispatch round, so the model chooses its queries
            // with the Host's preliminary result already in context.
            if executor.requires_web_observation()
                && !web_observation_performed
                && !host_web_bootstrap_dispatched
            {
                let bootstrap = self
                    .dispatch_host_web_bootstrap(
                        executor,
                        run_id,
                        tool_calls,
                        &tool_calls_by_class,
                        telemetry,
                        observer,
                        &mut messages,
                        observation_position,
                    )
                    .await?;
                web_observation_performed = bootstrap.observation_performed;
                if let Some(applied) = bootstrap.applied {
                    host_web_bootstrap_dispatched = true;
                    tool_calls = applied.tool_calls;
                    tool_calls_by_class
                        .insert(ToolBudgetClass::Network, applied.network_tool_calls);
                    if let Some(usage) = usage.as_deref_mut() {
                        usage.tool_calls = tool_calls;
                    }
                }
            }

            let mut discovery_calls_this_turn = 0_u32;
            let mut proposal_dispositions = plan_tool_proposals(
                &response.tool_calls,
                &active_allowed_tools,
                active_tools,
                discovery_calls_this_turn,
                tool_calls,
                &tool_calls_by_class,
                &executions,
                &fingerprints,
                self,
            );
            for (call, disposition) in &mut proposal_dispositions {
                if *disposition == ToolCallDisposition::Dispatched {
                    if let Some(reason) = executor.proposal_rejection_reason(call)? {
                        *disposition = ToolCallDisposition::Rejected(reason);
                    }
                }
            }
            for (call, disposition) in &proposal_dispositions {
                let reason = match disposition {
                    ToolCallDisposition::Rejected(reason) => *reason,
                    ToolCallDisposition::Deferred => "deferred_for_feedback",
                    ToolCallDisposition::Dispatched => "accepted",
                };
                let surface: Vec<&str> = active_allowed_tools.iter().copied().collect();
                let declared: Vec<&str> =
                    active_tools.iter().map(|tool| tool.name.as_str()).collect();
                executor.record_tool_loop_diagnostic(
                    crate::ai_runtime::tool_name_origin::proposal_payload(
                        &call.function.name,
                        &surface,
                        &declared,
                        executor.mapped_tool_name(&call.function.name),
                        reason,
                        model_turns,
                    ),
                );
            }
            if proposal_dispositions
                .iter()
                .all(|(_, disposition)| *disposition != ToolCallDisposition::Dispatched)
            {
                provider.on_tool_proposals_not_dispatched(provider_run_id)?;
                let envelope_last_turn = model_turns.saturating_add(1) >= self.max_model_turns
                    || tool_calls >= self.max_tool_calls;
                let target_action = if envelope_last_turn {
                    NEXT_ACTION_SYNTHESIZE
                } else {
                    NEXT_ACTION_CONTINUE
                };
                messages.push(tool_proposal_feedback_instruction(
                    &proposal_dispositions,
                    active_tools,
                    target_action,
                ));
                rejected_rounds = rejected_rounds.saturating_add(1);
                executor.record_tool_loop_diagnostic(serde_json::json!({"event":"repair", "round":rejected_rounds}));
                if envelope_last_turn {
                    synthesis_required = true;
                }
                continue;
            }
            rejected_rounds = 0;

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
            let mut round_had_success = false;
            for call in &dispatched_response.tool_calls {
                let mut recorded_execution = false;
                let mut replayed = false;
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
                    if executions.get(&fingerprint).is_some_and(ExecutionRecord::blocks_execution) {
                        rejected_result(call, "tool_call_already_succeeded")
                    } else {
                        let count = fingerprints.entry(fingerprint.clone()).or_insert(0);
                        *count += 1;
                        if *count > MAX_REPEAT_CALLS && !executions.get(&tool_fingerprint(call)).is_some_and(|record|record.compacted) {
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
                                let replay = executions.get(&fingerprint).filter(|record|record.compacted);
                                let execution = if let Some(record) = replay {
                                    if let Some(reason) = executor.observation_replay_rejection(run_id,call,tool_calls)? {
                                        Ok(rejected_result(call,reason))
                                    } else {
                                        replayed = true;
                                        executor.record_tool_loop_diagnostic(serde_json::json!({"event":"observation_replay","tool":crate::ai_runtime::tool_catalog::catalog_find(&call.function.name).map_or("external", |entry| entry.name),"providerAttempts":0}));
                                        Ok(record.result.clone())
                                    }
                                } else {
                                    recorded_execution = true;
                                    executor.execute(run_id,call,tool_calls).await
                                };
                                let result = match execution {
                                    Ok(result) => result,
                                    Err(error) => {
                                        executor.record_tool_loop_diagnostic(serde_json::json!({"event":"tool_error", "tool":crate::ai_runtime::tool_catalog::catalog_find(&call.function.name).map_or("unknown", |entry| entry.name), "reason":SafeRunErrorCode::from_app_error(&error).as_str()}));
                                        if SafeRunErrorCode::from_app_error(&error) == SafeRunErrorCode::ToolLoopLimit {
                                            observer.on_tools_finished()?;
                                            return Ok(evidence_limited_outcome(executor.evidence_limited_response(),model_turns,tool_calls,prompt_tokens,completion_tokens,total_tokens));
                                        }
                                        return Err(error);
                                    }
                                };
                                if matches!(
                                    call.function.name.as_str(),
                                    "web_search" | "web.search" | "web_fetch"
                                ) {
                                    web_observation_performed |= executor.web_observation_performed().unwrap_or(true);
                                }
                                round_had_success |= result.success;
                                result
                            }
                        }
                    }
                };
                let projection = self
                    .loop_projection(
                        self.max_model_turns.saturating_sub(model_turns),
                        tool_calls,
                        &tool_calls_by_class,
                    )
                    .with_failure_type(observation_failure_type(&result));
                let projected = tool_result_message(call, &result, projection);
                let (mut message, truncated) = match projected {
                    Ok(projected) => projected,
                    Err(error) if SafeRunErrorCode::from_app_error(&error) == SafeRunErrorCode::ToolLoopLimit => {
                        executor.record_tool_loop_diagnostic(serde_json::json!({"event":"projection_limit","reason":"tool_payload"}));
                        observer.on_tools_finished()?;
                        return Ok(evidence_limited_outcome(executor.evidence_limited_response(),model_turns,tool_calls,prompt_tokens,completion_tokens,total_tokens));
                    }
                    Err(error) => return Err(error),
                };
                if truncated {
                    if let Some(telemetry) = telemetry {
                        telemetry.record_truncation(
                            crate::ai_runtime::agent_capacity_eval::TruncationOutcome::ToolResultTruncated,
                        );
                    }
                }
                if recorded_execution || replayed {
                    let mut payload:serde_json::Value = serde_json::from_str(&message.content.text_content())?;
                    if replayed { payload["loopObservation"]["historicalObservation"] = serde_json::json!(true); }
                    let mut bounded = result.clone();
                    bounded.output = payload["output"].clone();
                    if !replayed { round_made_progress |= register_safe_progress(&mut observed_progress,&bounded); }
                    executions.insert(tool_fingerprint(call),ExecutionRecord {result:bounded,call_id:call.id.clone(),compacted:false});
                    message.content = serde_json::to_string(&payload)?.into();
                }
                messages.push(message);
            }
            if !non_dispatched.is_empty() {
                messages.push(tool_proposal_feedback_instruction(
                    &non_dispatched,
                    active_tools,
                    NEXT_ACTION_CONTINUE,
                ));
            }
            observer.on_tools_finished()?;
            if round_had_success {
                failed_service_rounds = 0;
                no_progress_rounds = if round_made_progress { 0 } else { no_progress_rounds.saturating_add(1) };
            } else {
                failed_service_rounds = failed_service_rounds.saturating_add(1);
            }
            executor.record_tool_loop_diagnostic(serde_json::json!({"event":"progress", "noProgressRounds":no_progress_rounds, "failedServiceRounds":failed_service_rounds}));
            // Envelope only: last model turn or exhausted tool-call ledger.
            // Recipe counters (`no_progress_rounds`, `failed_service_rounds`)
            // do not close the surface (R11, R12).
            let final_turn_must_be_reserved = model_turns.saturating_add(1) >= self.max_model_turns;
            if tool_calls >= self.max_tool_calls || final_turn_must_be_reserved {
                synthesis_required = true;
            }
        }

        if let Some(telemetry) = telemetry {
            telemetry.record_budget(
                crate::ai_runtime::agent_capacity_eval::BudgetOutcome::ModelTurnsExhausted,
            );
        }
        // The Run exhausted its model turns without a publishable answer.
        // Previously this was a terminal error and the user saw nothing at all.
        // It now closes with the same Host-authored bounded limitation, while
        // the real cause stays in the bounded diagnostics so an operator can
        // still tell turn exhaustion from an ordinary answer.
        executor.record_tool_loop_diagnostic(serde_json::json!({
            "event": "exhausted",
            "cause": if incomplete_final_draft.is_some() {
                "agent_run_incomplete_output"
            } else {
                "agent_run_tool_loop_limit"
            },
            "modelTurns": model_turns,
            "toolCalls": tool_calls,
            "rejectedRounds": rejected_rounds,
        }));
        Ok(evidence_limited_outcome(
            executor.evidence_limited_response(),
            model_turns,
            tool_calls,
            prompt_tokens,
            completion_tokens,
            total_tokens,
        ))
        }.await;
        let exit_reason = match &outcome {
            // The typed terminal decides this, not the body text and not a
            // synthetic Provider finish reason.
            Ok(result) if result.terminal.is_host_authored() => "evidence_limited",
            Ok(result) if is_natural_clarification(&result.content) => "clarification",
            Ok(_) if !web_observation_performed && executor.web_capability_blocked() => {
                "tool_unavailable"
            }
            Ok(result) => result.terminal.as_str(),
            Err(error) => SafeRunErrorCode::from_app_error(error).as_str(),
        };
        executor.record_tool_loop_diagnostic(serde_json::json!({"event":"exit", "reason":exit_reason, "modelTurns":model_turns, "toolCalls":tool_calls, "observationPerformed":web_observation_performed, "capabilityBlocked":executor.web_capability_blocked()}));
        outcome
    }
}

/// The Host's pre-dispatch classification is deliberately narrow. It does not
/// replace executor authorization; it only prevents a completely rejected or
/// deferred model proposal from becoming a fake provider-bound tool turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ToolCallDisposition {
    Rejected(&'static str),
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
    active_tools: &[ToolSpec],
    discovery_calls_this_turn: u32,
    tool_calls: u32,
    tool_calls_by_class: &HashMap<ToolBudgetClass, u32>,
    executions: &HashMap<String, ExecutionRecord>,
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
            let rejection = if !active_allowed_tools.contains(call.function.name.as_str()) {
                Some("tool_not_in_run_surface")
            } else if call.id.trim().is_empty() {
                Some("missing_call_id")
            } else if !valid_call_arguments(call) && !executor_owns_invalid_arguments {
                Some("invalid_arguments_json")
            } else if !executor_owns_invalid_arguments
                && active_tools
                    .iter()
                    .find(|tool| tool.name == call.function.name)
                    .is_some_and(|tool| {
                        let args =
                            serde_json::from_str::<serde_json::Value>(&call.function.arguments)
                                .unwrap_or_default();
                        matches!(
                            crate::ai_runtime::guardrails::verify_tool_args(
                                &call.function.name,
                                &args,
                                &tool.input_schema
                            ),
                            crate::ai_runtime::guardrails::GuardResult::Block { .. }
                        )
                    })
            {
                Some("arguments_schema_mismatch")
            } else if planned_total >= loop_policy.max_tool_calls {
                Some("tool_call_budget_exhausted")
            } else {
                None
            };
            if let Some(reason) = rejection {
                return (call, ToolCallDisposition::Rejected(reason));
            }
            let fingerprint = tool_fingerprint(call);
            if executions
                .get(&fingerprint)
                .is_some_and(ExecutionRecord::blocks_execution)
            {
                return (
                    call,
                    ToolCallDisposition::Rejected("tool_call_already_succeeded"),
                );
            }
            if is_discovery_call(call) && planned_discovery >= MAX_DISCOVERY_CALLS_PER_MODEL_TURN {
                return (call, ToolCallDisposition::Deferred);
            }
            let count = planned_fingerprints.entry(fingerprint).or_insert(0);
            *count = count.saturating_add(1);
            if *count > MAX_REPEAT_CALLS
                && !executions
                    .get(&tool_fingerprint(call))
                    .is_some_and(|record| record.compacted)
            {
                return (call, ToolCallDisposition::Rejected("tool_call_repeated"));
            }
            let class = catalog_tool_budget_class(&call.function.name)
                .unwrap_or(ToolBudgetClass::ExternalRead);
            let used = planned_by_class.entry(class).or_default();
            if *used >= loop_policy.tool_call_limit(class) {
                return (
                    call,
                    ToolCallDisposition::Rejected("tool_category_budget_exhausted"),
                );
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
    tools: &[ToolSpec],
    target_action: &str,
) -> LlmMessage {
    let surface = tools
        .iter()
        .map(|tool| {
            serde_json::json!({
                "name": tool.name, "parameters": tool.input_schema,
            })
        })
        .collect::<Vec<_>>();
    let surface = serde_json::to_string(&surface).unwrap_or_default();
    let feedback = proposals
        .iter()
        .map(|(call, disposition)| {
            let reason = match disposition {
                ToolCallDisposition::Rejected(reason) => reason,
                ToolCallDisposition::Deferred => "deferred_for_feedback",
                ToolCallDisposition::Dispatched => "dispatched",
            };
            if matches!(disposition, ToolCallDisposition::Rejected(_)) {
                format!("{}:rejected_before_dispatch({reason})", call.function.name)
            } else {
                format!("{}:{reason}", call.function.name)
            }
        })
        .collect::<Vec<_>>()
        .join(", ");
    LlmMessage {
        role: MessageRole::System,
        content: format!(
            "The previous tool proposal was not dispatched by the Host ({feedback}). It created no tool result and no external action. Correct the request using the exposed surface; this is not a failed search and it consumed no execution allowance. Next action: {target_action}. Allowed tool names and parameter schemas: {surface}. Supply a nonempty call id and JSON object matching the schema. Do not claim the rejected action ran."
        )
        .into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }
}

fn register_safe_progress(progress: &mut HashSet<String>, result: &ToolCallResult) -> bool {
    let mut added = false;
    if result.success {
        for identity in safe_progress_identities(&result.output) {
            added |= progress.insert(format!("{}:{identity}", result.tool_name));
        }
    }
    added
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
