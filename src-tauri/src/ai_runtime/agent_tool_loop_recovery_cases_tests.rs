//! The four loop-level recovery cases `D02` §三 requires (`V04`).
//!
//! `K16` invariant 5 asks for at least one field-level repair opportunity after a
//! recoverable error. The cases here are the ones a *combination* of components has
//! to get right — a rejection answered with usable feedback, the model correcting
//! itself, and the corrected proposal actually reaching dispatch:
//!
//! 1. an unknown tool, corrected to a legal one, then dispatched;
//! 2. invalid arguments, corrected from the field-level reason, then dispatched;
//! 3. a network tool that fails once, re-proposed, and then succeeds;
//! 4. a cancellation after the first dispatch, which must open no further turn.
//!
//! The existing `agent_tool_loop_tests.rs` and
//! `agent_tool_loop_host_authority_tests.rs` cover the neighbouring but different
//! shapes (two rejected rounds followed by the model answering on its own; a
//! failed web tool followed by the model answering anyway). None of them assert
//! that the *corrected* proposal is dispatched, which is the property that turns
//! "the Host said something" into "the Run recovered".
//!
//! Every case also pins the two things recovery must not cost:
//!
//! - the rejected attempt never reaches the executor, so it can never be counted
//!   or reported as a real tool execution (`K16` invariant 6, `M05` invariant 4);
//! - the Run ends on the model's own answer, not on a Host-authored limitation
//!   (`K16` invariant 7). `N17` states the user-visible form of that: a recovered
//!   Run must not announce a capability downgrade. The `capability_degraded`
//!   event itself is emitted one layer up, in `run_tool_loop.rs`, and is pinned
//!   there by `deferred_web_degradation_skips_after_successful_web_evidence` and
//!   `deferred_web_degradation_skips_when_failure_cleared_after_retry_success`
//!   (both assert a count of 0); this file pins the loop-level half.
//!
//! Kept out of `agent_tool_loop_tests.rs`, which is already at the size-budget
//! split queue.
//!
//! **Verification status**: all four cases passed on their first run, with no
//! change to `AgentToolLoop`. The `D02` plan expected at least one of them to be
//! red and to prove a missing behaviour; none was. They are therefore **pins, not
//! fixes** — the loop already recovered in these four shapes, and nothing in the
//! suite asserted it before. Recording this explicitly so the green result is not
//! read as "a defect was found and repaired here", and so a later reader does not
//! assume the assertions were weakened to make the run pass.

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

/// Every provider turn as the loop actually presented it: the transcript the
/// model received and the surface it was offered.
struct RecordedTurn {
    surfaces: Vec<String>,
    messages: Vec<LlmMessage>,
}

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

    /// Distinct Host-authored system messages, in provider order.
    ///
    /// The transcript is append-only, so an instruction injected on turn *n* is
    /// still present on turn *n + 1*; collapsing duplicates makes a failure read
    /// as "which instruction appeared" rather than "how often it repeated".
    fn host_instructions(&self) -> Vec<String> {
        let turns = self.turns.lock().expect("turns lock");
        let mut seen: Vec<String> = Vec::new();
        for turn in turns.iter() {
            for message in &turn.messages {
                if !matches!(message.role, MessageRole::System) {
                    continue;
                }
                let text = message.content.text_content();
                if !seen.contains(&text) {
                    seen.push(text);
                }
            }
        }
        seen
    }

    fn turn_count(&self) -> usize {
        self.turns.lock().expect("turns lock").len()
    }

    /// One line per turn, for failure messages.
    fn recorded_shape(&self) -> String {
        let turns = self.turns.lock().expect("turns lock");
        turns
            .iter()
            .enumerate()
            .map(|(index, turn)| format!("turn {index}: surfaces={:?}", turn.surfaces))
            .collect::<Vec<_>>()
            .join("\n")
    }
}

