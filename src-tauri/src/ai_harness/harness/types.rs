//! Harness public types.

use serde::Serialize;

use crate::ai_runtime::deliberation::{DeliberationState, VerificationSummary};
use crate::ai_runtime::model_gateway::{TokenUsage, ToolCall};
use crate::ai_runtime::{
    agent_task_policy::AgentTaskPolicy, retrieval_scope::ContextScopeDto, AiScene, ContextPacket,
    RuntimeDocumentSnapshot, SkillActivationPlanSummary,
};

use super::token_estimator::UsageSource;

/// Harness progress phase for structured UI.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessPhase {
    ToolStart,
    ToolComplete,
    SubagentSpawn,
    SubagentComplete,
    Reflection,
    FinalStream,
    Thinking,
}

/// Harness progress event for the frontend.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessTraceEvent {
    pub request_id: String,
    pub round: u32,
    pub phase: HarnessPhase,
    pub tool_name: String,
    pub status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub duration_ms: Option<u64>,
    pub message: Option<String>,
    pub output_preview: Option<String>,
}

/// Result of a harness run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HarnessFinishReason {
    Completed,
    AwaitingConfirmation,
    BudgetExhausted,
    RoundLimit,
}

/// Result of a harness run.
#[derive(Debug, Clone, Serialize)]
pub struct HarnessRunResult {
    pub request_id: String,
    pub session_id: i64,
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub tool_results: Vec<serde_json::Value>,
    pub usage: TokenUsage,
    pub citation_valid: bool,
    pub harness_rounds: u32,
    pub pending_confirmation: bool,
    pub evidence_packets: Vec<ContextPacket>,
    pub usage_source: UsageSource,
    pub finish_reason: HarnessFinishReason,
    pub deliberation_state: Option<DeliberationState>,
    pub verification_summary: Option<VerificationSummary>,
}

/// Inputs for a harness run.
#[derive(Debug, Clone)]
pub struct HarnessRunInput {
    pub request_id: String,
    pub scene: AiScene,
    pub session_id: i64,
    pub note_path: Option<String>,
    pub note_title: Option<String>,
    pub selection_excerpt: Option<String>,
    pub cold_start_packets: Vec<ContextPacket>,
    pub context_scope: ContextScopeDto,
    pub runtime_documents: Vec<RuntimeDocumentSnapshot>,
    pub web_search_enabled: bool,
    pub user_message: String,
    /// 图片附件（传给 LLM API 的多模态内容）。
    pub images: Option<Vec<crate::commands::ai_commands::ImageAttachmentDto>>,
    pub history_messages: Vec<(String, String)>,
    pub depth: u32,
    pub resume_from_checkpoint: bool,
    pub max_rounds_override: Option<u32>,
    pub token_budget: Option<u32>,
    pub input_budget: Option<u32>,
    pub skill_activation_plan: Option<SkillActivationPlanSummary>,
    pub task_policy: AgentTaskPolicy,
}
