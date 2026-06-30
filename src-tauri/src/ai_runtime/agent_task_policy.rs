//! Task-policy driven execution strategy for the Agent Task Runtime.
//!
//! `AiScene` remains a legacy compatibility hint for old sessions and trace
//! metadata. New execution budgets, slots, context strategy, and task focus are
//! derived from task facts captured in [`AgentTaskPolicyInput`].

use crate::ai_runtime::agent_task::AgentTaskKind;
use crate::ai_types::{AgentIntent, AiScene, AutonomyLevel, CapabilitySlot, ContextStrategy};
use crate::commands::assistant_commands::AssistantExecuteRequest;
use crate::error::AppResult;
use crate::llm::config::{
    resolve_capability_route, CapabilityRouteInput, PrivacyPreference, ResolvedCapabilityRoute,
};
use crate::storage::db::Database;
use serde::{Deserialize, Serialize};

/// Scope shape used by task policy. It is deliberately coarser than retrieval
/// filters so policy never needs note bodies or raw query text.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentTaskScope {
    Vault,
    Note,
    Selection,
}

/// Summary facts used to derive execution policy for a task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AgentTaskPolicyInput {
    pub intent: AgentIntent,
    pub task_kind: AgentTaskKind,
    pub scope: AgentTaskScope,
    pub web_authorized: bool,
    pub has_attachments: bool,
    pub write_permission_required: bool,
    pub research_depth: u32,
}

/// Runtime execution policy for a task.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentTaskPolicy {
    pub intent: AgentIntent,
    pub task_kind: AgentTaskKind,
    pub scope: AgentTaskScope,
    pub web_authorized: bool,
    pub has_attachments: bool,
    pub write_permission_required: bool,
    pub research_depth: u32,
    pub autonomy_level: AutonomyLevel,
    pub max_agentic_rounds: u32,
    pub max_tool_calls_per_round: u32,
    pub default_token_budget: u32,
    pub max_token_budget: u32,
    pub model_slot: CapabilitySlot,
    pub context_strategy: ContextStrategy,
    pub max_fetch_per_round: u32,
    /// Compatibility-only scene hint for old sessions, traces, and metadata.
    pub legacy_scene_hint: String,
}

impl AgentTaskPolicyInput {
    /// Derive task policy facts from the validated per-turn TaskPlan.
    pub fn from_task_plan(
        plan: &crate::ai_types::TaskPlanSummary,
        request: &AssistantExecuteRequest,
    ) -> Self {
        Self {
            intent: crate::ai_runtime::task_plan::agent_intent_for_task_plan(plan),
            task_kind: crate::ai_runtime::task_plan::task_kind_for_task_plan(plan),
            scope: crate::ai_runtime::task_plan::scope_for_task_plan(plan, request),
            web_authorized: request.web_authorized
                && matches!(plan.web_mode, crate::ai_types::WebMode::Brokered),
            has_attachments: request
                .images
                .as_ref()
                .is_some_and(|items| !items.is_empty()),
            write_permission_required:
                crate::ai_runtime::task_plan::write_permission_required_for_task_plan(plan),
            research_depth: crate::ai_runtime::task_plan::research_depth_for_task_plan(plan),
        }
    }
}

impl AgentTaskPolicy {
    /// Derive policy from task facts, not from a legacy scene profile.
    pub fn from_input(input: AgentTaskPolicyInput) -> Self {
        let model_slot = model_slot_for_input(input);
        let autonomy_level = autonomy_for_input(input);
        let max_agentic_rounds = max_rounds_for_input(input);
        let max_tool_calls_per_round = max_tools_for_input(input);
        let (default_token_budget, max_token_budget) = token_budget_for_input(input);
        let context_strategy = context_strategy_for_input(input, max_token_budget);
        let max_fetch_per_round = if !input.web_authorized {
            0
        } else if matches!(
            input.intent,
            AgentIntent::Research | AgentIntent::CitationCheck
        ) {
            2
        } else {
            1
        };
        let legacy_scene = legacy_scene(input.intent);

        Self {
            intent: input.intent,
            task_kind: input.task_kind,
            scope: input.scope,
            web_authorized: input.web_authorized,
            has_attachments: input.has_attachments,
            write_permission_required: input.write_permission_required,
            research_depth: input.research_depth,
            autonomy_level,
            max_agentic_rounds,
            max_tool_calls_per_round,
            default_token_budget,
            max_token_budget,
            model_slot,
            context_strategy,
            max_fetch_per_round,
            legacy_scene_hint: legacy_scene.profile().to_string(),
        }
    }

