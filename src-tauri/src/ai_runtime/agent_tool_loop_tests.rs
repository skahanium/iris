use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_capacity_eval::EvaluationTelemetryTap;
use super::agent_tool_loop::{
    is_natural_clarification, AgentModelTurnBudget, AgentToolLoop, ToolLoopExecutor,
    ToolLoopProvider,
};
use super::model_gateway::{StreamEventObserver, StreamSurface};
use crate::ai_runtime::run_contract::{RunBudgetPolicy, RunBudgetProfile};
use crate::ai_runtime::{
    FunctionCall, LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::{AppError, AppResult};

fn standard_tool_loop() -> AgentToolLoop {
    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
}

#[test]
fn standard_turn_reserves_output_before_selecting_history() {
    let budget = RunBudgetPolicy::standard();

    assert_eq!(budget.max_prompt_tokens, 128_000);
    assert_eq!(budget.max_completion_tokens, 16_000);
    assert_eq!(budget.max_turn_output_tokens, 4_000);
}

#[test]
fn direct_provider_has_an_explicit_nonzero_turn_budget() {
    assert_ne!(AgentModelTurnBudget::default().max_prompt_tokens, None);
}

#[tokio::test]
async fn parent_turn_reuses_one_frozen_budget_for_every_provider_call() {
    let provider = MultiTurnBudgetRecordingProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        budgets: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            "run-frozen-parent-budget",
            Vec::new(),
            vec![ToolSpec {
                name: "system_time_now".into(),
                description: "Get time".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect("two turns complete with one immutable budget");

    assert_eq!(
        provider.budgets.lock().expect("budget lock").as_slice(),
        [
            AgentModelTurnBudget {
                max_prompt_tokens: Some(128_000),
                max_completion_tokens: Some(16_000),
                max_turn_output_tokens: Some(4_000),
            },
            AgentModelTurnBudget {
                max_prompt_tokens: Some(128_000),
                max_completion_tokens: Some(16_000),
                max_turn_output_tokens: Some(4_000),
            },
        ]
    );
}

#[tokio::test]
async fn unexposed_tool_call_is_rejected_without_reaching_executor() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "call-not-exposed".into(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: "not_exposed".into(),
                        arguments: "{}".into(),
                    },
                }],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-unexposed-tool",
            Vec::new(),
            vec![ToolSpec {
                name: "system_time_now".into(),
                description: "Get time".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect("unexposed tool call must be rejected and the loop must finish");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "an unexposed tool call must never reach the executor"
    );
}

#[tokio::test]
async fn network_category_cap_rejects_the_second_dispatch_without_calling_executor() {
    let provider = ToolSurfaceRecordingProvider {
        responses: Mutex::new(VecDeque::from([
            scripted_tool_response(tool_call_with_arguments(
                "network-1",
                "web_search",
                serde_json::json!({ "query": "first" }),
            )),
            scripted_tool_response(tool_call_with_arguments(
                "network-2",
                "web_search",
                serde_json::json!({ "query": "second" }),
            )),
            scripted_final_response("bounded final answer"),
        ])),
        surfaces: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let policy = RunBudgetPolicy {
        schema_version: 2,
        profile: RunBudgetProfile::Standard,
        max_prompt_tokens: 64_000,
        max_completion_tokens: 8_000,
        max_turn_output_tokens: 4_000,
        max_model_turns: 4,
        max_tool_calls: 24,
        max_local_tool_calls: 12,
        max_network_tool_calls: 1,
        max_external_read_tool_calls: 6,
        max_runtime_tool_calls: 4,
        max_confirmed_change_calls: 6,
        max_child_runs: 0,
        child_max_model_turns: 0,
        child_max_tool_calls: 0,
        child_input_tokens_per_turn: 0,
        child_output_tokens_per_turn: 0,
        post_confirmation_max_model_turns: 0,
    };
    let mut observer = NoopObserver;

    let outcome = AgentToolLoop::from_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-network-cap",
            Vec::new(),
            vec![web_tool_spec()],
            &mut observer,
        )
        .await
        .expect("a category rejection leaves room for final synthesis");

    assert_eq!(outcome.content, "bounded final answer");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn frozen_category_caps_reject_the_first_call_beyond_each_boundary() {
    for (name, exposed_tool, allowed_dispatches) in [
        (
            "search_keyword",
            readonly_tool_spec("search_keyword"),
            12_u32,
        ),
        ("web_search", web_tool_spec(), 6),
        (
            "fs_read_authorized_folder",
            readonly_tool_spec("fs_read_authorized_folder"),
            6,
        ),
        ("system_time_now", readonly_tool_spec("system_time_now"), 4),
        (
            "insert_text_at_cursor",
            readonly_tool_spec("insert_text_at_cursor"),
            6,
        ),
        (
            "unknown_frozen_external_read",
            readonly_tool_spec("unknown_frozen_external_read"),
            6,
        ),
    ] {
        let calls = (0..=allowed_dispatches)
            .map(|index| {
                tool_call_with_arguments(
                    &format!("{name}-{index}"),
                    name,
                    serde_json::json!({ "query": format!("attempt-{index}") }),
                )
            })
            .collect();
        let provider = ToolSurfaceRecordingProvider {
            responses: Mutex::new(VecDeque::from([
                scripted_tool_calls_response(calls),
                scripted_final_response("bounded synthesis"),
            ])),
            surfaces: Mutex::new(Vec::new()),
        };
        let executor = RecordingExecutor {
            calls: AtomicU32::new(0),
            web_evidence: false,
        };
        let mut observer = NoopObserver;

        let outcome = standard_tool_loop()
            .execute(
                &provider,
                &executor,
                &format!("run-category-{name}"),
                Vec::new(),
                vec![exposed_tool],
                &mut observer,
            )
            .await
            .expect("a rejected boundary call must leave room for synthesis");

        assert_eq!(outcome.content, "bounded synthesis", "{name}");
        assert_eq!(
            executor.calls.load(Ordering::SeqCst),
            allowed_dispatches,
            "{name} must reject its first call beyond the frozen category cap"
        );
    }
}

#[tokio::test]
async fn two_complete_rounds_without_progress_close_tools_before_final_synthesis() {
    let provider = ToolSurfaceRecordingProvider {
        responses: Mutex::new(VecDeque::from([
            scripted_tool_response(tool_call_with_arguments(
                "runtime-1",
                "system_time_now",
                serde_json::json!({ "step": 1 }),
            )),
            scripted_tool_response(tool_call_with_arguments(
                "runtime-2",
                "system_time_now",
                serde_json::json!({ "step": 2 }),
            )),
            scripted_final_response("synthesized from bounded results"),
        ])),
        surfaces: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-no-progress-synthesis",
            Vec::new(),
            vec![readonly_tool_spec("system_time_now")],
            &mut observer,
        )
        .await
        .expect("two no-progress rounds must reserve a final synthesis turn");

    assert_eq!(outcome.content, "synthesized from bounded results");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        provider
            .surfaces
            .lock()
            .expect("tool surfaces lock")
            .as_slice(),
        [
            vec!["system_time_now".to_string()],
            vec!["system_time_now".to_string()],
            vec![]
        ]
    );
}

#[tokio::test]
async fn final_model_turn_is_reserved_after_progressing_tool_rounds() {
    let provider = ToolSurfaceRecordingProvider {
        responses: Mutex::new(VecDeque::from([
            scripted_tool_response(tool_call_with_arguments(
                "web-1",
                "web_search",
                serde_json::json!({ "query": "first direction" }),
            )),
            scripted_tool_response(tool_call_with_arguments(
                "web-2",
                "web_search",
                serde_json::json!({ "query": "second direction" }),
            )),
            scripted_final_response("synthesized after bounded exploration"),
        ])),
        surfaces: Mutex::new(Vec::new()),
    };
    let executor = ResourceTracingExecutor {
        calls: Mutex::new(Vec::new()),
    };
    let policy = RunBudgetPolicy {
        schema_version: 2,
        profile: RunBudgetProfile::Standard,
        max_prompt_tokens: 64_000,
        max_completion_tokens: 8_000,
        max_turn_output_tokens: 4_000,
        max_model_turns: 3,
        max_tool_calls: 24,
        max_local_tool_calls: 12,
        max_network_tool_calls: 6,
        max_external_read_tool_calls: 6,
        max_runtime_tool_calls: 4,
        max_confirmed_change_calls: 6,
        max_child_runs: 0,
        child_max_model_turns: 0,
        child_max_tool_calls: 0,
        child_input_tokens_per_turn: 0,
        child_output_tokens_per_turn: 0,
        post_confirmation_max_model_turns: 0,
    };
    let mut observer = NoopObserver;

    let outcome = AgentToolLoop::from_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-reserved-final-turn",
            Vec::new(),
            vec![web_tool_spec()],
            &mut observer,
        )
        .await
        .expect("the final model turn must remain available for synthesis");

    assert_eq!(outcome.content, "synthesized after bounded exploration");
    assert_eq!(executor.calls.lock().expect("trace calls lock").len(), 2);
    assert_eq!(
        provider
            .surfaces
            .lock()
            .expect("tool surfaces lock")
            .as_slice(),
        [
            vec!["web_search".to_string()],
            vec!["web_search".to_string()],
            vec![]
        ]
    );
}

