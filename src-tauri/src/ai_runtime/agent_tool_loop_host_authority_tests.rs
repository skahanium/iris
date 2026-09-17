//! Host authority over the next action: the negative cases for `R11` / `R12` /
//! `N21` / `G06`.
//!
//! The Host may enforce the envelope (still authorized, not cancelled, ledger not
//! exhausted). It must not also act as a second planner by injecting
//! `tool_surface_closed_instruction` because a *recipe* counter fired — two
//! rejected proposals, two rounds without a new safe resource, or two failed
//! service rounds. `R12` classified the completion reserve as an output-cap
//! envelope: a reserve turn may withhold business tools and must still carry the
//! remaining tokens, but must not inject the recipe closure instruction.
//!
//! Kept in this file rather than folded into `agent_tool_loop_tests.rs`, which is
//! already pinned at the size-budget split queue.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_tool_loop::{
    AgentModelTurnBudget, AgentToolLoop, AgentToolLoopOutcome, ToolLoopExecutor, ToolLoopProvider,
};
use super::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::run_contract::RunBudgetPolicy;
use crate::ai_runtime::{
    FunctionCall, LlmMessage, MessageRole, TokenUsage, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::AppResult;

// ── Harness ───────────────────────────────────────────────────────────────

#[derive(Clone)]
struct RecordedTurn {
    surfaces: Vec<String>,
    messages: Vec<LlmMessage>,
    max_completion_tokens: Option<u32>,
}

/// Records every provider turn: the tool surface the model actually received, the
/// transcript it received, and the completion allowance the loop granted it.
struct TurnRecordingProvider {
    responses: Mutex<VecDeque<GatewayResponse>>,
    turns: Mutex<Vec<RecordedTurn>>,
}

impl TurnRecordingProvider {
    fn new(responses: Vec<GatewayResponse>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
            turns: Mutex::new(Vec::new()),
        }
    }

    /// Distinct system messages the Host authored, in provider order.
    ///
    /// The transcript is append-only, so an instruction injected on turn *n* is
    /// still present on turn *n + 1*; duplicates are collapsed so a failure reads
    /// as "which instruction appeared" rather than "how often it repeated".
    fn host_instructions(&self) -> Vec<String> {
        let turns = self.turns.lock().expect("turns lock");
        let mut seen: Vec<String> = Vec::new();
        for turn in turns.iter() {
            for message in &turn.messages {
                if !matches!(message.role, MessageRole::System) {
                    continue;
                }
                // System messages are text; `text_content` keeps this total.
                let text = message.content.text_content();
                if !seen.contains(&text) {
                    seen.push(text);
                }
            }
        }
        seen
    }

    /// Whether the business tool was still offered on the last provider turn.
    fn last_turn_offered(&self, tool_name: &str) -> bool {
        self.turns
            .lock()
            .expect("turns lock")
            .last()
            .is_some_and(|turn| turn.surfaces.iter().any(|name| name == tool_name))
    }

    fn turn(&self, index: usize) -> RecordedTurn {
        self.turns
            .lock()
            .expect("turns lock")
            .get(index)
            .cloned()
            .unwrap_or_else(|| panic!("missing recorded turn {index}"))
    }

    fn turn_count(&self) -> usize {
        self.turns.lock().expect("turns lock").len()
    }
}

impl ToolLoopProvider for TurnRecordingProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        messages: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        self.turns.lock().expect("turns lock").push(RecordedTurn {
            surfaces: tools.iter().map(|tool| tool.name.clone()).collect(),
            messages: messages.to_vec(),
            max_completion_tokens: budget.max_completion_tokens,
        });
        Box::pin(async move {
            Ok(self
                .responses
                .lock()
                .expect("responses lock")
                .pop_front()
                .expect("scripted response"))
        })
    }
}

/// Succeeds, but returns a payload that carries no safe progress identity, so the
/// loop counts the round as "no new safe resource". This is what makes the
/// no-progress recipe fire; it is not a service failure.
struct NoProgressExecutor {
    calls: AtomicU32,
}

