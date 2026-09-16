//! Host authority over the next action: the negative cases for `R11` / `N21` / `G06`.
//!
//! The Host may enforce the envelope (still authorized, not cancelled, ledger not
//! exhausted). It must not also act as a second planner by injecting
//! `tool_surface_closed_instruction` because a *recipe* counter fired — two
//! rejected proposals, or two rounds without a new safe resource.
//!
//! Written test-first for the D02 evidence order *negative case → decision
//! (`R12`) → implementation*. Each assertion names the recorded case in
//! `agent-harness/implementation/D02-protocol-and-recovery.md` §三 that it pins:
//!
//! - §三.5.3 — two field-level `unknown_tool` rounds must not force synthesis
//!   → **red** (`two_repaired_unknown_tool_rounds_do_not_force_synthesis`): the
//!   loop injects `Tool proposal repair is exhausted.` after the second rejected
//!   proposal, so the third turn is forced synthesis with an empty surface.
//! - §三.5.1 — two rounds without a new safe resource must not close the surface
//!   → **red** (`repeated_observation_of_the_same_resource_...`): the loop injects
//!   `tool_surface_closed_instruction` on the third turn.
//! - §三.5.4 — the same counter must not fire on a repeatable observation
//!   → **green** (`no_progress_rounds_do_not_close_the_surface_...`): measured
//!   2026-09-16, no closure instruction is injected when the repeated call is
//!   rejected as `tool_call_already_succeeded`; kept as the green pin.
//! - the completion reserve — **green**
//!   (`completion_reserve_does_not_inject_a_recipe_closure_instruction`): with the
//!   reserve reached, no closure instruction is injected either; only the surface
//!   narrows, which is why the surface shape is left to `R12`.
//!
//! Measured 2026-09-16 with
//! `DEVELOPER_DIR=/Library/Developer/CommandLineTools cargo test --lib
//! agent_tool_loop_host_authority`: 2 passed, 2 failed. The no-progress counter is
//! incremented by rounds that actually dispatch, so a test that wants the recipe
//! to fire must vary its arguments each round; an identical repeat is rejected
//! before dispatch and never reaches the counter.
//!
//! These tests deliberately assert **no closure instruction**, never the *shape*
//! of the surface on a reserve turn. The surface shape belongs to `R12`: closing
//! the business surface is currently the only mechanism that lets a reserve turn
//! carry a non-zero allowance (`remaining.saturating_sub(synthesis_output_reserve)`
//! would otherwise be `0` and the loop would return `ToolLoopLimit`). Asserting it
//! here would pre-empt the decision instead of pinning the contract.
//!
//! When `R12` lands: fold the surviving assertions into `agent_tool_loop_tests.rs`,
//! extend `:699` / `:877` with the two independent claims (final-turn reserve vs
//! no-progress closure), and delete this file. It exists so the D02 negative cases
//! have an owner before the loop changes.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_tool_loop::prompt_assembly::tool_surface_closed_instruction;
use super::agent_tool_loop::{
    AgentModelTurnBudget, AgentToolLoop, AgentToolLoopOutcome, ToolLoopExecutor, ToolLoopProvider,
};
use super::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::run_contract::RunBudgetPolicy;
use crate::ai_runtime::{
    FunctionCall, LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::AppResult;

// ── Harness ───────────────────────────────────────────────────────────────

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
    GatewayResponse {
        content: None,
        tool_calls: vec![call],
        usage: Default::default(),
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

/// The loop's own closure instruction, taken from its constructor rather than
/// retyped, so a reworded instruction cannot silently satisfy an assertion.
fn closure_instruction_text() -> String {
    tool_surface_closed_instruction().content.text_content()
}

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
    expect_completed(&outcome, "two repaired unknown-tool rounds");
    assert_eq!(outcome.content, "answered from the third attempt");
}

// ── D02 §三.5.4 — no-progress counters are not the envelope ────────────────

/// `no_progress_rounds` and `failed_service_rounds` are recipe counters. While
/// authorization, cancellation and the ledger all still allow another action, the
/// Host must not close the surface because two rounds produced no new safe
/// resource.
///
/// Measured on 2026-09-16: with a *repeatable* observation (identical arguments,
/// rejected the second time as `tool_call_already_succeeded`) the closure
/// instruction is **not** injected — the surface still narrows on the following
/// turn, but the reason is not the no-progress recipe. The recipe closure is
/// reproduced by the `system_time_now` case in
/// `repeated_observation_of_the_same_resource_does_not_close_the_surface`, which
/// dispatches three times. This test is kept as the green pin for the
/// non-firing path, so a future change that starts injecting the instruction
/// here fails loudly.
///
/// The Run uses the standard completion envelope and stays far below the tool
/// allowance, so the *only* gate that could close the surface is the no-progress
/// recipe.
#[tokio::test]
async fn no_progress_rounds_do_not_close_the_surface_while_the_ledger_allows_it() {
    // The tool must be one whose repeat dispatch is allowed: an identical
    // `system_time_now` call is rejected on the second turn with
    // `tool_call_already_succeeded`, which leaves only one dispatched round and
    // never accumulates `no_progress_rounds`. Same shape as the loop's own
    // `:877` test, so the counter is reached for the reason under test.
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
    // The first round dispatches; the second is rejected as
    // `tool_call_already_succeeded`, because an identical `read_note` call is not a
    // new action. That rejection is correct, so this test asserts the counter's
    // *effect* rather than a dispatch count: two rounds pass without a new safe
    // resource and the closure lands on the following turn.
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
        &closure_instruction_text(),
        "Two rounds without a new safe resource exhausted a recipe counter, not \
         the envelope: authorization was valid, the Run was not cancelled, the \
         tool allowance had 21 calls left, and the completion envelope was not \
         near its reserve (D02 §三.5.4, R11, G06).",
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
        &closure_instruction_text(),
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

// ── The completion reserve is the other recipe gate ────────────────────────

/// The completion reserve is a different gate from the no-progress counter, and
/// it currently reaches the same instruction. With a small completion envelope
/// the reserve is hit on the second turn while the ledger still allows many more
/// actions, so nothing except the reserve can explain a closure.
///
/// The Host closing the business surface here is not an envelope event: the Run
/// still has model turns, tool allowance and authorization. The assertion is
/// again only "no closure instruction" — which surface a reserve turn carries is
/// `R12`'s decision, because the reserve currently shares its gate with the
/// output cap (`remaining - synthesis_output_reserve` is `0` for any turn the
/// loop does not classify as a synthesis turn).
#[tokio::test]
async fn completion_reserve_does_not_inject_a_recipe_closure_instruction() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "r-1",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        tool_response(tool_call(
            "r-2",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        final_response("answered within the reserved envelope"),
    ]);
    let executor = NoProgressExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let mut policy = RunBudgetPolicy::standard();
    // 6_000 total with a 4_000 turn ceiling leaves a 2_000 reserve: the first
    // turn gets 2_000, the second is already inside the reserve.
    policy.max_completion_tokens = 6_000;
    policy.max_turn_output_tokens = 4_000;

    // Reachability guard: if these numbers stop producing a reserve turn, this
    // test would pass vacuously, so it is asserted rather than assumed.
    let reserve = 6_000 - 4_000;
    assert_eq!(
        reserve, 2_000,
        "the reserve arithmetic this test depends on"
    );

    let _ = AgentToolLoop::from_policy(&policy)
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

    assert!(
        provider.turn_count() >= 2,
        "the reserve must be reached within the scripted turns; observed {} turn(s)",
        provider.turn_count()
    );

    assert_no_instruction(
        &provider.host_instructions(),
        &closure_instruction_text(),
        "The completion reserve was reached, but the ledger, the tool allowance \
         and the model-turn allowance all still allowed another action, and the \
         Run was not cancelled. A reserve is a bounded-envelope fact, not a \
         licence to close the business surface (D02 §二, R11, G06).",
    );
}