#[tokio::test]
async fn one_generic_loop_can_mix_network_and_local_reads_before_synthesizing() {
    let provider = ToolSurfaceRecordingProvider {
        responses: Mutex::new(VecDeque::from([
            scripted_tool_response(tool_call_with_arguments(
                "web-1",
                "web_search",
                serde_json::json!({ "query": "public context" }),
            )),
            scripted_tool_response(tool_call_with_arguments(
                "local-1",
                "search_hybrid",
                serde_json::json!({ "query": "note context" }),
            )),
            scripted_final_response("combined answer"),
        ])),
        surfaces: Mutex::new(Vec::new()),
    };
    let executor = ResourceTracingExecutor {
        calls: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-mixed-tool-loop",
            Vec::new(),
            vec![web_tool_spec(), readonly_tool_spec("search_hybrid")],
            &mut observer,
        )
        .await
        .expect("one generic loop must compose authorized network and local reads");

    assert_eq!(outcome.content, "combined answer");
    let calls = executor.calls.lock().expect("trace calls lock");
    assert_eq!(
        calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
        ["web_search", "search_hybrid"]
    );
}

#[tokio::test]
async fn missing_provider_usage_is_estimated_from_the_local_turn_data() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([super::model_gateway::GatewayResponse {
            content: Some("本地估算的回答".into()),
            tool_calls: Vec::new(),
            usage: Default::default(),
            finish_reason: "stop".into(),
            reasoning_content: None,
            continuation: None,
        }])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-estimated-usage",
            vec![LlmMessage {
                role: MessageRole::User,
                content: "请给出一个简短回答".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            Vec::new(),
            &mut observer,
        )
        .await
        .expect("missing usage still yields a bounded outcome");

    assert!(outcome.prompt_tokens > 0);
    assert!(outcome.completion_tokens > 0);
    assert_eq!(
        outcome.total_tokens,
        outcome.prompt_tokens + outcome.completion_tokens
    );
}

struct ScriptedProvider {
    responses: Mutex<VecDeque<super::model_gateway::GatewayResponse>>,
    calls: AtomicU32,
    second_turn_messages: Mutex<Vec<LlmMessage>>,
}

struct ToolSurfaceRecordingProvider {
    responses: Mutex<VecDeque<super::model_gateway::GatewayResponse>>,
    surfaces: Mutex<Vec<Vec<String>>>,
}