    /// Legacy scene retained for old session keys, traces, and tool metadata.
    pub fn legacy_scene(&self) -> AiScene {
        legacy_scene(self.intent)
    }

    /// Short prompt focus derived from task intent and scope.
    pub fn task_focus(&self) -> &'static str {
        task_focus(self.intent, self.scope, self.web_authorized)
    }
}

/// Resolve model routing for a task policy.
pub fn resolve_for_task_policy(
    db: &Database,
    policy: &AgentTaskPolicy,
) -> AppResult<ResolvedCapabilityRoute> {
    let context_tokens = if matches!(policy.model_slot, CapabilitySlot::LongContext) {
        policy.max_token_budget as usize
    } else {
        0
    };

    resolve_capability_route(
        db,
        CapabilityRouteInput {
            intent: policy.intent,
            context_tokens,
            has_images: matches!(policy.model_slot, CapabilitySlot::Vision),
            needs_tools: matches!(policy.model_slot, CapabilitySlot::AgentTools),
            needs_reasoning: matches!(policy.model_slot, CapabilitySlot::Reasoner),
            privacy_preference: PrivacyPreference::ExternalAllowed,
        },
    )
}

/// Task focus text used by prompt/persona and environment sections.
pub fn task_focus(
    intent: AgentIntent,
    scope: AgentTaskScope,
    web_authorized: bool,
) -> &'static str {
    match intent {
        AgentIntent::Research => {
            if web_authorized {
                "研究综合：结合本地证据与授权网络证据，标注缺口并形成可追溯结论"
            } else {
                "研究综合：基于本地证据进行多材料交叉论证，证据不足时直接说明"
            }
        }
        AgentIntent::CitationCheck => "引用核查：检查声明、证据覆盖与引用缺口",
        AgentIntent::RewriteSelection => "选区改写：只处理用户选区，写入前等待确认",
        AgentIntent::Write | AgentIntent::Chapter => {
            "文稿创作：围绕当前笔记生成或改写内容，写入前等待确认"
        }
        AgentIntent::DocumentCheck => "文档检查：面向整篇文档检查结构、风格、引用和交叉引用",
        AgentIntent::Organize => "知识整理：处理标题、标签、文件夹、链接和语料归属建议",
        AgentIntent::SkillManagement => "Skills 管理：创建或确认 prompt-only 技能",
        AgentIntent::VisionChat => "图像对话：结合附件与当前上下文回答，不编造图像外信息",
        AgentIntent::AskNotes => match scope {
            AgentTaskScope::Note | AgentTaskScope::Selection => "笔记问答：优先当前笔记与选中范围",
            AgentTaskScope::Vault => "知识查阅：检索本地知识库并基于证据回答",
        },
        AgentIntent::Chat => "轻量对话：保持简洁，仅在需要时读取本地上下文",
    }
}

/// Compatibility scene for old session keys and trace metadata.
pub fn legacy_scene(intent: AgentIntent) -> AiScene {
    match intent {
        AgentIntent::RewriteSelection
        | AgentIntent::Write
        | AgentIntent::Chapter
        | AgentIntent::DocumentCheck => AiScene::DraftingAssist,
        AgentIntent::Research | AgentIntent::CitationCheck => AiScene::ResearchSynthesis,
        AgentIntent::Chat
        | AgentIntent::AskNotes
        | AgentIntent::Organize
        | AgentIntent::VisionChat
        | AgentIntent::SkillManagement => AiScene::KnowledgeLookup,
    }
}

/// Convert legacy scene input into a task intent for migration-period IPC.
pub fn intent_from_legacy_scene(scene: AiScene) -> AgentIntent {
    match scene {
        AiScene::KnowledgeLookup => AgentIntent::AskNotes,
        AiScene::DraftingAssist => AgentIntent::Write,
        AiScene::ResearchSynthesis => AgentIntent::Research,
        _ => AgentIntent::Write,
    }
}

