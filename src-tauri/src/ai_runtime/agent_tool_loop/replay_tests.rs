use crate::ai_runtime::agent_tool_loop::*;
use crate::ai_runtime::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::{
    LlmMessage, MessageRole, TokenUsage, ToolAccessLevel, ToolCall, ToolCallResult, ToolSpec,
};
use crate::error::AppResult;
use serde_json::json;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;

struct Provider {
    turns: AtomicU32,
    deny_replay: bool,
    failed_read: bool,
}
impl ToolLoopProvider for Provider {
    fn answer_turn<'a>(
        &'a self,
        _: &'a str,
        messages: &'a [LlmMessage],
        _: &'a [ToolSpec],
        _: AgentModelTurnBudget,
        _: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        Box::pin(async move {
            let turn = self.turns.fetch_add(1, Ordering::SeqCst);
            if turn == 2 {
                assert!(
                    messages.iter().any(|message| message
                        .content
                        .text_content()
                        .contains("\"compacted\":true")),
                    "production loop must compact before calling model"
                );
            }
            if turn == 3 {
                let last = messages
                    .iter()
                    .rfind(|message| matches!(message.role, MessageRole::Tool))
                    .unwrap();
                let payload: serde_json::Value =
                    serde_json::from_str(&last.content.text_content()).unwrap();
                assert_eq!(
                    payload["loopObservation"]["historicalObservation"],
                    !self.deny_replay
                );
                if self.deny_replay {
                    assert_eq!(payload["success"], false);
                    assert_eq!(payload["error"], "observation_replay_access_denied");
                } else {
                    assert_eq!(payload["output"]["content"], "a".repeat(6000));
                    assert_eq!(payload["success"], !self.failed_read);
                    if self.failed_read {
                        assert_eq!(payload["error"], "source_unavailable");
                    }
                }
            }
            Ok(GatewayResponse {
                content: (turn == 3).then(|| "recovered".into()),
                tool_calls: if turn < 3 {
                    vec![ToolCall::new(
                        format!("read-{turn}"),
                        "read_note",
                        json!({"path":if turn==1 {"b.md"}else{"a.md"}}).to_string(),
                    )]
                } else {
                    Vec::new()
                },
                usage: Default::default(),
                finish_reason: if turn < 3 { "tool_calls" } else { "stop" }.into(),
                ..Default::default()
            })
        })
    }
}
struct Executor {
    calls: AtomicU32,
    checks: AtomicU32,
    deny_replay: bool,
    failed_read: bool,
    oversized_metadata: bool,
    diagnostics: Mutex<Vec<serde_json::Value>>,
}
impl ToolLoopExecutor for Executor {
    fn record_tool_loop_diagnostic(&self, value: serde_json::Value) {
        self.diagnostics.lock().unwrap().push(value);
    }
    fn observation_replay_rejection(
        &self,
        _: &str,
        _: &ToolCall,
        _: u32,
    ) -> AppResult<Option<&'static str>> {
        self.checks.fetch_add(1, Ordering::SeqCst);
        Ok(self
            .deny_replay
            .then_some("observation_replay_access_denied"))
    }
    fn execute<'a>(
        &'a self,
        _: &'a str,
        call: &'a ToolCall,
        _: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async move {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let args: serde_json::Value = serde_json::from_str(&call.function.arguments).unwrap();
            let success = !(self.failed_read && args["path"] == "a.md");
            Ok(ToolCallResult {
                tool_name: "read_note".into(),
                success,
                error: (!success).then(|| "source_unavailable".into()),
                duration_ms: 1,
                tokens_used: None,
                output: if self.oversized_metadata {
                    json!({"metadata":"x".repeat(9000)})
                } else {
                    json!({"path":args["path"],"content":"a".repeat(6000),"contentHash":"same",
                    "sourceSpan":{"start":0,"end":6000},"nextStartByte":null})
                },
            })
        })
    }
}
struct Observer;
impl StreamEventObserver for Observer {
    fn observe(
        &mut self,
        _: &crate::ai_runtime::model_gateway::StreamEvent,
        _: u32,
    ) -> AppResult<()> {
        Ok(())
    }
}

#[tokio::test]
async fn plan_a_loop_replays_compacted_read_without_redispatch() {
    for (deny_replay, failed_read) in [(false, false), (true, false), (false, true)] {
        let provider = Provider {
            turns: AtomicU32::new(0),
            deny_replay,
            failed_read,
        };
        let executor = Executor {
            calls: AtomicU32::new(0),
            checks: AtomicU32::new(0),
            deny_replay,
            failed_read,
            oversized_metadata: false,
            diagnostics: Mutex::default(),
        };
        let mut policy = crate::ai_runtime::run_contract::RunBudgetPolicy::standard();
        policy.max_prompt_tokens = 2600;
        let tools = vec![ToolSpec {
            name: "read_note".into(),
            description: "Read".into(),
            input_schema: json!({"type":"object"}),
            access_level: ToolAccessLevel::ReadProfile,
            requires_confirmation: false,
            max_results: None,
            capability_affinity: Vec::new(),
        }];
        let result = AgentToolLoop::from_policy(&policy)
            .execute(
                &provider,
                &executor,
                "plan-a-replay",
                Vec::new(),
                tools,
                &mut Observer,
            )
            .await
            .unwrap();
        assert_eq!(result.content, "recovered");
        assert_eq!(result.tool_calls, 3, "replay consumes logical call budget");
        assert_eq!(
            executor.calls.load(Ordering::SeqCst),
            2,
            "only two real reads"
        );
        assert_eq!(
            executor.checks.load(Ordering::SeqCst),
            1,
            "replay must recheck access"
        );
        let diagnostics = executor.diagnostics.lock().unwrap();
        let progress = diagnostics
            .iter()
            .rev()
            .find(|item| item["event"] == "progress")
            .unwrap();
        if !deny_replay && !failed_read {
            assert_eq!(
                progress["noProgressRounds"], 1,
                "replay is not fresh progress"
            );
        }
        assert_eq!(
            diagnostics
                .iter()
                .filter(|item| item["event"] == "observation_replay")
                .count(),
            usize::from(!deny_replay)
        );
    }
}