impl ToolLoopProvider for ToolSurfaceRecordingProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        _budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<super::model_gateway::GatewayResponse>> + Send + 'a>>
    {
        self.surfaces
            .lock()
            .expect("tool surfaces lock")
            .push(tools.iter().map(|tool| tool.name.clone()).collect());
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

struct MultiTurnBudgetRecordingProvider {
    responses: Mutex<VecDeque<super::model_gateway::GatewayResponse>>,
    budgets: Mutex<Vec<AgentModelTurnBudget>>,
}

impl ToolLoopProvider for MultiTurnBudgetRecordingProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [LlmMessage],
        _tools: &'a [ToolSpec],
        budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<super::model_gateway::GatewayResponse>> + Send + 'a>>
    {
        self.budgets.lock().expect("budget lock").push(budget);
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

impl ToolLoopProvider for ScriptedProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        messages: &'a [LlmMessage],
        _tools: &'a [ToolSpec],
        _budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<super::model_gateway::GatewayResponse>> + Send + 'a>>
    {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if self.calls.load(Ordering::SeqCst) == 2 {
            *self
                .second_turn_messages
                .lock()
                .expect("second turn messages lock") = messages.to_vec();
        }
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

struct RecordingExecutor {
    calls: AtomicU32,
    web_evidence: bool,
}

struct ResourceTracingExecutor {
    calls: Mutex<Vec<(String, String, String)>>,
}

struct RepeatedFailureExecutor {
    calls: AtomicU32,
}

struct FailingWebExecutor;
struct LargeResultExecutor;
struct LargeWebResultExecutor;
struct OversizedWebResultExecutor;
struct RequiredWebExecutor;
struct RequiredExternalExecutor;

impl ToolLoopExecutor for RequiredWebExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        _call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async { unreachable!("a required Web Run must not finalize before a call") })
    }

    fn requires_web_evidence(&self) -> bool {
        true
    }
}

impl ToolLoopExecutor for RequiredExternalExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        _call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async { unreachable!("external evidence is required before finalization") })
    }

    fn requires_external_evidence(&self) -> bool {
        true
    }
}

impl ToolLoopExecutor for LargeResultExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "body": "x".repeat(8_500) }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}

impl ToolLoopExecutor for LargeWebResultExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({
                    "evidence": "x".repeat(20_000),
                    "sentinel": "web-evidence-tail-must-survive",
                }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}

impl ToolLoopExecutor for OversizedWebResultExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "evidence": "x".repeat(40_000) }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}

impl ToolLoopExecutor for FailingWebExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: false,
                output: serde_json::json!({ "error": "agent_run_web_provider_timeout" }),
                duration_ms: 1,
                tokens_used: None,
                error: Some("agent_run_web_provider_timeout".to_string()),
            })
        })
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
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "answer": "result" }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }

    fn has_web_evidence(&self) -> bool {
        self.web_evidence && self.calls.load(Ordering::SeqCst) > 0
    }
}

impl ToolLoopExecutor for ResourceTracingExecutor {
    fn execute<'a>(
        &'a self,
        _run_id: &'a str,
        call: &'a ToolCall,
        _step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        let query_or_path = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
            .ok()
            .and_then(|arguments| {
                ["query", "path"].into_iter().find_map(|key| {
                    arguments
                        .get(key)
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned)
                })
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        let resource_id = format!("{}:{query_or_path}", call.function.name);
        self.calls.lock().expect("trace calls lock").push((
            call.function.name.clone(),
            query_or_path,
            resource_id.clone(),
        ));
        let tool_name = call.function.name.clone();
        Box::pin(async move {
            Ok(ToolCallResult {
                tool_name,
                success: true,
                output: serde_json::json!({ "resource_id": resource_id }),
                duration_ms: 1,
                tokens_used: None,
                error: None,
            })
        })
    }
}

impl ToolLoopExecutor for RepeatedFailureExecutor {
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
                output: serde_json::json!({ "error": "transient test failure" }),
                duration_ms: 1,
                tokens_used: None,
                error: Some("transient test failure".into()),
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

struct VisibleDraftObserver {
    content: String,
}

impl StreamEventObserver for VisibleDraftObserver {
    fn observe(
        &mut self,
        event: &super::model_gateway::StreamEvent,
        _token_index: u32,
    ) -> AppResult<()> {
        if let super::model_gateway::StreamEventData::Token { token, .. } = &event.data {
            self.content.push_str(token);
        }
        Ok(())
    }

    fn has_visible_content(&self) -> bool {
        !self.content.is_empty()
    }