/// Dispatches, but reports `success: false`. This is the other recipe counter
/// that used to share the no-progress close-surface gate.
struct FailedServiceExecutor {
    calls: AtomicU32,
}

impl ToolLoopExecutor for NoProgressExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "body": "unchanged observation" }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}

impl ToolLoopExecutor for FailedServiceExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: false,
                output: serde_json::json!({ "error": "service unavailable" }),
                duration_ms: 1,
                tokens_used: None,
                error: Some("service unavailable".into()),
            })
        })
    }
}

struct NoopObserver;

impl StreamEventObserver for NoopObserver {
    fn observe(
        &mut self,
        _event: &super::model_gateway::StreamEvent,
        _token_index: u32,
    ) -> AppResult<()> {
        Ok(())
    }
}

fn standard_tool_loop() -> AgentToolLoop {
    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
}

fn readonly_tool_spec(name: &str) -> ToolSpec {
    ToolSpec {
        name: name.into(),
        description: format!("Test tool {name}"),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }
}

fn tool_call(id: &str, name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: name.into(),
            arguments: arguments.to_string(),
        },
    }
}

fn tool_response(call: ToolCall) -> GatewayResponse {
    tool_response_with_usage(call, 0)
}

fn tool_response_with_usage(call: ToolCall, completion_tokens: u32) -> GatewayResponse {
    GatewayResponse {
        content: None,
        tool_calls: vec![call],
        usage: TokenUsage {
            prompt_tokens: 0,
            completion_tokens,
            total_tokens: completion_tokens,
            ..Default::default()
        },
        finish_reason: "tool_calls".into(),
        reasoning_content: None,
        continuation: None,
    }
}

fn final_response(content: &str) -> GatewayResponse {
    GatewayResponse {
        content: Some(content.into()),
        tool_calls: Vec::new(),
        usage: Default::default(),
        finish_reason: "stop".into(),
        reasoning_content: None,
        continuation: None,
    }
}

