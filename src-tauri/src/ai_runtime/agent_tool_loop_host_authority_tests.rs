//! Host authority over the next action: the negative cases for `R11` / `N21` / `G06`.
//!
//! The Host may enforce the envelope (still authorized, not cancelled, ledger not
//! exhausted). It must not also act as a second planner by injecting
//! `tool_surface_closed_instruction` because a *recipe* counter fired — two
//! rejected proposals, or two rounds without a new safe resource.
//!
//! Registered red on purpose, following the D02 evidence order
//! *negative case → decision (`R12`) → implementation*. Each failing assertion
//! names the recorded case in
//! `agent-harness/implementation/D02-protocol-and-recovery.md` §三 that it pins:
//!
//! - §三.5.3 — two field-level `unknown_tool` rounds must not force synthesis
//! - §三.5.4 — `no_progress_rounds` / `failed_service_rounds` must not close the
//!   surface while the ledger still allows another action
//! - §三.5.1 — the Host must not close the surface just because saved work
//!   reached material that had already been observed
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
                if message.role != MessageRole::System {
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
/// The Run uses the standard completion envelope and stays far below the tool
/// allowance, so the *only* gate that could close the surface is the no-progress
/// recipe.
#[tokio::test]
async fn no_progress_rounds_do_not_close_the_surface_while_the_ledger_allows_it() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call("t-1", "system_time_now", serde_json::json!({}))),
        tool_response(tool_call("t-2", "system_time_now", serde_json::json!({}))),
        tool_response(tool_call("t-3", "system_time_now", serde_json::json!({}))),
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
            vec![readonly_tool_spec("system_time_now")],
            &mut observer,
        )
        .await
        .expect("a no-progress round is not a Run failure");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        3,
        "the scripted observations must actually reach the executor"
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
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "s-1",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        tool_response(tool_call(
            "s-2",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        tool_response(tool_call(
            "s-3",
            "read_note",
            serde_json::json!({ "path": "a.md" }),
        )),
        final_response("answered after re-reading the same note"),
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
            vec![readonly_tool_spec("read_note")],
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

    assert!(
        provider.last_turn_offered("read_note"),
        "the business surface must stay open while the ledger allows another \
         action; the last provider turn offered {:?}",
        provider
            .turns
            .lock()
            .expect("turns lock")
            .last()
            .map(|turn| turn.surfaces.clone())
    );
    expect_completed(&outcome, "same-resource repeats");
    assert_eq!(outcome.content, "answered after re-reading the same note");
}

// ── Probe: the reserve-turn shape `R12` has to decide on ───────────────────

/// Not a contract; a diagnostic that records the two facts `R12` needs, so the
/// decision starts from the current shape rather than from memory:
///
/// 1. which completion allowance a reserve turn receives;
/// 2. which surface it receives.
///
/// The assertion only pins that the probe observed turns, so it cannot pass
/// vacuously against a loop that never ran.
#[tokio::test]
async fn probe_records_the_reserve_turn_shape_for_r12() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call("t-1", "system_time_now", serde_json::json!({}))),
        tool_response(tool_call("t-2", "system_time_now", serde_json::json!({}))),
        tool_response(tool_call("t-3", "system_time_now", serde_json::json!({}))),
        final_response("probe answer"),
    ]);
    let executor = NoProgressExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let mut policy = RunBudgetPolicy::standard();
    // A small completion envelope makes the reserve visible within one Run.
    policy.max_completion_tokens = 6_000;
    policy.max_turn_output_tokens = 4_000;

    let _ = AgentToolLoop::from_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-reserve-turn-probe",
            Vec::new(),
            vec![readonly_tool_spec("system_time_now")],
            &mut observer,
        )
        .await
        .expect("the probe Run must complete");

    let turns = provider.turns.lock().expect("turns lock");
    let shape: Vec<(usize, Vec<String>, Option<u32>)> = turns
        .iter()
        .enumerate()
        .map(|(index, turn)| (index, turn.surfaces.clone(), turn.max_completion_tokens))
        .collect();

    assert!(
        !shape.is_empty(),
        "the probe must observe provider turns; recorded shape: {shape:?}"
    );
}
