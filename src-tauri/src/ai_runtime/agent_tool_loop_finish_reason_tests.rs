//! Finish-reason dispatch contracts for the Agent tool loop.
//!
//! Split out of `agent_tool_loop_tests.rs` so Q04／F03 cases do not grow that
//! file past `npm run size:check`.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_tool_loop::{
    AgentModelTurnBudget, AgentToolLoop, ToolLoopExecutor, ToolLoopProvider,
};
use super::model_gateway::StreamEventObserver;
use super::run_contract::RunBudgetPolicy;
use crate::ai_runtime::{FunctionCall, LlmMessage, ToolCall, ToolCallResult, ToolSpec};
use crate::error::AppResult;

struct ScriptedProvider {
    responses: Mutex<VecDeque<super::model_gateway::GatewayResponse>>,
}

impl ToolLoopProvider for ScriptedProvider {
    fn answer_turn<'a>(
        &'a self,
        _run_id: &'a str,
        _messages: &'a [LlmMessage],
        _tools: &'a [ToolSpec],
        _budget: AgentModelTurnBudget,
        _observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<super::model_gateway::GatewayResponse>> + Send + 'a>>
    {
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

fn tool_call(id: &str, name: &str, arguments: &str) -> ToolCall {
    ToolCall {
        id: id.into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: name.into(),
            arguments: arguments.into(),
        },
    }
}

fn readonly_tool(name: &str) -> ToolSpec {
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

fn web_search_tool() -> ToolSpec {
    ToolSpec {
        access_level: crate::ai_runtime::ToolAccessLevel::Network,
        ..readonly_tool("web_search")
    }
}

fn response(
    content: Option<&str>,
    tool_calls: Vec<ToolCall>,
    finish_reason: &str,
) -> super::model_gateway::GatewayResponse {
    super::model_gateway::GatewayResponse {
        content: content.map(str::to_string),
        tool_calls,
        usage: Default::default(),
        finish_reason: finish_reason.into(),
        ..Default::default()
    }
}

#[tokio::test]
async fn length_finish_reason_does_not_dispatch_truncated_tool_calls() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            response(
                Some("截断前的正文"),
                vec![tool_call(
                    "call-web-search",
                    "web_search",
                    r#"{"query":"latest status"}"#,
                )],
                "length",
            ),
            response(Some("完整回答。"), Vec::new(), "stop"),
        ])),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            "run-q04-length-no-dispatch",
            Vec::new(),
            vec![web_search_tool()],
            &mut observer,
        )
        .await
        .expect("truncated tools must recover instead of executing");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "length must not dispatch assembled tool calls"
    );
    assert!(
        outcome.content.contains("完整回答") || outcome.terminal.is_host_authored(),
        "recovery must continue or close with a Host limitation: {:?}",
        outcome
    );
}

#[tokio::test]
async fn tool_calls_finish_reason_still_dispatches() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            response(
                None,
                vec![tool_call("call-time", "system_time_now", "{}")],
                "tool_calls",
            ),
            response(Some("现在是测试时间。"), Vec::new(), "stop"),
        ])),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            "run-q04-tool-calls-dispatch",
            Vec::new(),
            vec![readonly_tool("system_time_now")],
            &mut observer,
        )
        .await
        .expect("complete tool_calls still dispatch");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn stop_finish_reason_with_tools_still_dispatches() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            response(
                None,
                vec![tool_call("call-time", "system_time_now", "{}")],
                "stop",
            ),
            response(Some("现在是测试时间。"), Vec::new(), "stop"),
        ])),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            "run-q04-stop-with-tools",
            Vec::new(),
            vec![readonly_tool("system_time_now")],
            &mut observer,
        )
        .await
        .expect("MiniMax-style stop with tools still dispatch");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unknown_finish_reason_does_not_dispatch_tool_calls() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            response(
                Some("缺终止块"),
                vec![tool_call(
                    "call-web-search",
                    "web_search",
                    r#"{"query":"latest status"}"#,
                )],
                "unknown",
            ),
            response(Some("完整回答。"), Vec::new(), "stop"),
        ])),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            "run-q04-unknown-no-dispatch",
            Vec::new(),
            vec![web_search_tool()],
            &mut observer,
        )
        .await
        .expect("unknown termination must recover instead of executing");

    assert_eq!(
        executor.calls.load(Ordering::SeqCst),
        0,
        "unknown must not dispatch assembled tool calls"
    );
}

#[tokio::test]
async fn empty_content_with_length_recovers_instead_of_invalid_response() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            response(None, Vec::new(), "length"),
            response(Some("完整回答。"), Vec::new(), "stop"),
        ])),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
    };
    let mut observer = NoopObserver;

    let outcome = AgentToolLoop::from_policy(&RunBudgetPolicy::standard())
        .execute(
            &provider,
            &executor,
            "run-q04-length-empty-content",
            Vec::new(),
            Vec::new(),
            &mut observer,
        )
        .await
        .expect("length with empty prose must recover, not agent_run_invalid_model_response");

    assert_eq!(executor.calls.load(Ordering::SeqCst), 0);
    assert!(
        outcome.content.contains("完整回答") || outcome.terminal.is_host_authored(),
        "empty length must continue or close with a Host limitation: {:?}",
        outcome
    );
}
