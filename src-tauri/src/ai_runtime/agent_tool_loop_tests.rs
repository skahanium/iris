use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_capacity_eval::EvaluationTelemetryTap;
use super::agent_tool_loop::{
    AgentModelTurnBudget, AgentToolLoop, ToolLoopExecutor, ToolLoopProvider,
};
use super::model_gateway::{StreamEventObserver, StreamSurface};
use crate::ai_runtime::run_contract::{RunBudgetPolicy, RunBudgetProfile};
use crate::ai_runtime::{
    FunctionCall, LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::AppResult;

fn standard_tool_loop() -> AgentToolLoop {
    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
}

struct ScriptedProvider {
    responses: Mutex<VecDeque<super::model_gateway::GatewayResponse>>,
    calls: AtomicU32,
    second_turn_messages: Mutex<Vec<LlmMessage>>,
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

struct FailingWebExecutor;
struct LargeResultExecutor;
struct LargeWebResultExecutor;
struct OversizedWebResultExecutor;
struct RequiredWebExecutor;

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
        schema_version: 1,
        profile: RunBudgetProfile::Direct,
        max_model_turns: 1,
        max_tool_calls: 0,
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
        schema_version: 1,
        profile: RunBudgetProfile::Delegated,
        max_model_turns: 8,
        max_tool_calls: 24,
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
            input_token_budget: Some(2_000),
            max_output_tokens: Some(1_024),
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
        schema_version: 1,
        profile: RunBudgetProfile::Delegated,
        max_model_turns: 8,
        max_tool_calls: 24,
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