/// One line per provider turn, for failure messages: which surface the model was
/// offered and how much output the loop allowed it.
fn recorded_shape(provider: &TurnRecordingProvider) -> String {
    let turns = provider.turns.lock().expect("turns lock");
    turns
        .iter()
        .enumerate()
        .map(|(index, turn)| {
            format!(
                "turn {index}: surfaces={:?} max_completion_tokens={:?}",
                turn.surfaces, turn.max_completion_tokens
            )
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Historical recipe copy the loop used to inject. Identified by its own
/// words so a reintroduced instruction cannot silently satisfy an assertion.
const CLOSURE_INSTRUCTION_MARKER: &str = "produced no new safe resources in two complete rounds";

/// The instruction the Host injects when it decides the model's repair budget is
/// spent. Identified by its own opening words; it is inline in the loop today.
const REPAIR_EXHAUSTED_MARKER: &str = "Tool proposal repair is exhausted.";

/// The opening words of the field-level repair answer the Host already gives a
/// rejected proposal; used as the positive control that repair happens at all.
const REPAIR_FEEDBACK_MARKER: &str = "was not dispatched by the Host";

fn assert_no_instruction(instructions: &[String], needle: &str, context: &str) {
    let offenders: Vec<&String> = instructions
        .iter()
        .filter(|text| text.contains(needle))
        .collect();
    assert!(
        offenders.is_empty(),
        "the Host injected an instruction it does not own. {context}\n\
         Injected: {offenders:#?}\n\
         All Host instructions: {instructions:#?}"
    );
}

fn expect_completed(outcome: &AgentToolLoopOutcome, context: &str) {
    assert!(
        !outcome.terminal.is_host_authored(),
        "{context}: the probe must end on the model's own answer, not a Host \
         limitation ({:?}): {}",
        outcome.terminal,
        outcome.content
    );
}

// ── D02 §三.5.3 — two repaired unknown-tool rounds must not force synthesis ──

/// A tool name the run never exposed is a *proposal* defect, not an execution
/// failure: the Host already answers each rejected round with the exact legal
/// tool names and their schemas, and that feedback consumes no execution
/// allowance. With the model allowance still open, the next turn must still be a
/// normal repair turn — not a forced synthesis with the surface closed.
#[tokio::test]
async fn two_repaired_unknown_tool_rounds_do_not_force_synthesis() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call("unknown-1", "not_exposed", serde_json::json!({}))),
        tool_response(tool_call("unknown-2", "not_exposed", serde_json::json!({}))),
        tool_response(tool_call("unknown-3", "not_exposed", serde_json::json!({}))),
        final_response("answered from the third attempt"),
    ]);
    let executor = NoProgressExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-two-repaired-unknown-tool-rounds",
            Vec::new(),
            vec![readonly_tool_spec("system_time_now")],
            &mut observer,
        )
        .await
        .expect("rejected proposals must not fail the Run");

    let instructions = provider.host_instructions();

    // Positive control: the field-level repair answer must exist, otherwise the
    // test would pass because nothing happened at all.
    assert!(
        instructions
            .iter()
            .any(|text| text.contains(REPAIR_FEEDBACK_MARKER) && text.contains("system_time_now")),
        "a rejected proposal must still be answered with the legal tool names and \
         their schemas; got {instructions:#?}"
    );
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "a rejected proposal must never reach the executor"
    );

    assert_no_instruction(
        &instructions,
        REPAIR_EXHAUSTED_MARKER,
        "Two rejected proposals, each already answered with the legal tool names \
         and schemas, must not be treated as an exhausted repair budget \
         (D02 §三.5.3, N16, G06).",
    );

    // The model kept the surface and produced its own answer.
    assert!(
        provider.turn_count() >= 3,
        "the loop must keep giving the model turns while the ledger allows them; \
         it stopped after {} turn(s)",
        provider.turn_count()
    );
    assert!(
        provider.last_turn_offered("system_time_now"),
        "two rejected proposals must not close the business surface; recorded \
         shape:\n{}",
        recorded_shape(&provider)
    );
    expect_completed(&outcome, "two repaired unknown-tool rounds");
    assert_eq!(outcome.content, "answered from the third attempt");
}

// ── D02 §三.5.4 — no-progress counters are not the envelope ────────────────

/// Identical `read_note` repeats are rejected as `tool_call_already_succeeded`
/// before they can increment `no_progress_rounds`. Kept as the pin that this
/// rejection path must not inject a closure instruction. The actual no-progress
/// counter is covered by
/// `repeated_observation_of_the_same_resource_does_not_close_the_surface`.
#[tokio::test]
async fn no_progress_rounds_do_not_close_the_surface_while_the_ledger_allows_it() {
    // Identical `read_note` arguments are rejected after the first dispatch.
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "t-1",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        tool_response(tool_call(
            "t-2",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        tool_response(tool_call(
            "t-3",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        final_response("synthesized only after three observation rounds"),
    ]);
    let executor = NoProgressExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-no-progress-recipe",
            Vec::new(),
            vec![readonly_tool_spec("read_note")],
            &mut observer,
        )
        .await
        .expect("a no-progress round is not a Run failure");

    assert!(
        provider
            .turns
            .lock()
            .expect("turns lock")
            .get(1)
            .is_some_and(|turn| turn.surfaces.iter().any(|name| name == "read_note")),
        "the second turn must still offer the business tool so the no-progress \
         counter can accumulate; recorded shape:\n{}",
        recorded_shape(&provider)
    );
    // The first round dispatches; later identical repeats are rejected as
    // `tool_call_already_succeeded` and never increment `no_progress_rounds`.
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "exactly one observation dispatches; an identical repeat is not a new \
         action. recorded shape:\n{}",
        recorded_shape(&provider)
    );
    assert!(
        provider.turn_count() >= 3,
        "the counter needs two complete rounds before it can fire; recorded shape:\n{}",
        recorded_shape(&provider)
    );

    assert_no_instruction(
        &provider.host_instructions(),
        CLOSURE_INSTRUCTION_MARKER,
        "A repeat rejected as `tool_call_already_succeeded` must not inject a \
         recipe closure instruction (D02 §三.5.4, R11, G06).",
    );

    assert!(
        provider.turn_count() >= 3,
        "the loop must consume the model's own turns instead of closing the \
         surface; it stopped after {} turn(s)",
        provider.turn_count()
    );
    expect_completed(&outcome, "no-progress recipe");
    assert_eq!(
        outcome.content,
        "synthesized only after three observation rounds"
    );
}

