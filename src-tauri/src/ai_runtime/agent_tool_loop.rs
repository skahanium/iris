//! Bounded, provider-neutral model/tool orchestration for one Agent Run.
//!
//! This module owns transcript integrity and loop limits only. Permission checks,
//! confirmation persistence, audit writes and the concrete tool dispatch remain in
//! the Run-bound executor supplied by the caller.

use std::collections::{HashMap, HashSet};
use std::future::Future;
use std::pin::Pin;

use crate::ai_runtime::model_gateway::{GatewayResponse, StreamEventObserver};
use crate::ai_runtime::run_engine::RunEventSink;
use crate::ai_runtime::{LlmMessage, MessageRole, ToolCall, ToolCallResult, ToolSpec};
use crate::error::{AppError, AppResult};
use crate::storage::db::Database;

const MAX_MODEL_TURNS: u32 = 8;
const MAX_TOOL_CALLS: u32 = 24;
const MAX_REPEAT_CALLS: u32 = 2;
const MAX_TOOL_RESULT_CHARS: usize = 8_000;

/// Result of a fully bounded model/tool exchange.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct AgentToolLoopOutcome {
    /// Final assistant content emitted only after the model has stopped calling tools.
    pub(crate) content: String,
    /// Number of model turns used by this Run.
    pub(crate) model_turns: u32,
    /// Number of concrete tool dispatch attempts made by this Run.
    pub(crate) tool_calls: u32,
}

/// Provider-facing side of a model/tool loop.
pub(crate) trait ToolLoopProvider: Send + Sync {
    /// Execute one model turn against the current canonical transcript.
    fn answer_turn<'a>(
        &'a self,
        run_id: &'a str,
        messages: &'a [LlmMessage],
        tools: &'a [ToolSpec],
        observer: &'a mut dyn StreamEventObserver,
    ) -> Pin<Box<dyn Future<Output = AppResult<GatewayResponse>> + Send + 'a>>;
}

/// Run-bound side of a tool loop.
pub(crate) trait ToolLoopExecutor: Send + Sync {
    /// Validate, authorize, audit and execute one model-requested tool call.
    fn execute<'a>(
        &'a self,
        run_id: &'a str,
        call: &'a ToolCall,
        step: u32,
    ) -> Pin<Box<dyn Future<Output = AppResult<ToolCallResult>> + Send + 'a>>;

    /// Evidence registered by this Run's tool calls for final-message binding.
    fn evidence_ids(&self) -> Vec<i64> {
        Vec::new()
    }

    /// Whether this Run has registered usable Web evidence (for deferred degradation).
    fn has_web_evidence(&self) -> bool {
        false
    }

    /// Emit a deferred Web degradation notice after the tool loop succeeds.
    /// Default executors have nothing to report.
    fn emit_deferred_web_degradation_if_needed(
        &self,
        _db: &Database,
        _sink: &dyn RunEventSink,
    ) -> AppResult<()> {
        Ok(())
    }
}

/// Executes the only permitted shape of an Agent tool loop.
#[derive(Debug, Clone, Copy)]
pub(crate) struct AgentToolLoop {
    max_model_turns: u32,
    max_tool_calls: u32,
}

impl Default for AgentToolLoop {
    fn default() -> Self {
        Self {
            max_model_turns: MAX_MODEL_TURNS,
            max_tool_calls: MAX_TOOL_CALLS,
        }
    }
}

