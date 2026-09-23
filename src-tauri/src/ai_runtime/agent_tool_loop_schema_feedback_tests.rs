//! Field-level Host repair for invalid tool arguments (`K16` / `C14`).
//!
//! A rejected proposal must keep the stable code `arguments_schema_mismatch`
//! and also carry `GuardResult::Block.reason` (path, allowed type / enum,
//! required field). Kept out of `agent_tool_loop_tests.rs` because that file is
//! already on the size-budget split queue.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_tool_loop::{
    AgentModelTurnBudget, AgentToolLoop, ToolLoopExecutor, ToolLoopProvider,
};
use super::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::run_contract::RunBudgetPolicy;
use crate::ai_runtime::{
    FunctionCall, LlmMessage, MessageRole, TokenUsage, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::AppResult;

struct TurnRecordingProvider {
    responses: Mutex<VecDeque<GatewayResponse>>,
    turns: Mutex<Vec<Vec<LlmMessage>>>,
}

impl TurnRecordingProvider {
    fn new(responses: Vec<GatewayResponse>) -> Self {
        Self {
            responses: Mutex::new(VecDeque::from(responses)),
            turns: Mutex::new(Vec::new()),
        }
    }

    fn host_instructions(&self) -> Vec<String> {
        let turns = self.turns.lock().expect("turns lock");
        let mut seen: Vec<String> = Vec::new();
        for messages in turns.iter() {
            for message in messages {
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
}

impl ToolLoopProvider for TurnRecordingProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        messages: &'a [LlmMessage],
        _tools: &'a [ToolSpec],
        _budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        self.turns
            .lock()
            .expect("turns lock")
            .push(messages.to_vec());
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

struct CountingExecutor {
    calls: AtomicU32,
}

impl ToolLoopExecutor for CountingExecutor {
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
                output: serde_json::json!({ "ok": true }),
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

fn web_search_spec() -> ToolSpec {
    ToolSpec {
        name: "web_search".into(),
        description: "Search the web".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "query": {"type": "string"},
                "limit": {"type": "integer"}
            },
            "required": ["query"]
        }),
        access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }
}

fn enum_mode_spec() -> ToolSpec {
    ToolSpec {
        name: "search_hybrid".into(),
        description: "Search with a declared mode".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "properties": {
                "mode": {"type": "string", "enum": ["fast", "thorough"]}
            },
            "required": ["mode"]
        }),
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
        usage: TokenUsage {
            prompt_tokens: 0,
            completion_tokens: 0,
            total_tokens: 0,
            ..Default::default()
        },
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

async fn run_rejected_proposal(run_id: &str, spec: ToolSpec, call: ToolCall) -> (Vec<String>, u32) {
    let provider = TurnRecordingProvider::new(vec![tool_response(call), final_response("ok")]);
    let executor = CountingExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;
    // Process-global model-turn ledger is keyed by run_id. These four tests
    // execute in parallel; a shared id lets one BindGuard unbind while another
    // is still claiming, which surfaces as ToolLoopLimit instead of feedback.
    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            run_id,
            Vec::new(),
            vec![spec],
            &mut observer,
        )
        .await
        .expect("a rejected proposal must not fail the Run");
    (
        provider.host_instructions(),
        executor.calls.load(Ordering::SeqCst),
    )
}

#[tokio::test]
async fn schema_mismatch_feedback_includes_missing_required_field() {
    let (instructions, dispatched) = run_rejected_proposal(
        "run-schema-mismatch-missing-query",
        web_search_spec(),
        tool_call("c1", "web_search", serde_json::json!({"limit": 3})),
    )
    .await;
    let feedback = instructions.join("\n");
    assert_eq!(
        dispatched, 0,
        "invalid arguments must not reach the executor"
    );
    assert!(
        feedback.contains("arguments_schema_mismatch"),
        "stable rejection code must remain, got {instructions:#?}"
    );
    assert!(
        feedback.contains("arguments.query is required"),
        "Host feedback must name the missing required field, got {instructions:#?}"
    );
    assert!(
        feedback.contains("web_search"),
        "legal tool name must still be listed, got {instructions:#?}"
    );
}

#[tokio::test]
async fn schema_mismatch_feedback_includes_expected_type() {
    let (instructions, dispatched) = run_rejected_proposal(
        "run-schema-mismatch-wrong-type",
        web_search_spec(),
        tool_call("c1", "web_search", serde_json::json!({"query": 7})),
    )
    .await;
    let feedback = instructions.join("\n");
    assert_eq!(dispatched, 0);
    assert!(feedback.contains("arguments_schema_mismatch"));
    assert!(
        feedback.contains("arguments.query must be a string"),
        "Host feedback must name the allowed type, got {instructions:#?}"
    );
}

#[tokio::test]
async fn schema_mismatch_feedback_lists_enum_values() {
    let (instructions, dispatched) = run_rejected_proposal(
        "run-schema-mismatch-enum",
        enum_mode_spec(),
        tool_call("c1", "search_hybrid", serde_json::json!({"mode": "slow"})),
    )
    .await;
    let feedback = instructions.join("\n");
    assert_eq!(dispatched, 0);
    assert!(feedback.contains("arguments_schema_mismatch"));
    assert!(
        feedback.contains("arguments.mode is outside the declared enum [fast, thorough]"),
        "Host feedback must list allowed enum values, got {instructions:#?}"
    );
}

#[tokio::test]
async fn unknown_tool_feedback_does_not_invent_schema_field_errors() {
    let (instructions, dispatched) = run_rejected_proposal(
        "run-schema-mismatch-unknown-tool",
        web_search_spec(),
        tool_call("c1", "not_exposed", serde_json::json!({})),
    )
    .await;
    let feedback = instructions.join("\n");
    assert_eq!(dispatched, 0);
    assert!(
        feedback.contains("not_exposed:rejected_before_dispatch"),
        "unknown tools keep the stable rejection prefix, got {instructions:#?}"
    );
    assert!(
        !feedback.contains("arguments.query is required"),
        "unknown-tool feedback must not invent a field error, got {instructions:#?}"
    );
    assert!(
        feedback.contains("web_search"),
        "legal tool name must still be listed, got {instructions:#?}"
    );
}