    fn visible_content_snapshot(&self) -> Option<String> {
        (!self.content.trim().is_empty()).then_some(self.content.clone())
    }
}

struct InterruptedThenRecoveryProvider {
    calls: AtomicU32,
    recovery_tools: Mutex<Vec<ToolSpec>>,
    recovery_messages: Mutex<Vec<LlmMessage>>,
    recovery_attempts_tool_call: bool,
    visible_draft: String,
    continuation: String,
}

impl ToolLoopProvider for InterruptedThenRecoveryProvider {
    fn answer_turn<'a>(
        &'a self,
        run_id: &'a str,
        messages: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        _budget: AgentModelTurnBudget,
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<super::model_gateway::GatewayResponse>> + Send + 'a>>
    {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        if call == 0 {
            return Box::pin(async move {
                observer.observe(
                    &super::model_gateway::StreamEvent {
                        request_id: run_id.to_string(),
                        event_type: super::model_gateway::StreamEventType::Token,
                        data: super::model_gateway::StreamEventData::Token {
                            token: self.visible_draft.clone(),
                            replace_visible: false,
                        },
                        surface: StreamSurface::VisibleAnswerSanitized,
                        classified: false,
                    },
                    1,
                )?;
                Err(AppError::StreamInterrupted(
                    "upstream connection closed".into(),
                ))
            });
        }
        self.recovery_tools
            .lock()
            .expect("recovery tools lock")
            .extend_from_slice(tools);
        *self
            .recovery_messages
            .lock()
            .expect("recovery messages lock") = messages.to_vec();
        Box::pin(async {
            Ok(super::model_gateway::GatewayResponse {
                content: Some(self.continuation.clone()),
                tool_calls: self
                    .recovery_attempts_tool_call
                    .then(tool_call)
                    .into_iter()
                    .collect(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            })
        })
    }
}

#[tokio::test]
async fn partial_visible_stream_error_recovers_once_with_same_provider_and_no_tools() {
    let provider = InterruptedThenRecoveryProvider {
        calls: AtomicU32::new(0),
        recovery_tools: Mutex::new(Vec::new()),
        recovery_messages: Mutex::new(Vec::new()),
        recovery_attempts_tool_call: false,
        visible_draft: "已经露出的开头，".into(),
        continuation: "这是同一回答的续写，现已完整。".into(),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = VisibleDraftObserver {
        content: String::new(),
    };
    let original_tools = vec![ToolSpec {
        name: "web_search".into(),
        description: "Search Web".into(),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::Network,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-visible-stream-recovery",
            vec![LlmMessage {
                role: MessageRole::User,
                content: "给我一份完整回答".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            original_tools,
            &mut observer,
        )
        .await
        .expect("one append-only recovery must complete the answer");

    assert_eq!(
        outcome.content,
        "已经露出的开头，\n\n这是同一回答的续写，现已完整。"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(
        outcome.completion_tokens,
        crate::ai_runtime::text_support::estimate_tokens("已经露出的开头，") as u32
            + crate::ai_runtime::text_support::estimate_tokens("这是同一回答的续写，现已完整。")
                as u32,
        "the recovery budget includes the visible draft exactly once"
    );
    assert!(provider
        .recovery_tools
        .lock()
        .expect("recovery tools lock")
        .is_empty());
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    let recovery_messages = provider
        .recovery_messages
        .lock()
        .expect("recovery messages lock");
    assert!(recovery_messages.iter().any(|message| {
        matches!(message.role, MessageRole::System)
            && message
                .content
                .text_content()
                .contains("only the missing continuation")
    }));
    assert!(recovery_messages
        .iter()
        .all(|message| message.reasoning_content.is_none()));
}

#[tokio::test]
async fn partial_visible_stream_recovery_rejects_a_business_tool_call() {
    let provider = InterruptedThenRecoveryProvider {
        calls: AtomicU32::new(0),
        recovery_tools: Mutex::new(Vec::new()),
        recovery_messages: Mutex::new(Vec::new()),
        recovery_attempts_tool_call: true,
        visible_draft: "已经露出的开头，".into(),
        continuation: "这是同一回答的续写，现已完整。".into(),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = VisibleDraftObserver {
        content: String::new(),
    };

    let error = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-visible-stream-recovery-tool-call",
            Vec::new(),
            vec![ToolSpec {
                name: "system_time_now".into(),
                description: "Get time".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect_err("an append-only recovery may not reopen the tool surface");

    assert_eq!(error.to_string(), "agent_run_incomplete_output");
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn interrupted_visible_drafts_count_against_per_turn_and_cumulative_budgets() {
    const LIMIT: usize = 8;

    for (draft_tokens, expected_error) in [
        (LIMIT - 1, None),
        (LIMIT, Some("agent_run_tool_loop_limit")),
        (LIMIT + 1, Some("agent_run_output_too_long")),
    ] {
        let provider = InterruptedThenRecoveryProvider {
            calls: AtomicU32::new(0),
            recovery_tools: Mutex::new(Vec::new()),
            recovery_messages: Mutex::new(Vec::new()),
            recovery_attempts_tool_call: false,
            visible_draft: "答".repeat(draft_tokens),
            continuation: "续".into(),
        };
        let executor = RecordingExecutor {
            calls: AtomicU32::new(0),
            web_evidence: false,
        };
        let mut observer = VisibleDraftObserver {
            content: String::new(),
        };
        let policy = RunBudgetPolicy {
            schema_version: 2,
            profile: RunBudgetProfile::Standard,
            max_prompt_tokens: 1_000,
            max_completion_tokens: LIMIT as u32,
            max_turn_output_tokens: LIMIT as u32,
            max_model_turns: 2,
            max_tool_calls: 0,
            max_local_tool_calls: 0,
            max_network_tool_calls: 0,
            max_external_read_tool_calls: 0,
            max_runtime_tool_calls: 0,
            max_confirmed_change_calls: 0,
            max_child_runs: 0,
            child_max_model_turns: 0,
            child_max_tool_calls: 0,
            child_input_tokens_per_turn: 0,
            child_output_tokens_per_turn: 0,
            post_confirmation_max_model_turns: 0,
        };

        let result = AgentToolLoop::from_policy(&policy)
            .execute(
                &provider,
                &executor,
                "run-interrupted-draft-budget",
                Vec::new(),
                Vec::new(),
                &mut observer,
            )
            .await;

        match expected_error {
            None => {
                let outcome =
                    result.expect("N-1 visible draft leaves exactly one completion token");
                assert_eq!(outcome.completion_tokens, LIMIT as u32);
                assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
            }
            Some(expected_error) => {
                assert_eq!(
                    result
                        .expect_err("N and N+1 visible drafts must not be resumed over budget")
                        .to_string(),
                    expected_error
                );
                assert_eq!(
                    provider.calls.load(Ordering::SeqCst),
                    1,
                    "over-budget recovery must not call the provider again"
                );
            }
        }
    }
}

fn tool_call() -> ToolCall {
    ToolCall {
        id: "call-1".into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: "system_time_now".into(),
            arguments: "{}".into(),
        },
    }
}

fn web_tool_call() -> ToolCall {
    ToolCall {
        id: "call-web-search".into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: "web_search".into(),
            arguments: r#"{"query":"latest status"}"#.into(),
        },
    }
}

fn tool_call_with_arguments(id: &str, name: &str, arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: id.into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: name.into(),
            arguments: arguments.to_string(),
        },
    }
}

fn scripted_tool_response(call: ToolCall) -> super::model_gateway::GatewayResponse {
    scripted_tool_calls_response(vec![call])
}

fn scripted_tool_calls_response(calls: Vec<ToolCall>) -> super::model_gateway::GatewayResponse {
    super::model_gateway::GatewayResponse {
        content: None,
        tool_calls: calls,
        usage: Default::default(),
        finish_reason: "tool_calls".into(),
        reasoning_content: None,
        continuation: None,
    }
}

fn scripted_final_response(content: &str) -> super::model_gateway::GatewayResponse {
    super::model_gateway::GatewayResponse {
        content: Some(content.into()),
        tool_calls: Vec::new(),
        usage: Default::default(),
        finish_reason: "stop".into(),
        reasoning_content: None,
        continuation: None,
    }
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

fn web_tool_spec() -> ToolSpec {
    ToolSpec {
        access_level: crate::ai_runtime::ToolAccessLevel::Network,
        ..readonly_tool_spec("web_search")
    }
}

fn final_answer_tool_call(arguments: serde_json::Value) -> ToolCall {
    ToolCall {
        id: "call-submit-final-answer".into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: "submit_final_answer".into(),
            arguments: arguments.to_string(),
        },
    }
}

fn final_answer_tool_spec() -> ToolSpec {
    ToolSpec {
        name: "submit_final_answer".into(),
        description: "Submit the final answer with source bindings".into(),
        input_schema: serde_json::json!({
            "type": "object",
            "required": ["blocks"],
            "properties": {
                "blocks": {
                    "type": "array",
                    "items": {
                        "type": "object",
                        "required": ["markdown", "sources"],
                        "properties": {
                            "markdown": { "type": "string" },
                            "sources": {
                                "type": "array",
                                "items": { "type": "string" }
                            }
                        }
                    }
                }
            }
        }),
        access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }
}

#[tokio::test]
async fn internal_final_answer_submission_bypasses_executor_history_and_tool_budget() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![final_answer_tool_call(serde_json::json!({
                    "blocks": [
                        { "markdown": "第一段。", "sources": ["W1"] },
                        { "markdown": "分析上可能如此。", "sources": ["W1", "I"] }
                    ]
                }))],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: Some("private reasoning must not enter the transcript".into()),
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("the loop must not request a second turn".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-final-answer",
            Vec::new(),
            vec![final_answer_tool_spec()],
            &mut observer,
        )
        .await
        .expect("internal final answer submission");

    assert_eq!(outcome.content, "第一段。\n\n分析上可能如此。");
    assert_eq!(outcome.tool_calls, 0);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert!(provider
        .second_turn_messages
        .lock()
        .expect("second turn messages lock")
        .is_empty());
}

#[tokio::test]
async fn final_submission_retries_one_withheld_plain_draft() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: Some("不应展示的普通草稿。".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: Some("private".into()),
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![final_answer_tool_call(serde_json::json!({
                    "blocks": [{ "markdown": "已提交。", "sources": ["W1"] }]
                }))],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-final-repair",
            Vec::new(),
            vec![final_answer_tool_spec()],
            &mut observer,
        )
        .await
        .expect("one repair submits the final answer");

    assert_eq!(outcome.content, "已提交。");
    assert_eq!(outcome.tool_calls, 0);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert!(provider
        .second_turn_messages
        .lock()
        .expect("repair messages lock")
        .iter()
        .all(|message| message.reasoning_content.is_none()));
}

#[test]
fn final_submission_rejects_model_authored_web_markers() {
    for markdown in ["模型手写的来源。[W99]", "模型手写的来源。[w99]"] {
        let call = final_answer_tool_call(serde_json::json!({
            "blocks": [{ "markdown": markdown, "sources": ["W1"] }]
        }));

        assert!(
            super::final_answer_submission::FinalAnswerSubmission::from_tool_call(&call).is_err()
        );
    }
}

#[tokio::test]
async fn tool_loop_returns_tool_results_to_the_next_model_turn_before_finalizing() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;
    let initial_messages = vec![LlmMessage {
        role: MessageRole::User,
        content: "what time is it".into(),
        tool_call_id: None,
        tool_calls: None,
        reasoning_content: None,
    }];
    let tools = vec![ToolSpec {
        name: "system_time_now".into(),
        description: "Get time".into(),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-1",
            initial_messages,
            tools,
            &mut observer,
        )
        .await
        .expect("tool loop result");

    assert_eq!(outcome.content, "final answer");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    let messages = provider
        .second_turn_messages
        .lock()
        .expect("second turn messages lock");
    assert!(messages.iter().any(|message| {
        matches!(message.role, MessageRole::Assistant)
            && message
                .tool_calls
                .as_ref()
                .is_some_and(|calls| calls.len() == 1)
    }));
    assert!(messages.iter().any(|message| {
        matches!(message.role, MessageRole::Tool)
            && message.tool_call_id.as_deref() == Some("call-1")
            && message.content.text_content().contains("result")
    }));
    let _ = StreamSurface::VisibleAnswer;
}

#[tokio::test]
async fn successful_equivalent_tool_call_is_not_executed_twice() {
    let mut repeated_call = tool_call();
    repeated_call.id = "call-2".into();
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![repeated_call],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-successful-tool-dedup",
            Vec::new(),
            vec![ToolSpec {
                name: "system_time_now".into(),
                description: "Get time".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect("the model can finalize after a duplicate call is rejected");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn hr1_adaptive_search_accepts_a_refined_query_with_a_new_resource() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call_with_arguments(
                    "search-wide",
                    "web_search",
                    serde_json::json!({ "query": "组织治理" }),
                )],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call_with_arguments(
                    "search-refined",
                    "web_search",
                    serde_json::json!({ "query": "组织治理 入门 读者" }),
                )],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("已基于第二轮的新资料完成回答。".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = ResourceTracingExecutor {
        calls: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-hr1-adaptive-search",
            Vec::new(),
            vec![web_tool_spec()],
            &mut observer,
        )
        .await
        .expect("a refined search with a new resource must remain answerable");

    let calls = executor.calls.lock().expect("trace calls lock");
    assert_eq!(calls.len(), 2);
    assert_eq!(calls[0].0, "web_search");
    assert_eq!(calls[1].0, "web_search");
    assert_ne!(calls[0].1, calls[1].1, "the second query must be refined");
    assert_ne!(
        calls[0].2, calls[1].2,
        "the refinement must discover a new resource"
    );
    assert_eq!(outcome.content, "已基于第二轮的新资料完成回答。");
}

#[tokio::test]
async fn hr1_local_multi_hop_reads_distinct_notes_without_web_access() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call_with_arguments(
                    "search-local",
                    "search_keyword",
                    serde_json::json!({ "query": "责任认定" }),
                )],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call_with_arguments(
                    "read-first",
                    "read_note",
                    serde_json::json!({ "path": "notes/first.md" }),
                )],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call_with_arguments(
                    "read-second",
                    "read_note",
                    serde_json::json!({ "path": "notes/second.md" }),
                )],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("已综合两份本地笔记。".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = ResourceTracingExecutor {
        calls: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-hr1-local-multi-hop",
            Vec::new(),
            vec![
                readonly_tool_spec("search_keyword"),
                readonly_tool_spec("read_note"),
            ],
            &mut observer,
        )
        .await
        .expect("local multi-hop research must remain answerable");

    let calls = executor.calls.lock().expect("trace calls lock");
    assert_eq!(
        calls.iter().map(|call| call.0.as_str()).collect::<Vec<_>>(),
        ["search_keyword", "read_note", "read_note"]
    );
    assert_ne!(
        calls[1].2, calls[2].2,
        "each note must retain its own identity"
    );
    assert!(calls.iter().all(|call| call.0 != "web_search"));
    assert_eq!(outcome.content, "已综合两份本地笔记。");
}

#[tokio::test]
async fn hr1_repeated_failed_tool_call_stops_after_two_real_executions() {
    let repeated_call = |id| {
        tool_call_with_arguments(
            id,
            "web_search",
            serde_json::json!({ "query": "same failing query" }),
        )
    };
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![repeated_call("failed-first")],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![repeated_call("failed-second")],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![repeated_call("failed-third")],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("工具暂不可用，已说明限制。".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RepeatedFailureExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-hr1-repeated-failure",
            Vec::new(),
            vec![web_tool_spec()],
            &mut observer,
        )
        .await
        .expect("the model should still be able to summarize the failed lookup");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 2);
    assert_eq!(provider.calls.load(Ordering::SeqCst), 4);
    assert_eq!(outcome.content, "工具暂不可用，已说明限制。");
}

#[tokio::test]
async fn malformed_spawn_subagent_arguments_reach_the_bounded_executor() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "spawn-invalid-json".into(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: "spawn_subagent".into(),
                        arguments: "{".into(),
                    },
                }],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("handled invalid child request".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;

    standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-invalid-child-json",
            Vec::new(),
            vec![ToolSpec {
                name: "spawn_subagent".into(),
                description: "Run bounded child".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect("the executor normalizes malformed child arguments");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        1,
        "spawn_subagent owns its structured parse-error report"
    );
}

#[tokio::test]
async fn online_mode_accepts_a_direct_answer_without_forcing_web_search() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([super::model_gateway::GatewayResponse {
            content: Some("stable knowledge answer".into()),
            tool_calls: Vec::new(),
            usage: Default::default(),
            finish_reason: "stop".into(),
            reasoning_content: None,
            continuation: None,
        }])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;
    let tools = vec![ToolSpec {
        name: "web_search".into(),
        description: "Search Web".into(),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::Network,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-1",
            vec![LlmMessage {
                role: MessageRole::User,
                content: "explain recursion".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            tools,
            &mut observer,
        )
        .await
        .expect("online mode may answer without searching");

    assert_eq!(outcome.content, "stable knowledge answer");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn web_required_rejects_a_final_answer_without_registered_evidence() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([super::model_gateway::GatewayResponse {
            content: Some("unverified answer".into()),
            tool_calls: Vec::new(),
            usage: Default::default(),
            finish_reason: "stop".into(),
            reasoning_content: None,
            continuation: None,
        }])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;
    let error = standard_tool_loop()
        .execute(
            &provider,
            &RequiredWebExecutor,
            "run-required-web",
            Vec::new(),
            Vec::new(),
            &mut observer,
        )
        .await
        .expect_err("web-required must not silently finalize");
    assert_eq!(error.to_string(), "agent_run_web_evidence_required");
}

#[test]
fn natural_clarification_is_a_single_source_free_question_only() {
    assert!(is_natural_clarification(
        "为了查询附近场次，请告诉我所在的城市或地区？"
    ));
    assert!(is_natural_clarification("Which city should I use?"));
    assert!(!is_natural_clarification(
        "上海今晚有多个场次，你在哪个城市？"
    ));
    assert!(!is_natural_clarification("请看 [W1] 后告诉我城市？"));
    assert!(!is_natural_clarification(
        "请看 https://example.test 后告诉我城市？"
    ));
}

#[tokio::test]
async fn web_required_accepts_a_natural_clarification_before_any_tool_dispatch() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([super::model_gateway::GatewayResponse {
            content: Some("为了查询附近场次，请告诉我所在的城市或地区？".into()),
            tool_calls: Vec::new(),
            usage: Default::default(),
            finish_reason: "stop".into(),
            reasoning_content: None,
            continuation: None,
        }])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;
    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &RequiredWebExecutor,
            "run-required-web-clarification",
            Vec::new(),
            Vec::new(),
            &mut observer,
        )
        .await
        .expect("a natural clarification must complete without fabricated evidence");

    assert_eq!(
        outcome.content,
        "为了查询附近场次，请告诉我所在的城市或地区？"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn external_required_rejects_a_final_answer_without_registered_evidence() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([super::model_gateway::GatewayResponse {
            content: Some("unverified external answer".into()),
            tool_calls: Vec::new(),
            usage: Default::default(),
            finish_reason: "stop".into(),
            reasoning_content: None,
            continuation: None,
        }])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;
    let error = standard_tool_loop()
        .execute(
            &provider,
            &RequiredExternalExecutor,
            "run-required-external",
            Vec::new(),
            Vec::new(),
            &mut observer,
        )
        .await
        .expect_err("external-required must not silently finalize");
    assert_eq!(error.to_string(), "agent_run_external_evidence_required");
}

#[tokio::test]
async fn cancelled_run_never_starts_a_model_or_tool_turn() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::new()),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let mut observer = NoopObserver;
    crate::ai_runtime::model_gateway::request_abort("run-cancelled-loop");
    let result = standard_tool_loop()
        .execute(
            &provider,
            &executor,
            "run-cancelled-loop",
            Vec::new(),
            Vec::new(),
            &mut observer,
        )
        .await;
    crate::ai_runtime::model_gateway::clear_abort("run-cancelled-loop");

    assert_eq!(
        result.expect_err("cancelled loop must stop").to_string(),
        "agent_run_cancelled"
    );
    assert_eq!(provider.calls.load(Ordering::SeqCst), 0);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn online_mode_continues_after_a_failed_web_tool_with_the_model_answer() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![web_tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some(
                    "I could not verify the current status because Web timed out. Please retry."
                        .into(),
                ),
                tool_calls: vec![],
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;
    let tools = vec![ToolSpec {
        name: "web_search".into(),
        description: "Search Web".into(),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::Network,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];

    let outcome = standard_tool_loop()
        .execute(
            &provider,
            &FailingWebExecutor,
            "run-1",
            vec![LlmMessage {
                role: MessageRole::User,
                content: "latest status".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            tools,
            &mut observer,
        )
        .await
        .expect("online mode continues after web tool failure");

    assert!(outcome.content.contains("could not verify"));
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    let messages = provider
        .second_turn_messages
        .lock()
        .expect("second turn messages lock");
    assert!(messages.iter().any(|message| {
        matches!(message.role, MessageRole::Tool)
            && message
                .content
                .text_content()
                .contains("agent_run_web_provider_timeout")
    }));
}

#[tokio::test]
async fn evaluation_tool_loop_tap_records_turns_usage_tools_and_truncation_in_memory() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call()],
                usage: crate::ai_types::TokenUsage {
                    prompt_tokens: 10,
                    completion_tokens: 3,
                    total_tokens: 13,
                    prompt_cache_hit_tokens: 0,
                    prompt_cache_miss_tokens: 10,
                },
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: crate::ai_types::TokenUsage {
                    prompt_tokens: 12,
                    completion_tokens: 5,
                    total_tokens: 17,
                    prompt_cache_hit_tokens: 4,
                    prompt_cache_miss_tokens: 8,
                },
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let telemetry = EvaluationTelemetryTap::default();
    let mut observer = NoopObserver;
    let tools = vec![ToolSpec {
        name: "system_time_now".into(),
        description: "Get time".into(),
        input_schema: serde_json::json!({ "type": "object" }),
        access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];

    standard_tool_loop()
        .execute_with_eval_telemetry(
            &provider,
            &LargeResultExecutor,
            "run-eval",
            vec![LlmMessage {
                role: MessageRole::User,
                content: "synthetic".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            tools,
            &mut observer,
            &telemetry,
        )
        .await
        .expect("evaluation loop");

    let snapshot = telemetry.snapshot();
    assert_eq!(snapshot.model_turns(), 2);
    assert_eq!(snapshot.tool_calls(), 1);
    assert_eq!(snapshot.total_tokens(), 30);
    assert_eq!(snapshot.tool_result_truncations(), 1);
}

#[tokio::test]
async fn web_tool_results_use_the_web_specific_budget_without_losing_the_tail() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![web_tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;
    standard_tool_loop()
        .execute(
            &provider,
            &LargeWebResultExecutor,
            "run-web-result-budget",
            Vec::new(),
            vec![ToolSpec {
                name: "web_search".into(),
                description: "Search Web".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::Network,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect("web tool loop result");

    let messages = provider
        .second_turn_messages
        .lock()
        .expect("second turn messages lock");
    assert!(messages.iter().any(|message| {
        matches!(message.role, MessageRole::Tool)
            && message
                .content
                .text_content()
                .contains("web-evidence-tail-must-survive")
    }));
}

#[tokio::test]
async fn oversized_web_tool_results_fail_closed_with_valid_json() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![web_tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("I cannot verify this from the returned evidence.".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let mut observer = NoopObserver;
    standard_tool_loop()
        .execute(
            &provider,
            &OversizedWebResultExecutor,
            "run-web-result-overflow",
            Vec::new(),
            vec![ToolSpec {
                name: "web_search".into(),
                description: "Search Web".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::Network,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect("overflow is presented as a valid failed tool result");

    let messages = provider
        .second_turn_messages
        .lock()
        .expect("second turn messages lock");
    let tool_payload = messages
        .iter()
        .find(|message| matches!(message.role, MessageRole::Tool))
        .expect("tool result")
        .content
        .text_content();
    let parsed: serde_json::Value = serde_json::from_str(&tool_payload).expect("valid JSON packet");
    assert_eq!(parsed["success"], false);
    assert_eq!(parsed["error"], "web_evidence_pack_overflow");
}

#[tokio::test]
async fn from_policy_preserves_the_direct_one_model_zero_tool_budget() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([super::model_gateway::GatewayResponse {
            content: None,
            tool_calls: vec![tool_call()],
            usage: Default::default(),
            finish_reason: "tool_calls".into(),
            reasoning_content: None,
            continuation: None,
        }])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let policy = RunBudgetPolicy {
        schema_version: 2,
        profile: RunBudgetProfile::Direct,
        max_prompt_tokens: 64_000,
        max_completion_tokens: 8_000,
        max_turn_output_tokens: 8_000,
        max_model_turns: 1,
        max_tool_calls: 0,
        max_local_tool_calls: 0,
        max_network_tool_calls: 0,
        max_external_read_tool_calls: 0,
        max_runtime_tool_calls: 0,
        max_confirmed_change_calls: 0,
        max_child_runs: 0,
        child_max_model_turns: 0,
        child_max_tool_calls: 0,
        child_input_tokens_per_turn: 0,
        child_output_tokens_per_turn: 0,
        post_confirmation_max_model_turns: 0,
    };
    let mut observer = NoopObserver;

    let error = AgentToolLoop::from_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-direct-budget",
            Vec::new(),
            vec![ToolSpec {
                name: "system_time_now".into(),
                description: "Get time".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect_err("a direct policy must reject every tool call");

    assert_eq!(error.to_string(), "agent_run_tool_loop_limit");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
}

struct BudgetRecordingProvider {
    budgets: Mutex<Vec<AgentModelTurnBudget>>,
}

impl ToolLoopProvider for BudgetRecordingProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [LlmMessage],
        _tools: &'a [ToolSpec],
        budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<super::model_gateway::GatewayResponse>> + Send + 'a>>
    {
        self.budgets.lock().expect("budget lock").push(budget);
        Box::pin(async {
            Ok(super::model_gateway::GatewayResponse {
                content: Some("child answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
                continuation: None,
            })
        })
    }
}

#[tokio::test]
async fn child_policy_reaches_every_provider_turn_with_gateway_token_limits() {
    let provider = BudgetRecordingProvider {
        budgets: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let policy = RunBudgetPolicy {
        schema_version: 2,
        profile: RunBudgetProfile::Delegated,
        max_prompt_tokens: 96_000,
        max_completion_tokens: 12_000,
        max_turn_output_tokens: 4_000,
        max_model_turns: 8,
        max_tool_calls: 24,
        max_local_tool_calls: 12,
        max_network_tool_calls: 6,
        max_external_read_tool_calls: 6,
        max_runtime_tool_calls: 4,
        max_confirmed_change_calls: 6,
        max_child_runs: 3,
        child_max_model_turns: 2,
        child_max_tool_calls: 6,
        child_input_tokens_per_turn: 2_000,
        child_output_tokens_per_turn: 1_024,
        post_confirmation_max_model_turns: 0,
    };
    let mut observer = NoopObserver;

    AgentToolLoop::from_child_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-child-budget",
            Vec::new(),
            Vec::new(),
            &mut observer,
        )
        .await
        .expect("child loop result");

    assert_eq!(
        provider.budgets.lock().expect("budget lock").as_slice(),
        [AgentModelTurnBudget {
            max_prompt_tokens: Some(2_000),
            max_completion_tokens: Some(2_048),
            max_turn_output_tokens: Some(1_024),
        }]
    );
}

#[tokio::test]
async fn child_policy_executes_six_tools_and_rejects_the_seventh() {
    let tool_calls = (0..6)
        .map(|index| ToolCall {
            id: format!("child-tool-{index}"),
            call_type: "function".into(),
            function: FunctionCall {
                name: "system_time_now".into(),
                arguments: serde_json::json!({ "index": index }).to_string(),
            },
        })
        .collect();
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls,
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![ToolCall {
                    id: "child-tool-6".into(),
                    call_type: "function".into(),
                    function: FunctionCall {
                        name: "system_time_now".into(),
                        arguments: serde_json::json!({ "index": 6 }).to_string(),
                    },
                }],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
                continuation: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: false,
    };
    let policy = RunBudgetPolicy {
        schema_version: 2,
        profile: RunBudgetProfile::Delegated,
        max_prompt_tokens: 96_000,
        max_completion_tokens: 12_000,
        max_turn_output_tokens: 4_000,
        max_model_turns: 8,
        max_tool_calls: 24,
        max_local_tool_calls: 12,
        max_network_tool_calls: 6,
        max_external_read_tool_calls: 6,
        max_runtime_tool_calls: 4,
        max_confirmed_change_calls: 6,
        max_child_runs: 3,
        child_max_model_turns: 2,
        child_max_tool_calls: 6,
        child_input_tokens_per_turn: 2_000,
        child_output_tokens_per_turn: 1_024,
        post_confirmation_max_model_turns: 0,
    };
    let mut observer = NoopObserver;

    let error = AgentToolLoop::from_child_policy(&policy)
        .execute(
            &provider,
            &executor,
            "run-child-tool-budget",
            Vec::new(),
            vec![ToolSpec {
                name: "system_time_now".into(),
                description: "Get time".into(),
                input_schema: serde_json::json!({ "type": "object" }),
                access_level: crate::ai_runtime::ToolAccessLevel::ReadProfile,
                requires_confirmation: false,
                max_results: None,
                capability_affinity: Vec::new(),
            }],
            &mut observer,
        )
        .await
        .expect_err("the seventh child tool call must exceed the frozen budget");

    assert_eq!(error.to_string(), "agent_run_tool_loop_limit");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 2);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 6);
}