// ── D02 §三.5.1 — an unchanged resource is not a reason to stop either ─────

/// The discovery reading of §三.5.1, on a local surface: re-reading the same,
/// unchanged note is not progress, but it is not an envelope reason to close the
/// surface either. The model owns the decision to re-read or to change approach.
#[tokio::test]
async fn repeated_observation_of_the_same_resource_does_not_close_the_surface() {
    // Same shape as the loop's own `:877` test: three dispatching rounds whose
    // observations are byte-identical, so `safe_progress_identities` yields
    // nothing and `no_progress_rounds` reaches 2 on the third turn. The arguments
    // must differ per call, otherwise the second call is rejected as
    // `tool_call_already_succeeded` and no second round completes.
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "s-1",
            "system_time_now",
            serde_json::json!({ "step": 1 }),
        )),
        tool_response(tool_call(
            "s-2",
            "system_time_now",
            serde_json::json!({ "step": 2 }),
        )),
        tool_response(tool_call(
            "s-3",
            "system_time_now",
            serde_json::json!({ "step": 3 }),
        )),
        final_response("answered after three unchanged observations"),
    ]);
    let executor = NoProgressExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-same-resource-repeats",
            Vec::new(),
            vec![readonly_tool_spec("system_time_now")],
            &mut observer,
        )
        .await
        .expect("re-reading an unchanged note is not a Run failure");

    assert_no_instruction(
        &provider.host_instructions(),
        CLOSURE_INSTRUCTION_MARKER,
        "A second observation of the same, unchanged resource is not progress, but \
         it is not an envelope reason to close the surface either: the model owns \
         the decision to re-read or to change its approach \
         (D02 §三.5.1, N21, G06).",
    );

    // The three differing calls must dispatch, otherwise the counter cannot reach 2.
    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        3,
        "three differing calls must dispatch so the counter can reach 2; recorded \
         shape:\n{}",
        recorded_shape(&provider)
    );
    assert!(
        provider.turn_count() >= 3,
        "the third turn must be reached before the counter fires; recorded shape:\n{}",
        recorded_shape(&provider)
    );
    assert!(
        provider.last_turn_offered("system_time_now"),
        "the business surface must stay open while the ledger allows another \
         action; recorded shape:\n{}",
        recorded_shape(&provider)
    );
    expect_completed(&outcome, "same-resource repeats");
    assert_eq!(
        outcome.content,
        "answered after three unchanged observations"
    );
}

// ── D02 §三.5.4 — failed-service rounds are also recipe, not envelope ──────

/// Two dispatched rounds with `success: false` share the no-progress close-surface
/// gate. While the ledger still allows another action, the Host must not close
/// the surface or inject a closure instruction.
#[tokio::test]
async fn failed_service_rounds_do_not_close_the_surface_while_the_ledger_allows_it() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "f-1",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        tool_response(tool_call(
            "f-2",
            "read_note",
            serde_json::json!({ "path": "b.md" }),
        )),
        tool_response(tool_call(
            "f-3",
            "read_note",
            serde_json::json!({ "path": "c.md" }),
        )),
        final_response("answered after two failed observations"),
    ]);
    let executor = FailedServiceExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-failed-service-recipe",
            Vec::new(),
            vec![readonly_tool_spec("read_note")],
            &mut observer,
        )
        .await
        .expect("a failed-service round is not a Run failure");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        3,
        "three differing failed observations must dispatch; recorded shape:\n{}",
        recorded_shape(&provider)
    );
    assert_no_instruction(
        &provider.host_instructions(),
        CLOSURE_INSTRUCTION_MARKER,
        "Two failed-service rounds exhausted a recipe counter, not the envelope \
         (D02 §三.5.4, R11, G06).",
    );
    assert!(
        provider.last_turn_offered("read_note"),
        "the business surface must stay open after two failed-service rounds; \
         recorded shape:\n{}",
        recorded_shape(&provider)
    );
    expect_completed(&outcome, "failed-service recipe");
    assert_eq!(outcome.content, "answered after two failed observations");
}