#[tokio::test]
async fn plan_a_projection_overflow_closes_the_loop_with_a_visible_outcome() {
    let provider = Provider {
        turns: AtomicU32::new(0),
        deny_replay: false,
        failed_read: false,
    };
    let executor = Executor {
        calls: AtomicU32::new(0),
        checks: AtomicU32::new(0),
        deny_replay: false,
        failed_read: false,
        oversized_metadata: true,
        diagnostics: Mutex::default(),
    };
    let tools = vec![ToolSpec {
        name: "read_note".into(),
        description: "Read".into(),
        input_schema: json!({"type":"object"}),
        access_level: ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];
    let result =
        AgentToolLoop::from_policy(&crate::ai_runtime::run_contract::RunBudgetPolicy::standard())
            .execute(
                &provider,
                &executor,
                "plan-a-overflow",
                Vec::new(),
                tools,
                &mut Observer,
            )
            .await
            .unwrap();
    assert!(!result.content.is_empty());
    assert_eq!(result.tool_calls, 1);
    assert_eq!(executor.calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        provider.turns.load(Ordering::SeqCst),
        1,
        "never send malformed observation to another model turn"
    );
    assert!(executor
        .diagnostics
        .lock()
        .unwrap()
        .iter()
        .any(|item| item["event"] == "projection_limit"));
}

#[derive(Default)]
struct ChunkSearch {
    turns: AtomicU32,
    calls: AtomicU32,
}

impl ToolLoopProvider for ChunkSearch {
    fn answer_turn<'a>(
        &'a self,
        _: &'a str,
        _: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        _: AgentModelTurnBudget,
        _: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>> {
        Box::pin(async move {
            let turn = self.turns.fetch_add(1, Ordering::SeqCst);
            if turn < 4 {
                assert!(
                    tools.iter().any(|tool| tool.name == "search_keyword"),
                    "fresh chunks must not close the tool surface at turn {turn}"
                );
            }
            Ok(GatewayResponse {
                content: (turn == 4).then(|| "all four chunks inspected".into()),
                tool_calls: if turn < 4 {
                    vec![ToolCall::new(
                        format!("chunk-{turn}"),
                        "search_keyword",
                        json!({"query":format!("section {turn}")}).to_string(),
                    )]
                } else {
                    Vec::new()
                },
                usage: TokenUsage {
                    prompt_tokens: 1,
                    completion_tokens: 1,
                    total_tokens: 2,
                    ..Default::default()
                },
                finish_reason: if turn < 4 { "tool_calls" } else { "stop" }.into(),
                ..Default::default()
            })
        })
    }
}

impl ToolLoopExecutor for ChunkSearch {
    fn execute<'a>(
        &'a self,
        _: &'a str,
        call: &'a ToolCall,
        _: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>> {
        Box::pin(async move {
            let chunk = self.calls.fetch_add(1, Ordering::SeqCst);
            Ok(ToolCallResult {
                tool_name: call.function.name.clone(),
                success: true,
                output: json!({"results":[{"source_path":"notes/long.md",
                    "content_hash":"same-revision","source_span":{"start":chunk*3,"end":(chunk+1)*3},
                    "excerpt":"中"}]}),
                duration_ms: 0,
                tokens_used: None,
                error: None,
            })
        })
    }
}

#[tokio::test]
async fn plan_a_review_loop_keeps_tools_open_for_new_chunks_of_one_note() {
    let search = ChunkSearch::default();
    let tools = vec![ToolSpec {
        name: "search_keyword".into(),
        description: "Search note chunks".into(),
        input_schema: json!({"type":"object","properties":{"query":{"type":"string"}},"required":["query"]}),
        access_level: ToolAccessLevel::ReadProfile,
        requires_confirmation: false,
        max_results: None,
        capability_affinity: Vec::new(),
    }];
    let result =
        AgentToolLoop::from_policy(&crate::ai_runtime::run_contract::RunBudgetPolicy::standard())
            .execute(
                &search,
                &search,
                "plan-a-review-chunks",
                Vec::new(),
                tools,
                &mut Observer,
            )
            .await
            .unwrap();
    assert_eq!(result.content, "all four chunks inspected");
    assert_eq!(search.calls.load(Ordering::SeqCst), 4);
}
