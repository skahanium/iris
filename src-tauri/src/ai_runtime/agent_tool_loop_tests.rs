use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

use super::agent_tool_loop::{AgentToolLoop, ToolLoopExecutor, ToolLoopProvider};
use super::model_gateway::{StreamEventObserver, StreamSurface};
use crate::ai_runtime::{
    FunctionCall, LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::AppResult;

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

    fn web_evidence_failure_code(
        &self,
    ) -> Option<crate::ai_runtime::run_contract::SafeRunErrorCode> {
        Some(crate::ai_runtime::run_contract::SafeRunErrorCode::WebProviderTimeout)
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
            },
            super::model_gateway::GatewayResponse {
                content: Some("final answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
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

    let outcome = AgentToolLoop::default()
        .execute(
            &provider,
            &executor,
            "run-1",
            initial_messages,
            tools,
            false,
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
async fn required_web_evidence_sends_one_correction_before_final_answer() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: Some("unsupported current claim".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
            },
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
            },
            super::model_gateway::GatewayResponse {
                content: Some("evidence-backed answer".into()),
                tool_calls: Vec::new(),
                usage: Default::default(),
                finish_reason: "stop".into(),
                reasoning_content: None,
            },
        ])),
        calls: AtomicU32::new(0),
        second_turn_messages: Mutex::new(Vec::new()),
    };
    let executor = RecordingExecutor {
        calls: AtomicU32::new(0),
        web_evidence: true,
    };
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

    let outcome = AgentToolLoop::default()
        .execute(
            &provider,
            &executor,
            "run-1",
            vec![LlmMessage {
                role: MessageRole::User,
                content: "latest status".into(),
                tool_call_id: None,
                tool_calls: None,
                reasoning_content: None,
            }],
            tools,
            true,
            &mut observer,
        )
        .await
        .expect("corrected tool loop result");

    assert_eq!(outcome.content, "evidence-backed answer");
    assert_eq!(provider.calls.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn failed_follow_up_web_tool_returns_a_structured_result_for_a_degraded_final_answer() {
    let provider = ScriptedProvider {
        responses: Mutex::new(VecDeque::from([
            super::model_gateway::GatewayResponse {
                content: None,
                tool_calls: vec![web_tool_call()],
                usage: Default::default(),
                finish_reason: "tool_calls".into(),
                reasoning_content: None,
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

    let outcome = AgentToolLoop::default()
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
            true,
            &mut observer,
        )
        .await
        .expect("a failed Web tool is recoverable");

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