impl ToolLoopProvider for TurnRecordingProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        messages: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        _budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        self.turns.lock().expect("turns lock").push(RecordedTurn {
            surfaces: tools.iter().map(|tool| tool.name.clone()).collect(),
            messages: messages.to_vec(),
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

/// Dispatches every accepted call successfully and records what it was asked to
/// run. An empty `dispatched` list after a rejection is the evidence that the
/// rejected proposal never became a real tool execution.
struct RecordingExecutor {
    dispatched: Mutex<Vec<String>>,
    calls: AtomicU32,
    /// When set, the Run is cancelled as this call is dispatched — the shape of
    /// "the user cancelled while the first action was running".
    cancel_after_dispatch: Option<String>,
}

impl RecordingExecutor {
    fn new() -> Self {
        Self {
            dispatched: Mutex::new(Vec::new()),
            calls: AtomicU32::new(0),
            cancel_after_dispatch: None,
        }
    }

    fn cancelling(run_id: &str) -> Self {
        Self {
            cancel_after_dispatch: Some(run_id.to_string()),
            ..Self::new()
        }
    }

    fn dispatched_names(&self) -> Vec<String> {
        self.dispatched.lock().expect("dispatched lock").clone()
    }

    fn calls(&self) -> u32 {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ToolLoopExecutor for RecordingExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.dispatched
            .lock()
            .expect("dispatched lock")
            .push(call.function.name.clone());
        if let Some(run_id) = &self.cancel_after_dispatch {
            crate::ai_runtime::model_gateway::request_abort(run_id);
        }
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "body": "observed" }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}

/// Dispatches, but reports `success: false` on the first call and succeeds
/// afterwards — a transient service failure that recovers.
struct FlakyServiceExecutor {
    calls: AtomicU32,
}

impl ToolLoopExecutor for FlakyServiceExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        let attempt = self.calls.fetch_add(1, Ordering::SeqCst);
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(if attempt == 0 {
                ToolCallResult {
                    tool_name,
                    success: false,
                    output: serde_json::json!({ "error": "service unavailable" }),
                    duration_ms: 1,
                    tokens_used: None,
                    error: Some("service unavailable".into()),
                }
            } else {
                ToolCallResult {
                    tool_name,
                    success: true,
                    output: serde_json::json!({ "body": "recovered observation" }),
                    duration_ms: 1,
                    tokens_used: None,
                    error: None,
                }
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

// ── Fixtures ──────────────────────────────────────────────────────────────

fn standard_tool_loop() -> AgentToolLoop {
    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
}

fn web_search_spec() -> ToolSpec {
    ToolSpec {
        name: "web_search".into(),
        description: "Search the web".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": { "query": {"type": "string"} },
            "additionalProperties": false,
            "required": ["query"]
        }),
        access_level: crate::ai_runtime::ToolAccessLevel::Network,
        requires_confirmation: false,
        max_results: Some(8),
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
        usage: TokenUsage::default(),
        finish_reason: "tool_calls".into(),
        ..Default::default()
    }
}

fn final_response(content: &str) -> GatewayResponse {
    GatewayResponse {
        content: Some(content.into()),
        tool_calls: Vec::new(),
        usage: Default::default(),
        finish_reason: "stop".into(),
        ..Default::default()
    }
}

/// The Host's repair answer for a rejected proposal, identified by its own words.
const REPAIR_FEEDBACK_MARKER: &str = "was not dispatched by the Host";

/// Historical recipe copy the loop used to inject while closing the surface.
const CLOSURE_INSTRUCTION_MARKER: &str = "produced no new safe resources in two complete rounds";

/// The Host's "your repair budget is spent" instruction.
const REPAIR_EXHAUSTED_MARKER: &str = "Tool proposal repair is exhausted.";

fn feedback_for(instructions: &[String], marker: &str) -> String {
    instructions
        .iter()
        .filter(|text| text.contains(marker))
        .cloned()
        .collect::<Vec<_>>()
        .join("\n")
}

/// Recovery must end on the model's own answer. A Host-authored terminal is the
/// loop telling the user it could not finish, which is what `N16` / `N17` forbid
/// for a recoverable error.
fn expect_model_answer(outcome: &AgentToolLoopOutcome, context: &str) {
    assert!(
        !outcome.terminal.is_host_authored(),
        "{context}: the Run must end on the model's own answer after a successful \
         recovery, not on a Host limitation ({:?}): {}",
        outcome.terminal,
        outcome.content
    );
    assert!(
        !outcome.terminal.as_str().starts_with("host_"),
        "{context}: a recovered Run must not surface a capability downgrade \
         (N17); terminal was {:?}",
        outcome.terminal
    );
}

/// No recipe counter may close the surface or announce exhausted repair: the
/// envelope is the only thing allowed to withhold tools (`R11`, `R12`, `G06`).
fn assert_no_recipe_instruction(instructions: &[String], context: &str) {
    for marker in [CLOSURE_INSTRUCTION_MARKER, REPAIR_EXHAUSTED_MARKER] {
        let offenders: Vec<&String> = instructions
            .iter()
            .filter(|text| text.contains(marker))
            .collect();
        assert!(
            offenders.is_empty(),
            "{context}: the Host injected a recipe instruction it does not own \
             (`R11`, `R12`, `G06`).\nInjected: {offenders:#?}\nAll Host \
             instructions: {instructions:#?}"
        );
    }
}

// ── Case 1 — unknown tool, corrected, then dispatched ─────────────────────

/// The full repair arc for an unknown tool. The existing
/// `two_repaired_unknown_tool_rounds_do_not_force_synthesis` covers two rejected
/// rounds followed by the model answering on its own; this covers the shape the
/// contract actually promises — the model reads the Host's answer, proposes a
/// legal tool, and that proposal *runs*.
#[tokio::test]
async fn unknown_tool_is_corrected_and_then_dispatched() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "u-1",
            "not_exposed",
            serde_json::json!({ "query": "x" }),
        )),
        tool_response(tool_call(
            "u-2",
            "web_search",
            serde_json::json!({ "query": "x" }),
        )),
        final_response("answered after correcting the tool name"),
    ]);
    let executor = RecordingExecutor::new();
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-recovery-unknown-tool",
            Vec::new(),
            vec![web_search_spec()],
            &mut observer,
        )
        .await
        .expect("a corrected proposal must not fail the Run");

    // Positive control: the repair answer must exist, otherwise this test would
    // pass because the Host said nothing at all.
    let instructions = provider.host_instructions();
    let feedback = feedback_for(&instructions, REPAIR_FEEDBACK_MARKER);
    assert!(
        feedback.contains("not_exposed"),
        "the rejection must name the offending proposal; got {instructions:#?}"
    );
    assert!(
        feedback.contains("web_search"),
        "the repair answer must carry the legal tool names and their schemas; \
         got {instructions:#?}"
    );
    assert!(
        feedback.contains("rejected_before_dispatch"),
        "the rejection must keep its stable code so the model can tell a proposal \
         defect from an execution failure; got {instructions:#?}"
    );

    // The rejected proposal is not an execution, and the corrected one is.
    assert_eq!(
        executor.calls(),
        1,
        "exactly one call may reach the executor: the corrected proposal. \
         dispatched={:?}\n{}",
        executor.dispatched_names(),
        provider.recorded_shape()
    );
    assert_eq!(
        executor.dispatched_names(),
        vec!["web_search".to_string()],
        "the dispatched call must be the corrected tool, not the rejected name"
    );

    assert!(
        outcome.content.contains("correcting the tool name"),
        "the Run must finish on the model's answer that follows the successful \
         dispatch; got {:?}",
        outcome.content
    );
    assert_no_recipe_instruction(&instructions, "a corrected unknown tool");
    expect_model_answer(&outcome, "corrected unknown tool");

    // The surface stayed open across the repair round.
    assert!(
        provider.turn_count() >= 3,
        "the corrected turn must actually run; recorded shape:\n{}",
        provider.recorded_shape()
    );
}