fn model_slot_for_input(input: AgentTaskPolicyInput) -> CapabilitySlot {
    if input.has_attachments || matches!(input.intent, AgentIntent::VisionChat) {
        return CapabilitySlot::Vision;
    }
    if matches!(input.intent, AgentIntent::SkillManagement) {
        return CapabilitySlot::AgentTools;
    }
    if input.research_depth > 1
        || matches!(
            input.intent,
            AgentIntent::Research | AgentIntent::CitationCheck
        )
    {
        return CapabilitySlot::Reasoner;
    }
    if matches!(input.intent, AgentIntent::DocumentCheck) {
        return CapabilitySlot::LongContext;
    }
    if input.write_permission_required
        || matches!(
            input.intent,
            AgentIntent::RewriteSelection | AgentIntent::Write | AgentIntent::Chapter
        )
    {
        return CapabilitySlot::Writer;
    }
    CapabilitySlot::Fast
}

fn autonomy_for_input(input: AgentTaskPolicyInput) -> AutonomyLevel {
    match input.intent {
        AgentIntent::Chat | AgentIntent::VisionChat => AutonomyLevel::L1,
        AgentIntent::Research | AgentIntent::CitationCheck => AutonomyLevel::L3,
        AgentIntent::AskNotes
        | AgentIntent::RewriteSelection
        | AgentIntent::Write
        | AgentIntent::Organize
        | AgentIntent::Chapter
        | AgentIntent::DocumentCheck
        | AgentIntent::SkillManagement => AutonomyLevel::L2,
    }
}

fn max_rounds_for_input(input: AgentTaskPolicyInput) -> u32 {
    let base = match input.intent {
        AgentIntent::Chat if input.web_authorized => 2,
        AgentIntent::Chat | AgentIntent::VisionChat => 1,
        AgentIntent::Research | AgentIntent::CitationCheck => 4,
        AgentIntent::DocumentCheck => 4,
        AgentIntent::AskNotes | AgentIntent::Write | AgentIntent::Chapter => 3,
        AgentIntent::RewriteSelection => 3,
        AgentIntent::Organize | AgentIntent::SkillManagement => 2,
    };
    if matches!(input.task_kind, AgentTaskKind::Complex) {
        base.max(3)
    } else {
        base
    }
}

fn max_tools_for_input(input: AgentTaskPolicyInput) -> u32 {
    match input.intent {
        AgentIntent::Research | AgentIntent::CitationCheck | AgentIntent::DocumentCheck => 6,
        AgentIntent::Write | AgentIntent::Chapter => 5,
        AgentIntent::Chat | AgentIntent::VisionChat => 2,
        _ => 4,
    }
}

fn token_budget_for_input(input: AgentTaskPolicyInput) -> (u32, u32) {
    match input.intent {
        AgentIntent::Chat if input.web_authorized => (80_000, 120_000),
        AgentIntent::Research | AgentIntent::CitationCheck => (100_000, 240_000),
        AgentIntent::DocumentCheck => (120_000, 240_000),
        AgentIntent::Write | AgentIntent::Chapter | AgentIntent::RewriteSelection => {
            (60_000, 160_000)
        }
        AgentIntent::VisionChat => (50_000, 120_000),
        AgentIntent::AskNotes | AgentIntent::Organize | AgentIntent::SkillManagement => {
            (30_000, 80_000)
        }
        AgentIntent::Chat => (20_000, 40_000),
    }
}

fn context_strategy_for_input(input: AgentTaskPolicyInput, _max_budget: u32) -> ContextStrategy {
    if matches!(input.intent, AgentIntent::DocumentCheck) {
        ContextStrategy::LongContext
    } else {
        ContextStrategy::Hybrid
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chat_policy(web_authorized: bool) -> AgentTaskPolicy {
        AgentTaskPolicy::from_input(AgentTaskPolicyInput {
            intent: crate::ai_types::AgentIntent::Chat,
            task_kind: AgentTaskKind::Lightweight,
            scope: AgentTaskScope::Vault,
            web_authorized,
            has_attachments: false,
            write_permission_required: false,
            research_depth: 0,
        })
    }

    #[test]
    fn web_authorized_chat_allows_a_tool_round_and_answer_round() {
        let policy = chat_policy(true);

        assert_eq!(policy.autonomy_level, crate::ai_types::AutonomyLevel::L1);
        assert_eq!(policy.max_agentic_rounds, 2);
        assert!(policy.default_token_budget >= 60_000);
        assert!(policy.max_token_budget >= policy.default_token_budget);
        assert_eq!(policy.max_fetch_per_round, 1);
    }

    #[test]
    fn offline_chat_keeps_single_round() {
        let policy = chat_policy(false);

        assert_eq!(policy.max_agentic_rounds, 1);
        assert_eq!(policy.max_fetch_per_round, 0);
    }
}