impl AgentToolLoop {
    /// Run model turns until a non-empty final answer is received or a bound is reached.
    pub(crate) async fn execute(
        &self,
        provider: &impl ToolLoopProvider,
        executor: &impl ToolLoopExecutor,
        run_id: &str,
        mut messages: Vec<LlmMessage>,
        tools: Vec<ToolSpec>,
        observer: &mut dyn StreamEventObserver,
    ) -> AppResult<AgentToolLoopOutcome> {
        let allowed_tools = tools
            .iter()
            .map(|tool| tool.name.as_str())
            .collect::<HashSet<_>>();
        let mut model_turns = 0;
        let mut tool_calls = 0;
        let mut fingerprints = HashMap::<String, u32>::new();

        while model_turns < self.max_model_turns {
            model_turns += 1;
            let response = provider
                .answer_turn(run_id, &messages, &tools, observer)
                .await?;

            if response.tool_calls.is_empty() {
                let content = response.content.unwrap_or_default();
                if content.trim().is_empty() {
                    return Err(AppError::msg("agent_run_invalid_model_response"));
                }
                return Ok(AgentToolLoopOutcome {
                    content,
                    model_turns,
                    tool_calls,
                });
            }

            if tool_calls.saturating_add(response.tool_calls.len() as u32) > self.max_tool_calls {
                return Err(AppError::msg("agent_run_tool_loop_limit"));
            }

            messages.push(assistant_tool_message(&response));
            for call in &response.tool_calls {
                tool_calls += 1;
                let result = if !allowed_tools.contains(call.function.name.as_str()) {
                    rejected_result(call, "tool_not_in_run_surface")
                } else if !valid_call_arguments(call) {
                    rejected_result(call, "tool_arguments_invalid")
                } else {
                    let fingerprint = tool_fingerprint(call);
                    let count = fingerprints.entry(fingerprint).or_insert(0);
                    *count += 1;
                    if *count > MAX_REPEAT_CALLS {
                        rejected_result(call, "tool_call_repeated")
                    } else {
                        executor.execute(run_id, call, tool_calls).await?
                    }
                };
                messages.push(tool_result_message(call, &result));
            }
        }

        Err(AppError::msg("agent_run_tool_loop_limit"))
    }
}

fn assistant_tool_message(response: &GatewayResponse) -> LlmMessage {
    LlmMessage {
        role: MessageRole::Assistant,
        content: response.content.clone().unwrap_or_default().into(),
        tool_call_id: None,
        tool_calls: Some(response.tool_calls.clone()),
        reasoning_content: response.reasoning_content.clone(),
    }
}

fn tool_result_message(call: &ToolCall, result: &ToolCallResult) -> LlmMessage {
    let payload = serde_json::json!({
        "success": result.success,
        "output": result.output,
        "error": result.error,
    });
    let serialized = serde_json::to_string(&payload).unwrap_or_else(|_| {
        "{\"success\":false,\"error\":\"tool_result_serialization_failed\"}".into()
    });
    let content = truncate_chars(&serialized, MAX_TOOL_RESULT_CHARS);
    LlmMessage {
        role: MessageRole::Tool,
        content: content.into(),
        tool_call_id: Some(call.id.clone()),
        tool_calls: None,
        reasoning_content: None,
    }
}

fn valid_call_arguments(call: &ToolCall) -> bool {
    !call.id.trim().is_empty()
        && !call.function.name.trim().is_empty()
        && serde_json::from_str::<serde_json::Value>(&call.function.arguments)
            .is_ok_and(|value| value.is_object())
}

fn tool_fingerprint(call: &ToolCall) -> String {
    let arguments = serde_json::from_str::<serde_json::Value>(&call.function.arguments)
        .ok()
        .and_then(|value| canonical_json(&value))
        .unwrap_or_else(|| call.function.arguments.clone());
    format!("{}:{arguments}", call.function.name)
}

fn canonical_json(value: &serde_json::Value) -> Option<String> {
    match value {
        serde_json::Value::Object(map) => {
            let mut keys = map.keys().collect::<Vec<_>>();
            keys.sort_unstable();
            let values = keys
                .into_iter()
                .map(|key| Some(format!("{key}:{}", canonical_json(&map[key])?)))
                .collect::<Option<Vec<_>>>()?;
            Some(format!("{{{}}}", values.join(",")))
        }
        serde_json::Value::Array(values) => Some(format!(
            "[{}]",
            values
                .iter()
                .map(canonical_json)
                .collect::<Option<Vec<_>>>()?
                .join(",")
        )),
        _ => serde_json::to_string(value).ok(),
    }
}

fn rejected_result(call: &ToolCall, reason: &str) -> ToolCallResult {
    ToolCallResult {
        tool_name: call.function.name.clone(),
        success: false,
        output: serde_json::json!({ "error": reason }),
        duration_ms: 0,
        tokens_used: None,
        error: Some(reason.to_string()),
    }
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars();
    let prefix = chars.by_ref().take(max_chars).collect::<String>();
    if chars.next().is_some() {
        format!("{prefix}…")
    } else {
        prefix
    }
}