// ── Case 2 — invalid arguments, corrected from the field-level reason ─────

/// The same arc for a tool that exists but whose arguments fail the declared
/// schema. The existing `schema_mismatch_feedback_*` cases pin the feedback text
/// only; here the model acts on it and the corrected call must dispatch.
#[tokio::test]
async fn invalid_arguments_are_corrected_from_the_field_reason_then_dispatched() {
    // `query` is required and must be a string.
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call("a-1", "web_search", serde_json::json!({}))),
        tool_response(tool_call(
            "a-2",
            "web_search",
            serde_json::json!({ "query": "corrected" }),
        )),
        final_response("answered after correcting the arguments"),
    ]);
    let executor = RecordingExecutor::new();
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-recovery-invalid-arguments",
            Vec::new(),
            vec![web_search_spec()],
            &mut observer,
        )
        .await
        .expect("a corrected proposal must not fail the Run");

    let instructions = provider.host_instructions();
    let feedback = feedback_for(&instructions, REPAIR_FEEDBACK_MARKER);
    assert!(
        feedback.contains("arguments_schema_mismatch"),
        "the stable rejection code must remain; got {instructions:#?}"
    );
    assert!(
        feedback.contains("arguments.query is required"),
        "the repair answer must name the exact field, otherwise the model cannot \
         correct it; got {instructions:#?}"
    );

    assert_eq!(
        executor.calls(),
        1,
        "the schema-invalid proposal must not reach the executor, and the \
         corrected one must. dispatched={:?}\n{}",
        executor.dispatched_names(),
        provider.recorded_shape()
    );
    assert_eq!(
        executor.dispatched_names(),
        vec!["web_search".to_string()],
        "the corrected arguments must be dispatched"
    );

    assert!(
        outcome.content.contains("correcting the arguments"),
        "the Run must finish on the model's answer after the successful dispatch; \
         got {:?}",
        outcome.content
    );
    assert_no_recipe_instruction(&instructions, "corrected invalid arguments");
    expect_model_answer(&outcome, "corrected invalid arguments");
}