// ── R12 — completion reserve is an output-cap envelope ────────────────────

/// First turn spends enough completion tokens that the remainder is at or below
/// the reserve. `R12` option B: withhold the business surface, give the remaining
/// tokens to the expression turn, and do not inject a closure instruction.
#[tokio::test]
async fn completion_reserve_does_not_inject_a_recipe_closure_instruction() {
    const TOTAL: u32 = 6_000;
    const TURN_CEILING: u32 = 4_000;
    // Explore allowance is TOTAL - reserve = 2_000. Usage must fit that
    // cap (otherwise the loop rejects the turn as ToolLoopLimit) and still
    // leave remaining <= reserve so the next turn is the expression turn.
    const FIRST_TURN_USAGE: u32 = 2_000;
    let reserve = TOTAL.min(TURN_CEILING);
    let remaining_after_first = TOTAL - FIRST_TURN_USAGE;
    assert!(
        remaining_after_first <= reserve,
        "this case must actually reach remaining <= reserve"
    );
    assert_eq!(
        FIRST_TURN_USAGE,
        TOTAL - reserve,
        "first-turn usage must equal the explore allowance"
    );

    let provider = TurnRecordingProvider::new(vec![
        tool_response_with_usage(
            tool_call("r-1", "read_note", serde_json::json!({ "path": "a.md" })),
            FIRST_TURN_USAGE,
        ),
        final_response("answered within the reserved envelope"),
    ]);
    let executor = NoProgressExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let mut policy = RunBudgetPolicy::standard();
    policy.max_completion_tokens = TOTAL;
    policy.max_turn_output_tokens = TURN_CEILING;

    let outcome = AgentToolLoop::from_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-completion-reserve",
            Vec::new(),
            vec![readonly_tool_spec("read_note")],
            &mut observer,
        )
        .await
        .expect("the reserve Run must complete");

    assert_eq!(
        provider.turn_count(),
        2,
        "explore then expression; recorded shape:\n{}",
        recorded_shape(&provider)
    );

    let explore = provider.turn(0);
    assert!(
        explore.surfaces.iter().any(|name| name == "read_note"),
        "the first turn is still an explore turn; recorded shape:\n{}",
        recorded_shape(&provider)
    );
    assert_eq!(
        explore.max_completion_tokens,
        Some(TOTAL - reserve),
        "explore allowance is remaining minus reserve; recorded shape:\n{}",
        recorded_shape(&provider)
    );

    let expression = provider.turn(1);
    assert!(
        !expression.surfaces.iter().any(|name| name == "read_note"),
        "R12 allows withholding the business surface once remaining <= reserve; \
         recorded shape:\n{}",
        recorded_shape(&provider)
    );
    assert_eq!(
        expression.max_completion_tokens,
        Some(remaining_after_first),
        "the expression turn must receive the remaining tokens, not zero; \
         recorded shape:\n{}",
        recorded_shape(&provider)
    );

    assert_no_instruction(
        &provider.host_instructions(),
        CLOSURE_INSTRUCTION_MARKER,
        "The completion reserve is an output-cap envelope (R12 option B): it may \
         withhold tools and must still give remaining tokens to the expression \
         turn, but must not inject a recipe closure instruction.",
    );
    expect_completed(&outcome, "completion reserve");
    assert_eq!(outcome.content, "answered within the reserved envelope");
}