// ── Case 3 — a network tool fails, is re-proposed, and succeeds ───────────

/// A transient service failure must cost a retry, not the task. The existing
/// `online_mode_continues_after_a_failed_web_tool_with_the_model_answer` stops at
/// "the model answered anyway"; `K16` asks for the stronger shape: the model
/// re-proposes and the second attempt is the one that works.
#[tokio::test]
async fn failed_network_tool_is_re_proposed_and_then_succeeds() {
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "n-1",
            "web_search",
            serde_json::json!({ "query": "first" }),
        )),
        // A different query, so this is a new action rather than a repeat.
        tool_response(tool_call(
            "n-2",
            "web_search",
            serde_json::json!({ "query": "second" }),
        )),
        final_response("answered from the recovered observation"),
    ]);
    let executor = FlakyServiceExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-recovery-failed-service",
            Vec::new(),
            vec![web_search_spec()],
            &mut observer,
        )
        .await
        .expect("a failed service round is not a Run failure");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        2,
        "the failed attempt and the re-proposal must both dispatch; recorded \
         shape:\n{}",
        provider.recorded_shape()
    );
    assert!(
        provider.turn_count() >= 3,
        "the model must get the turn that carries its re-proposal; recorded \
         shape:\n{}",
        provider.recorded_shape()
    );

    let instructions = provider.host_instructions();
    assert_no_recipe_instruction(&instructions, "a recovered service failure");
    expect_model_answer(&outcome, "recovered service failure");
    assert_eq!(outcome.content, "answered from the recovered observation");
}

// ── Case 4 — cancellation opens no further turn ───────────────────────────

/// The existing `cancelled_run_never_starts_a_model_or_tool_turn` pins a Run
/// cancelled *before* it began. This pins the other half: cancellation that
/// arrives after the first round has already dispatched must open no further
/// model or tool turn — a cancelled Run must not take one more action on the way
/// out (`M05` invariant 6).
#[tokio::test]
async fn cancellation_after_the_first_dispatch_opens_no_further_turn() {
    const RUN_ID: &str = "run-recovery-cancel-after-dispatch";
    // Two responses are scripted so that a loop which ignored the cancellation
    // would visibly consume the second one instead of running out of script.
    let provider = TurnRecordingProvider::new(vec![
        tool_response(tool_call(
            "c-1",
            "web_search",
            serde_json::json!({ "query": "first" }),
        )),
        final_response("this answer must never be produced: the Run was cancelled"),
    ]);
    let executor = RecordingExecutor::cancelling(RUN_ID);
    let mut observer = NoopObserver;

    let result = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            RUN_ID,
            Vec::new(),
            vec![web_search_spec()],
            &mut observer,
        )
        .await;
    crate::ai_runtime::model_gateway::clear_abort(RUN_ID);

    assert_eq!(
        result.expect_err("a cancelled Run must stop").to_string(),
        "agent_run_cancelled"
    );
    assert_eq!(
        executor.calls(),
        1,
        "the call already in flight may finish, but no further tool turn may \
         start; recorded shape:\n{}",
        provider.recorded_shape()
    );
    assert_eq!(
        provider.turn_count(),
        1,
        "no model turn may start after the cancellation; recorded shape:\n{}",
        provider.recorded_shape()
    );
}
