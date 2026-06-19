//! Shared types for the Iris AI subsystem.
//!
//! This module owns all cross-cutting data types that are referenced by both
//! `ai_runtime` (business logic) and `llm` (infrastructure). Extracting them
//! here breaks the circular dependency that previously existed between those
//! two modules.
//!
//! `ai_runtime::mod` re-exports everything via `pub use crate::ai_types::*;`
//! so that existing call-sites remain unchanged.

use serde::{Deserialize, Serialize};

// ─── Scene ──────────────────────────────────────────────

/// AI 使用场景，对应前端场景选择器。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AiScene {
    /// 知识查阅 — 法规条款、笔记关联
    KnowledgeLookup,
    /// 文稿学习 — 范文结构、表达特征
    ExemplarLearning,
    /// 文稿创作 — 写作辅助
    DraftingAssist,
    /// 学术研究 — 多材料交叉论证
    ResearchSynthesis,
}

impl AiScene {
    /// Parse the stable IPC wire value without constructing ad-hoc JSON.
    pub fn parse_wire(value: &str) -> Option<Self> {
        match value.trim() {
            "knowledge_lookup" => Some(AiScene::KnowledgeLookup),
            "exemplar_learning" => Some(AiScene::ExemplarLearning),
            "drafting_assist" => Some(AiScene::DraftingAssist),
            "research_synthesis" => Some(AiScene::ResearchSynthesis),
            _ => None,
        }
    }

    /// 场景对应的默认自治等级。
    pub fn autonomy_level(&self) -> AutonomyLevel {
        match self {
            AiScene::KnowledgeLookup => AutonomyLevel::L1,
            AiScene::ExemplarLearning => AutonomyLevel::L1,
            AiScene::DraftingAssist => AutonomyLevel::L2,
            AiScene::ResearchSynthesis => AutonomyLevel::L3,
        }
    }

    /// 场景的 runtime profile 标识。
    pub fn profile(&self) -> &'static str {
        match self {
            AiScene::KnowledgeLookup => "knowledge_lookup",
            AiScene::ExemplarLearning => "exemplar_learning",
            AiScene::DraftingAssist => "drafting_assist",
            AiScene::ResearchSynthesis => "research_synthesis",
        }
    }

    /// 场景默认的会话范围是否为库级（不绑定笔记）。
    pub fn default_global_scope(&self) -> bool {
        matches!(self, AiScene::KnowledgeLookup | AiScene::ResearchSynthesis)
    }
}

// ─── Phase 2 Agent Intent ────────────────────────────────

/// User-facing Phase 2 assistant intent.
///
/// This replaces visible scene selection while keeping [`AiScene`] as an
/// internal compatibility layer for existing workflows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentIntent {
    /// General conversation with no stronger task signal.
    Chat,
    /// Ask questions over local notes or scoped vault context.
    AskNotes,
    /// Rewrite, summarize, translate, or otherwise transform selected text.
    RewriteSelection,
    /// Write or continue content in the current note without a fixed selection.
    Write,
    /// Multi-evidence research and synthesis.
    Research,
    /// Organize titles, tags, folders, links, or corpus membership.
    Organize,
    /// Check claims and citation coverage.
    CitationCheck,
    /// Chapter-level writing or restructuring.
    Chapter,
    /// Whole-document checks such as outline, citation gaps, or style.
    DocumentCheck,
    /// Image-aware chat path, with safe fallback to chat.
    VisionChat,
    /// Natural-language skill install, update, toggle, or diagnostic request.
    SkillManagement,
}

impl AgentIntent {
    /// Map legacy frontend assistant intent strings to Phase 2 intents.
    pub fn from_legacy_assistant_intent(value: &str) -> Self {
        match value {
            "knowledge" => AgentIntent::AskNotes,
            "writing" => AgentIntent::RewriteSelection,
            "citation" => AgentIntent::CitationCheck,
            "organize" => AgentIntent::Organize,
            "research" => AgentIntent::Research,
            "chapter" => AgentIntent::Chapter,
            "document" => AgentIntent::DocumentCheck,
            "chat" => AgentIntent::Chat,
            _ => AgentIntent::Chat,
        }
    }
}

/// Explainable intent detection metadata passed from the UI to the harness.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IntentDetectionSummary {
    pub detected_intent: AgentIntent,
    pub confidence: f64,
    pub reason: String,
    pub alternatives: Vec<AgentIntent>,
    pub fallback_behavior: String,
    pub source_hints: Vec<String>,
}

/// Skill manifest compatibility source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCompatibilitySource {
    Iris,
    Claude,
    Hermes,
    #[default]
    Unknown,
}

/// Runtime capability a skill may declare for preflight.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SkillRuntimeCapability {
    #[serde(rename = "skill.read_resource")]
    ReadResource,
    #[serde(rename = "skill.write_storage")]
    WriteStorage,
    #[serde(rename = "skill.request_capabilities")]
    RequestCapabilities,
    #[serde(rename = "skill.execute_script_sandboxed")]
    ExecuteScriptSandboxed,
    #[serde(rename = "skill.install_dependency")]
    InstallDependency,
    #[serde(rename = "skill.mcp_bridge")]
    McpBridge,
}

impl SkillRuntimeCapability {
    /// Parse a manifest capability string.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim() {
            "skill.read_resource" | "read_resource" => Some(Self::ReadResource),
            "skill.write_storage" | "write_storage" => Some(Self::WriteStorage),
            "skill.request_capabilities" | "request_capabilities" => {
                Some(Self::RequestCapabilities)
            }
            "skill.execute_script_sandboxed" | "execute_script_sandboxed" => {
                Some(Self::ExecuteScriptSandboxed)
            }
            "skill.install_dependency" | "install_dependency" => Some(Self::InstallDependency),
            "skill.mcp_bridge" | "mcp_bridge" => Some(Self::McpBridge),
            _ => None,
        }
    }

    /// Stable manifest string.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ReadResource => "skill.read_resource",
            Self::WriteStorage => "skill.write_storage",
            Self::RequestCapabilities => "skill.request_capabilities",
            Self::ExecuteScriptSandboxed => "skill.execute_script_sandboxed",
            Self::InstallDependency => "skill.install_dependency",
            Self::McpBridge => "skill.mcp_bridge",
        }
    }
}

/// Support status for a requested skill capability.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkillCapabilitySupportStatus {
    Supported,
    SupportedWithConfirmation,
    Planned,
    UnsupportedByProductScope,
    BlockedByPolicy,
    MissingUserGrant,
}

/// Resource metadata safe for UI and audit summaries.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillResourceStatusSummary {
    pub relative_path: String,
    pub kind: String,
    pub available: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size_bytes: Option<u64>,
    pub truncated: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Safe summary for a blocked or degraded skill capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedCapabilitySummary {
    pub skill_name: String,
    pub capability: String,
    pub status: SkillCapabilitySupportStatus,
    pub risk_level: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub permission: Option<ToolAccessLevel>,
    pub fallback_guidance: String,
}

/// Per-skill activation metadata safe for Run Plan display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationItemSummary {
    pub name: String,
    pub scope: String,
    pub score: f64,
    pub match_reason: String,
    pub injected_sections: Vec<String>,
    pub requested_tools: Vec<String>,
    pub requested_capabilities: Vec<SkillRuntimeCapability>,
    pub confirmation_required_tools: Vec<String>,
    pub resources: Vec<SkillResourceStatusSummary>,
    pub blocked_capabilities: Vec<BlockedCapabilitySummary>,
    pub compatibility_source: SkillCompatibilitySource,
    pub workspace_root: String,
    pub workspace_ready: bool,
    pub workspace_missing_items: Vec<String>,
}

/// Per-run skill activation plan safe for UI display.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillActivationPlanSummary {
    pub activated_skills: Vec<SkillActivationItemSummary>,
    pub requested_tools: Vec<String>,
    pub requested_capabilities: Vec<SkillRuntimeCapability>,
    pub confirmation_required_tools: Vec<String>,
    pub blocked_capabilities: Vec<BlockedCapabilitySummary>,
    pub skill_overlay_summary: String,
    pub degraded: bool,
}

impl SkillActivationPlanSummary {
    /// Tool allowlist produced by activated skills.
    pub fn allowed_tools(&self) -> Vec<String> {
        self.requested_tools.clone()
    }
}

/// Safe audit summary for Run Plan display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentAuditSummary {
    pub tool_events: u32,
    pub confirmed_tools: u32,
    pub denied_tools: u32,
    pub sanitized: bool,
}

/// Safe permission preflight metadata for the assistant response.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PermissionPreflightSummary {
    pub summary: String,
    pub required_confirmations: Vec<String>,
    pub blocked_capabilities: Vec<BlockedCapabilitySummary>,
    pub missing_user_grants: Vec<String>,
    pub exposed_tools: Vec<String>,
    pub degraded: bool,
}

/// Minimal run-plan summary that is safe for UI display.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunPlanSummary {
    pub request_id: String,
    pub detected_intent: AgentIntent,
    pub legacy_scene: AiScene,
    pub context_summary: Vec<String>,
    pub tool_summary: String,
    pub permission_summary: String,
    pub progress_state: String,
    pub blocked_reasons: Vec<String>,
    pub degraded: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_route: Option<CapabilityRouteSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub persona_layers: Vec<PersonaLayerSummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub skill_activation_plan: Option<SkillActivationPlanSummary>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub blocked_capabilities: Vec<BlockedCapabilitySummary>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub audit_summary: Option<AgentAuditSummary>,
}

impl AgentRunPlanSummary {
    /// Build a summary from metadata only; never include note bodies or secrets.
    pub fn for_intent(
        request_id: String,
        detected_intent: AgentIntent,
        legacy_scene: AiScene,
        context_summary: Vec<String>,
        tool_summary: String,
    ) -> Self {
        Self {
            request_id,
            detected_intent,
            legacy_scene,
            context_summary,
            tool_summary,
            permission_summary: "按当前 ToolPolicy 预检；需要确认的工具会暂停等待用户决定".into(),
            progress_state: "completed".into(),
            blocked_reasons: Vec::new(),
            degraded: false,
            model_route: None,
            persona_layers: Vec::new(),
            skill_activation_plan: None,
            blocked_capabilities: Vec::new(),
            audit_summary: None,
        }
    }

    /// Attach execution state derived from the harness result.
    pub fn with_execution_state(
        mut self,
        progress_state: impl Into<String>,
        permission_summary: String,
        blocked_reasons: Vec<String>,
        degraded: bool,
    ) -> Self {
        self.progress_state = progress_state.into();
        self.permission_summary = permission_summary;
        self.blocked_reasons = blocked_reasons;
        self.degraded = degraded;
        self
    }

    /// Attach the selected model route summary without exposing credentials.
    pub fn with_model_route(mut self, model_route: CapabilityRouteSummary) -> Self {
        self.model_route = Some(model_route);
        self
    }

    /// Attach prompt persona layer summaries without rendering sensitive prompt bodies.
    pub fn with_persona_layers(mut self, persona_layers: Vec<PersonaLayerSummary>) -> Self {
        self.persona_layers = persona_layers;
        self
    }

    /// Attach the per-run skill activation plan.
    pub fn with_skill_activation_plan(mut self, plan: SkillActivationPlanSummary) -> Self {
        self.degraded = self.degraded || plan.degraded;
        self.blocked_capabilities
            .extend(plan.blocked_capabilities.clone());
        self.skill_activation_plan = Some(plan);
        self
    }

    /// Attach blocked capability summaries not owned by one plan.
    pub fn with_blocked_capabilities(mut self, blocked: Vec<BlockedCapabilitySummary>) -> Self {
        self.degraded = self.degraded || !blocked.is_empty();
        self.blocked_capabilities.extend(blocked);
        self
    }

    /// Attach safe audit summary metadata.
    pub fn with_audit_summary(mut self, audit_summary: AgentAuditSummary) -> Self {
        self.audit_summary = Some(audit_summary);
        self
    }
}

/// Provider API family used by the model gateway adapter layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EndpointFamily {
    OpenAiCompatibleChatCompletions,
    AnthropicMessages,
    ResponsesReserved,
}

/// Capability probe strategy used by settings and run-plan summaries.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProbeStrategy {
    OpenAiModelsThenChat,
    AnthropicMessagesPing,
    StaticOnly,
}

/// Safe, explainable model routing metadata for the Run Plan UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CapabilityRouteSummary {
    pub slot: CapabilitySlot,
    pub provider_id: String,
    pub model: String,
    pub fallback_chain: Vec<CapabilitySlot>,
    pub reason: String,
    pub probe_status: String,
    pub degraded: bool,
}

/// Safe prompt-persona layer metadata for the Run Plan UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PersonaLayerSummary {
    pub layer: String,
    pub summary: String,
}

impl PersonaLayerSummary {
    /// Construct a safe persona layer summary.
    pub fn new(layer: impl Into<String>, summary: impl Into<String>) -> Self {
        Self {
            layer: layer.into(),
            summary: summary.into(),
        }
    }
}

// ─── Autonomy Level ──────────────────────────────────────

/// 工具自治等级。等级越高，Agent 自主决策空间越大。
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutonomyLevel {
    /// L0: 纯规则/本地检索，无 LLM 决策
    L0 = 0,
    /// L1: 单轮 LLM + 受控上下文，无工具循环
    L1 = 1,
    /// L2: 工作流中允许少量工具调用
    L2 = 2,
    /// L3: 有限 agentic loop，限制最大轮数和工具次数
    L3 = 3,
}

// ─── Scene Profile ───────────────────────────────────────

/// Scene profile: describes what capabilities a scene activates.
#[derive(Debug, Clone)]
pub struct SceneProfile {
    pub scene: AiScene,
    pub autonomy_level: AutonomyLevel,
    pub default_global_scope: bool,
    pub max_agentic_rounds: u32,
    pub max_tool_calls_per_round: u32,
    pub default_token_budget: usize,
    pub max_token_budget: usize,
}

/// Resolve a scene to its profile.
pub fn resolve_scene(scene: AiScene) -> SceneProfile {
    match scene {
        AiScene::KnowledgeLookup => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L2,
            default_global_scope: true,
            max_agentic_rounds: 3,
            max_tool_calls_per_round: 4,
            default_token_budget: 30_000,
            max_token_budget: 80_000,
        },
        AiScene::ExemplarLearning => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L2,
            default_global_scope: false,
            max_agentic_rounds: 2,
            max_tool_calls_per_round: 4,
            default_token_budget: 50_000,
            max_token_budget: 120_000,
        },
        AiScene::DraftingAssist => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L2,
            default_global_scope: false,
            max_agentic_rounds: 3,
            max_tool_calls_per_round: 5,
            default_token_budget: 60_000,
            max_token_budget: 160_000,
        },
        AiScene::ResearchSynthesis => SceneProfile {
            scene,
            autonomy_level: AutonomyLevel::L3,
            default_global_scope: true,
            max_agentic_rounds: 4,
            max_tool_calls_per_round: 6,
            default_token_budget: 100_000,
            max_token_budget: 240_000,
        },
    }
}

/// Select appropriate capability slot for scene.
pub fn slot_for_scene(scene: AiScene) -> CapabilitySlot {
    match scene {
        AiScene::KnowledgeLookup => CapabilitySlot::Fast,
        AiScene::ExemplarLearning => CapabilitySlot::Writer,
        AiScene::DraftingAssist => CapabilitySlot::Writer,
        AiScene::ResearchSynthesis => CapabilitySlot::Reasoner,
    }
}

// ─── Web evidence metadata (spec §4.1) ───────────────────

/// 网页检索后端标识。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSearchBackend {
    Minimax,
    Duckduckgo,
}

/// 网页来源可信等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WebSourceRank {
    Official,
    Academic,
    Media,
    Community,
    Unknown,
}

/// 网页证据扩展元数据（仅 `source_type = web` 时填充）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebEvidenceMeta {
    pub url: Option<String>,
    pub domain: Option<String>,
    pub published_at: Option<String>,
    pub fetched_at: String,
    pub search_backend: WebSearchBackend,
    pub source_rank: WebSourceRank,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure_reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fallback_from: Option<WebSearchBackend>,
}

// ─── ContextPacket ───────────────────────────────────────

/// 证据包 — 结构化的检索结果，是 AI 体系的核心数据结构。
///
/// `ContextPacket` 用于：
/// - 为 LLM 提供可追溯的证据来源
/// - 支持引用验证和事实核查
/// - 实现证据链可视化
///
/// 检索结果必须先变成 `ContextPacket`，再进入 prompt。
/// 各检索层（FTS / Vector / Graph / Exact / Template）均输出此类型，
/// 由 `retrieval_broker::fuse_and_rank` 统一评分融合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPacket {
    pub id: String,
    pub source_type: SourceType,
    pub source_path: Option<String>,
    pub title: String,
    pub heading_path: Option<String>,
    pub source_span: Option<SourceSpan>,
    pub content_hash: String,
    pub excerpt: String,
    pub retrieval_reason: String,
    pub score: f64,
    pub trust_level: TrustLevel,
    pub citation_label: String,
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<WebEvidenceMeta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub corpus: Option<CorpusPacketMeta>,
}

/// Corpus role metadata attached to local evidence packets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorpusPacketMeta {
    pub id: String,
    pub name: String,
    pub kind: String,
    pub label: String,
    pub instruction: String,
    pub can_be_authority: bool,
}

/// 证据包的数据来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    Note,
    Anchor,
    Regulation,
    Template,
    Session,
    Web,
}

/// UTF-8 byte offsets into a Markdown source string.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    pub start: usize,
    pub end: usize,
}

/// 证据信任等级，按可信度从高到低排列。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    UserNote,
    DerivedCache,
    ExternalWeb,
    ModelGenerated,
}

// ─── Context Status ──────────────────────────────────────

/// 上下文状态摘要，用于前端显示和调试。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStatus {
    pub regulations_loaded: usize,
    pub model_essays_loaded: usize,
    pub anchors_loaded: usize,
    pub links_loaded: usize,
    pub total_tokens_estimate: usize,
}

// ─── Tool Access Level ───────────────────────────────────

/// 工具访问权限等级，决定工具可执行的操作范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccessLevel {
    ReadIndex,
    ReadNoteSpan,
    ReadProfile,
    Network,
    WriteCache,
    WriteMarkdown,
    WriteSettings,
    /// Install / uninstall / toggle agent skills.
    #[serde(rename = "manage_skills")]
    ManageSkills,
}

/// Tool capability affinity used by task-policy driven tool exposure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolCapabilityAffinity {
    ReadNotes,
    SearchNotes,
    WriteNotes,
    PatchDocument,
    WebFetch,
    ResearchSynthesis,
    SkillManagement,
    VaultOrganize,
}

// ─── Tool Spec ───────────────────────────────────────────

/// 工具规格定义，描述一个可供 LLM 调用的工具。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub access_level: ToolAccessLevel,
    pub requires_confirmation: bool,
    pub max_results: Option<u32>,
    /// Scenes where this tool is naturally relevant.
    /// Empty means universally available.
    #[serde(default)]
    pub scene_affinity: Vec<AiScene>,
}

// ─── Request / Response types ────────────────────────────

/// AI 请求，从前端发起。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    pub scene: AiScene,
    pub note_path: Option<String>,
    pub note_content_hash: Option<String>,
    pub query: String,
    pub session_id: Option<i64>,
    pub selected_packet_ids: Option<Vec<String>>,
}

// ─── Tool Confirmation ───────────────────────────────────

/// 工具调用确认请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfirmRequest {
    pub request_id: String,
    pub tool_call_id: String,
    pub decision: ToolConfirmDecision,
    pub modified_args: Option<serde_json::Value>,
}

/// 用户对工具调用的确认决策。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolConfirmDecision {
    Approve,
    Reject,
    Modify,
}

// ─── Tool Call Result ─────────────────────────────────────

/// 工具调用结果（含可观测性元数据）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    pub tool_name: String,
    pub success: bool,
    pub output: serde_json::Value,
    pub duration_ms: u64,
    pub tokens_used: Option<u32>,
    pub error: Option<String>,
}

// ─── PatchProposal ────────────────────────────────────────

/// 受控编辑补丁 — AI 对 Markdown 的所有正文写入都必须走此结构。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposal {
    pub id: String,
    pub target_path: String,
    pub base_content_hash: String,
    pub range: SourceSpan,
    pub original_text: String,
    pub replacement_text: String,
    pub evidence_packet_ids: Vec<String>,
    pub risk_level: RiskLevel,
    pub warnings: Vec<String>,
    pub created_at: String,
}

/// 补丁风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
}

// ─── Chunked Patch Types ─────────────────────────────────

/// 分块补丁 — 多个相关补丁的集合。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedPatchProposal {
    pub id: String,
    pub target_path: String,
    pub base_content_hash: String,
    pub chunks: Vec<PatchChunk>,
    pub description: String,
    pub risk_level: RiskLevel,
    pub warnings: Vec<String>,
    pub created_at: String,
}

/// 补丁块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchChunk {
    pub id: String,
    pub range: SourceSpan,
    pub original_text: String,
    pub replacement_text: String,
    pub chunk_type: ChunkType,
    pub order: usize,
}

/// 块类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    Rewrite,
    Insert,
    Delete,
    Move,
}

/// 补丁应用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    pub success: bool,
    pub new_content_hash: Option<String>,
    pub error: Option<String>,
    pub warnings: Vec<String>,
}

/// 补丁验证错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatchValidationError {
    HashMismatch {
        expected: String,
        actual: String,
    },
    RangeOutOfBounds {
        range_start: usize,
        range_end: usize,
        content_length: usize,
    },
    TextMismatch {
        expected: String,
        actual: String,
    },
    FileNotFound {
        path: String,
    },
}

impl std::fmt::Display for PatchValidationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchValidationError::HashMismatch { expected, actual } => {
                write!(f, "内容哈希不匹配：期望 {expected}，实际 {actual}")
            }
            PatchValidationError::RangeOutOfBounds {
                range_start,
                range_end,
                content_length,
            } => {
                write!(
                    f,
                    "范围越界：[{range_start}, {range_end}) 超出内容长度 {content_length}"
                )
            }
            PatchValidationError::TextMismatch { expected, actual } => {
                write!(
                    f,
                    "原文不一致：期望 {:?}，实际 {:?}",
                    &expected[..expected.len().min(50)],
                    &actual[..actual.len().min(50)]
                )
            }
            PatchValidationError::FileNotFound { path } => {
                write!(f, "文件不存在：{path}")
            }
        }
    }
}

// ─── Writing Workflow Types ──────────────────────────────

/// 写作意图。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingIntent {
    Continue,
    Rewrite,
    AddEvidence,
    Outline,
    UnifyTone,
    ChapterRewrite,
    ChapterContinue,
    ChapterRestructure,
    OutlineCheck,
    CitationGapCheck,
    StyleConsistency,
    CrossDocReference,
}

/// 写作意图级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingIntentLevel {
    Selection,
    Chapter,
    Document,
}

impl WritingIntent {
    /// 获取意图级别。
    pub fn level(&self) -> WritingIntentLevel {
        match self {
            WritingIntent::Continue
            | WritingIntent::Rewrite
            | WritingIntent::AddEvidence
            | WritingIntent::Outline
            | WritingIntent::UnifyTone => WritingIntentLevel::Selection,
            WritingIntent::ChapterRewrite
            | WritingIntent::ChapterContinue
            | WritingIntent::ChapterRestructure => WritingIntentLevel::Chapter,
            WritingIntent::OutlineCheck
            | WritingIntent::CitationGapCheck
            | WritingIntent::StyleConsistency
            | WritingIntent::CrossDocReference => WritingIntentLevel::Document,
        }
    }
}

/// 写作建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingSuggestion {
    pub id: String,
    pub intent: WritingIntent,
    pub explanation: String,
    pub confidence: f64,
}

/// 写作任务输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingTaskInput {
    pub target_path: String,
    pub base_content_hash: String,
    pub selection: Option<String>,
    pub cursor_context: String,
    pub writing_goal: String,
    pub web_authorized: bool,
}

/// 写作任务结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingTaskResult {
    pub request_id: String,
    pub suggestions: Vec<WritingSuggestion>,
    pub patches: Vec<PatchProposal>,
    pub evidence_used: Vec<ContextPacket>,
    pub total_tokens: TokenUsage,
}

// ─── Citation Check Types ────────────────────────────────

/// 引用检查输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckInput {
    pub paragraph_text: String,
    pub document_path: String,
    pub scope: Option<CitationCheckScope>,
    pub web_authorized: bool,
}

/// 引用检查范围。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckScope {
    pub paths: Vec<String>,
    pub path_prefixes: Vec<String>,
    pub corpus_ids: Option<Vec<String>>,
}

/// 引用检查结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckResult {
    pub request_id: String,
    pub claims: Vec<FactClaim>,
    pub coverage: CitationCoverage,
    pub suggestions: Vec<CitationSuggestion>,
    pub evidence_used: Vec<ContextPacket>,
    pub total_tokens: TokenUsage,
}

/// 事实声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactClaim {
    pub id: String,
    pub statement: String,
    pub has_support: bool,
    pub supporting_evidence: Vec<String>,
    pub conflicting_evidence: Vec<String>,
}

/// 引用覆盖度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationCoverage {
    WellSupported,
    PartiallySupported,
    WeaklySupported,
    Unsupported,
    Contradicted,
}

/// 引用建议动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationAction {
    AddCitation,
    Rewrite,
    RemoveClaim,
    AddQualifier,
}

/// 引用建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationSuggestion {
    pub claim_id: String,
    pub action: CitationAction,
    pub suggested_citation: Option<String>,
    pub explanation: String,
}

// ─── Organize Workflow Types ─────────────────────────────

/// 整理建议类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeSuggestionType {
    RenameTitle,
    AddTag,
    MoveToFolder,
    AssignCorpus,
    AddBlockLink,
    ExtractTemplate,
}

/// 整理建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeSuggestion {
    pub id: String,
    pub suggestion_type: OrganizeSuggestionType,
    pub target_path: String,
    pub current_value: Option<String>,
    pub suggested_value: String,
    pub reason: String,
    pub source: String,
    pub confidence: f64,
    pub evidence_packet_ids: Vec<String>,
}

/// 批量变更计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeBatch {
    pub id: String,
    pub title: String,
    pub description: String,
    pub suggestions: Vec<OrganizeSuggestion>,
    pub created_at: String,
}

/// 整理任务输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskInput {
    pub scope: Option<OrganizeTaskScope>,
    pub task_type: OrganizeTaskType,
}

/// 整理任务范围。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskScope {
    pub paths: Vec<String>,
    pub path_prefixes: Vec<String>,
    pub corpus_ids: Option<Vec<String>>,
}

/// 整理任务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeTaskType {
    FullAudit,
    TitleSuggestions,
    TagSuggestions,
    FolderSuggestions,
    LinkSuggestions,
}

/// 整理任务结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskResult {
    pub request_id: String,
    pub batch: OrganizeBatch,
    pub total_tokens: TokenUsage,
}

// ─── Research Workflow State ─────────────────────────────

/// 研究任务状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchTaskState {
    Idle,
    Planning,
    Retrieving,
    Analyzing,
    Completed,
    Paused,
    Failed,
    Aborted,
}

/// 研究任务逐轮进度。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProgress {
    pub request_id: String,
    pub topic: String,
    pub state: ResearchTaskState,
    pub current_round: u32,
    pub max_rounds: u32,
    pub queries_executed: Vec<String>,
    pub new_evidence_count: usize,
    pub total_evidence_count: usize,
    pub tokens_used: u32,
    pub token_budget: usize,
    pub progress_pct: f64,
    pub round_terminated_early: bool,
}

/// 研究笔记生成请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNoteRequest {
    pub topic: String,
    pub summary: String,
    pub evidence_count: usize,
    pub coverage_score: f64,
    pub target_path: Option<String>,
}

/// 研究笔记生成结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNoteResult {
    pub content: String,
    pub suggested_path: String,
    pub section_count: usize,
}

// ─── Gateway Types ───────────────────────────────────────

/// Token usage statistics.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
    #[serde(default)]
    pub prompt_cache_hit_tokens: u32,
    #[serde(default)]
    pub prompt_cache_miss_tokens: u32,
}

/// 能力槽位，用于 provider/model 选择。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CapabilitySlot {
    Fast,
    Writer,
    Reasoner,
    LongContext,
    Vision,
    AgentTools,
    Embedding,
    Reranker,
    LocalPrivate,
}

/// LLM provider configuration (from settings or registry).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProviderConfig {
    pub name: String,
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub slot: CapabilitySlot,
    #[serde(default = "default_endpoint_family")]
    pub endpoint_family: EndpointFamily,
}

fn default_endpoint_family() -> EndpointFamily {
    EndpointFamily::OpenAiCompatibleChatCompletions
}

/// LLM 对话消息角色。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

/// 消息内容：纯文本或混合多模态。
///
/// 使用 `#[serde(untagged)]` 保证：
/// - `Text(String)` 序列化为 JSON 字符串（向后兼容）
/// - `Parts(Vec<ContentPart>)` 序列化为 JSON 数组（OpenAI multimodal 格式）
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MessageContent {
    /// 纯文本消息
    Text(String),
    /// 多模态内容数组（文本 + 图片混合）
    Parts(Vec<ContentPart>),
}

impl Default for MessageContent {
    fn default() -> Self {
        MessageContent::Text(String::new())
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Text(s)
    }
}

impl From<&str> for MessageContent {
    fn from(s: &str) -> Self {
        MessageContent::Text(s.to_string())
    }
}

impl MessageContent {
    /// Extract the text content, panics if this is a Parts variant (for migration safety).
    pub fn as_str(&self) -> &str {
        match self {
            MessageContent::Text(s) => s.as_str(),
            MessageContent::Parts(_) => {
                panic!("called as_str() on multimodal Parts content")
            }
        }
    }

    /// Mutable access to text content.
    pub fn as_mut_str(&mut self) -> &mut String {
        match self {
            MessageContent::Text(s) => s,
            MessageContent::Parts(_) => {
                panic!("called as_mut_str() on multimodal Parts content")
            }
        }
    }

    /// Check if content is empty (for Text variant).
    pub fn is_empty(&self) -> bool {
        match self {
            MessageContent::Text(s) => s.is_empty(),
            MessageContent::Parts(parts) => parts.is_empty(),
        }
    }
}

/// 内容片段（遵循 OpenAI multimodal 格式，可转换为 Anthropic 格式）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum ContentPart {
    /// 文本片段
    Text { text: String },
    /// 图片片段（base64 data URL 或 HTTP URL）
    ImageUrl { image_url: ImageUrlPayload },
}

/// 图片 URL 负载。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageUrlPayload {
    /// "data:image/png;base64,xxxxx" 或 HTTP URL
    pub url: String,
    /// "auto" | "low" | "high"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
}

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: MessageContent,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_call_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tool_calls: Option<Vec<ToolCall>>,
    /// DeepSeek / thinking-mode chain-of-thought; must be echoed on tool-call turns.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reasoning_content: Option<String>,
}

impl Default for LlmMessage {
    fn default() -> Self {
        Self {
            role: MessageRole::User,
            content: MessageContent::Text(String::new()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        }
    }
}

/// Tool call from LLM (OpenAI / DeepSeek chat completions format).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall {
    pub id: String,
    #[serde(rename = "type", default = "default_tool_call_type")]
    pub call_type: String,
    pub function: FunctionCall,
}

fn default_tool_call_type() -> String {
    "function".into()
}

impl ToolCall {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        arguments: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            call_type: default_tool_call_type(),
            function: FunctionCall {
                name: name.into(),
                arguments: arguments.into(),
            },
        }
    }
}

/// Function call details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FunctionCall {
    pub name: String,
    pub arguments: String,
}

// ─── LLM Config Types ────────────────────────────────────

/// 上下文组装策略。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStrategy {
    Hybrid,
    LongContext,
}

// ─── Testability Seams (trait abstractions) ───────────────

/// Abstraction over an LLM provider for testability.
///
/// Production code uses the concrete implementation backed by `reqwest`;
/// tests can inject a mock that records calls and returns canned responses.
#[allow(async_fn_in_trait)]
pub trait LlmBackend: Send + Sync {
    /// Send a non-streaming chat completion request.
    async fn chat(
        &self,
        provider: &ProviderConfig,
        messages: &[LlmMessage],
        tools: &[serde_json::Value],
        max_tokens: Option<u32>,
        temperature: Option<f64>,
    ) -> Result<LlmBackendResponse, String>;
}

/// Simplified response from [`LlmBackend::chat`].
#[derive(Debug, Clone)]
pub struct LlmBackendResponse {
    pub content: Option<String>,
    pub tool_calls: Vec<ToolCall>,
    pub usage: TokenUsage,
    pub finish_reason: String,
}

/// Abstraction over a text embedding engine for testability.
///
/// Production code loads the fastembed model; tests can inject a
/// deterministic stub that returns fixed vectors.
pub trait EmbedBackend: Send + Sync {
    /// Embed a single text into a vector.
    fn embed(&self, text: &str) -> Result<Vec<f32>, String>;

    /// Batch-embed multiple texts (default: sequential calls to [`embed`]).
    fn embed_batch(&self, texts: &[&str]) -> Result<Vec<Vec<f32>>, String> {
        texts.iter().map(|t| self.embed(t)).collect()
    }
}

#[cfg(test)]
mod phase2_agent_intent_tests {
    use super::*;

    #[test]
    fn agent_intent_legacy_mapping_keeps_old_assistant_values_compatible() {
        assert_eq!(
            AgentIntent::from_legacy_assistant_intent("knowledge"),
            AgentIntent::AskNotes
        );
        assert_eq!(
            AgentIntent::from_legacy_assistant_intent("writing"),
            AgentIntent::RewriteSelection
        );
        assert_eq!(
            AgentIntent::from_legacy_assistant_intent("citation"),
            AgentIntent::CitationCheck
        );
        assert_eq!(
            AgentIntent::from_legacy_assistant_intent("document"),
            AgentIntent::DocumentCheck
        );
        assert_eq!(
            AgentIntent::from_legacy_assistant_intent("unknown"),
            AgentIntent::Chat
        );
    }

    #[test]
    fn run_plan_summary_does_not_store_sensitive_content_fields() {
        let summary = AgentRunPlanSummary::for_intent(
            "req-1".to_string(),
            AgentIntent::AskNotes,
            AiScene::KnowledgeLookup,
            vec!["当前笔记".to_string()],
            "读取当前笔记摘要，必要时检索知识库".to_string(),
        );

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("ask_notes"));
        assert!(!json.contains("note_content"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("base64"));
        assert!(!json.contains("clipboard"));
    }

    #[test]
    fn run_plan_summary_uses_harness_status_and_permission_state() {
        let summary = AgentRunPlanSummary::for_intent(
            "req-2".to_string(),
            AgentIntent::Chat,
            AiScene::KnowledgeLookup,
            vec!["无额外上下文".to_string()],
            "chat tools".to_string(),
        )
        .with_execution_state(
            "pending_confirmation",
            "等待工具确认".to_string(),
            vec!["shell tool needs approval".to_string()],
            true,
        );

        assert_eq!(summary.progress_state, "pending_confirmation");
        assert_eq!(summary.permission_summary, "等待工具确认");
        assert_eq!(
            summary.blocked_reasons,
            vec!["shell tool needs approval".to_string()]
        );
        assert!(summary.degraded);
    }
}

#[cfg(test)]
mod phase3_model_persona_route_tests {
    use super::*;

    #[test]
    fn capability_slot_serializes_all_phase3_slots() {
        let slots = [
            (CapabilitySlot::Fast, "fast"),
            (CapabilitySlot::Writer, "writer"),
            (CapabilitySlot::Reasoner, "reasoner"),
            (CapabilitySlot::LongContext, "long_context"),
            (CapabilitySlot::Vision, "vision"),
            (CapabilitySlot::AgentTools, "agent_tools"),
            (CapabilitySlot::Embedding, "embedding"),
            (CapabilitySlot::Reranker, "reranker"),
            (CapabilitySlot::LocalPrivate, "local_private"),
        ];

        for (slot, wire) in slots {
            assert_eq!(serde_json::to_value(slot).unwrap(), serde_json::json!(wire));
            assert_eq!(
                serde_json::from_value::<CapabilitySlot>(serde_json::json!(wire)).unwrap(),
                slot
            );
        }
    }

    #[test]
    fn ai_scene_parse_wire_accepts_only_stable_scene_values() {
        assert_eq!(
            AiScene::parse_wire("knowledge_lookup"),
            Some(AiScene::KnowledgeLookup)
        );
        assert_eq!(
            AiScene::parse_wire(" drafting_assist "),
            Some(AiScene::DraftingAssist)
        );
        assert_eq!(AiScene::parse_wire("\"knowledge_lookup\""), None);
        assert_eq!(AiScene::parse_wire("unknown"), None);
    }

    #[test]
    fn run_plan_summary_includes_safe_model_and_persona_metadata() {
        let summary = AgentRunPlanSummary::for_intent(
            "req-phase3".to_string(),
            AgentIntent::VisionChat,
            AiScene::KnowledgeLookup,
            vec!["包含图片附件摘要".to_string()],
            "工具策略不变".to_string(),
        )
        .with_model_route(CapabilityRouteSummary {
            slot: CapabilitySlot::Vision,
            provider_id: "openai".to_string(),
            model: "gpt-4o".to_string(),
            fallback_chain: vec![CapabilitySlot::Vision, CapabilitySlot::Fast],
            reason: "vision_chat requires image-aware model".to_string(),
            probe_status: "unknown".to_string(),
            degraded: false,
        })
        .with_persona_layers(vec![
            PersonaLayerSummary::new("safety_overlay", "最高优先级安全边界"),
            PersonaLayerSummary::new("identity", "PromptProfile identity"),
            PersonaLayerSummary::new("style", "PromptProfile style"),
            PersonaLayerSummary::new("task_overlay", "vision_chat task guidance"),
            PersonaLayerSummary::new("skill_overlay", "active skill guidance"),
        ]);

        let json = serde_json::to_string(&summary).unwrap();
        assert!(json.contains("modelRoute"));
        assert!(json.contains("personaLayers"));
        assert!(json.contains("vision"));
        assert!(json.contains("safety_overlay"));
        assert!(!json.contains("api_key"));
        assert!(!json.contains("base64"));
        assert!(!json.contains("clipboard"));
        assert!(!json.contains("note_content"));
    }
}

#[cfg(test)]
mod multimodal_message_content_tests {
    use super::*;

    // ── MessageContent serialization ──

    #[test]
    fn text_content_serializes_as_plain_string() {
        let content = MessageContent::Text("hello".to_string());
        let json = serde_json::to_value(&content).unwrap();
        assert_eq!(json, serde_json::json!("hello"));
    }

    #[test]
    fn text_content_deserializes_from_plain_string() {
        let json = serde_json::json!("hello world");
        let content: MessageContent = serde_json::from_value(json).unwrap();
        match content {
            MessageContent::Text(s) => assert_eq!(s, "hello world"),
            _ => panic!("expected Text variant"),
        }
    }

    #[test]
    fn parts_content_serializes_as_array() {
        let parts = vec![
            ContentPart::Text {
                text: "describe this".to_string(),
            },
            ContentPart::ImageUrl {
                image_url: ImageUrlPayload {
                    url: "data:image/png;base64,abc123".to_string(),
                    detail: Some("auto".to_string()),
                },
            },
        ];
        let content = MessageContent::Parts(parts);
        let json = serde_json::to_value(&content).unwrap();
        assert!(json.is_array());
        assert_eq!(json.as_array().unwrap().len(), 2);
    }

    #[test]
    fn parts_content_deserializes_from_array() {
        let json = serde_json::json!([
            { "type": "text", "text": "what is this?" },
            { "type": "image_url", "image_url": { "url": "data:image/jpeg;base64,xyz", "detail": "auto" } }
        ]);
        let content: MessageContent = serde_json::from_value(json).unwrap();
        match content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    ContentPart::Text { text } => assert_eq!(text, "what is this?"),
                    _ => panic!("expected Text content part"),
                }
            }
            _ => panic!("expected Parts variant"),
        }
    }

    // ── ContentPart serialization ──

    #[test]
    fn text_part_serializes_correctly() {
        let part = ContentPart::Text {
            text: "hello".to_string(),
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "hello");
    }

    #[test]
    fn image_url_part_serializes_correctly() {
        let part = ContentPart::ImageUrl {
            image_url: ImageUrlPayload {
                url: "data:image/png;base64,abc".to_string(),
                detail: Some("high".to_string()),
            },
        };
        let json = serde_json::to_value(&part).unwrap();
        assert_eq!(json["type"], "image_url");
        assert_eq!(json["image_url"]["url"], "data:image/png;base64,abc");
        assert_eq!(json["image_url"]["detail"], "high");
    }

    #[test]
    fn image_url_part_detail_is_optional() {
        let part = ContentPart::ImageUrl {
            image_url: ImageUrlPayload {
                url: "data:image/png;base64,abc".to_string(),
                detail: None,
            },
        };
        let json = serde_json::to_value(&part).unwrap();
        assert!(json["image_url"].get("detail").is_none());
    }

    // ── LlmMessage backwards compatibility ──

    #[test]
    fn llm_message_text_content_serializes_as_string() {
        let msg = LlmMessage {
            role: MessageRole::User,
            content: MessageContent::Text("hello".to_string()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        // content field should be a plain string (backwards compatible)
        assert!(json["content"].is_string());
        assert_eq!(json["content"], "hello");
    }

    #[test]
    fn llm_message_parts_content_serializes_as_array() {
        let msg = LlmMessage {
            role: MessageRole::User,
            content: MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "describe".to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrlPayload {
                        url: "data:image/png;base64,abc".to_string(),
                        detail: None,
                    },
                },
            ]),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };
        let json = serde_json::to_value(&msg).unwrap();
        assert!(json["content"].is_array());
    }

    #[test]
    fn llm_message_deserializes_plain_string_as_text() {
        let json = serde_json::json!({
            "role": "user",
            "content": "plain text message"
        });
        let msg: LlmMessage = serde_json::from_value(json).unwrap();
        match msg.content {
            MessageContent::Text(s) => assert_eq!(s, "plain text message"),
            _ => panic!("should deserialize as text"),
        }
    }

    // ── From<String> / From<&str> helpers ──

    #[test]
    fn from_string_creates_text_variant() {
        let content = MessageContent::from("hello".to_string());
        match content {
            MessageContent::Text(s) => assert_eq!(s, "hello"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn from_str_creates_text_variant() {
        let content = MessageContent::from("world");
        match content {
            MessageContent::Text(s) => assert_eq!(s, "world"),
            _ => panic!("expected Text"),
        }
    }

    #[test]
    fn into_message_content_works_with_string() {
        let content: MessageContent = "test message".into();
        match content {
            MessageContent::Text(s) => assert_eq!(s, "test message"),
            _ => panic!("expected Text"),
        }
    }

    // ── LlmMessage default ──

    #[test]
    fn llm_message_default_has_empty_text_content() {
        let msg = LlmMessage::default();
        match msg.content {
            MessageContent::Text(s) => assert!(s.is_empty()),
            _ => panic!("default should be Text"),
        }
    }

    // ── Round-trip: message with text ──

    #[test]
    fn round_trip_text_message() {
        let original = LlmMessage {
            role: MessageRole::User,
            content: MessageContent::Text("hello world".to_string()),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: LlmMessage = serde_json::from_str(&json).unwrap();
        match restored.content {
            MessageContent::Text(s) => assert_eq!(s, "hello world"),
            _ => panic!("round-trip should preserve Text"),
        }
    }

    // ── Round-trip: message with multimodal parts ──

    #[test]
    fn round_trip_multimodal_message() {
        let original = LlmMessage {
            role: MessageRole::User,
            content: MessageContent::Parts(vec![
                ContentPart::Text {
                    text: "describe this image".to_string(),
                },
                ContentPart::ImageUrl {
                    image_url: ImageUrlPayload {
                        url: "data:image/png;base64,iVBORw0KGgo=".to_string(),
                        detail: Some("auto".to_string()),
                    },
                },
            ]),
            tool_call_id: None,
            tool_calls: None,
            reasoning_content: None,
        };
        let json = serde_json::to_string(&original).unwrap();
        let restored: LlmMessage = serde_json::from_str(&json).unwrap();
        match restored.content {
            MessageContent::Parts(parts) => {
                assert_eq!(parts.len(), 2);
                match &parts[0] {
                    ContentPart::Text { text } => assert_eq!(text, "describe this image"),
                    _ => panic!("first part should be Text"),
                }
                match &parts[1] {
                    ContentPart::ImageUrl { image_url } => {
                        assert!(image_url.url.starts_with("data:image/png;base64,"));
                    }
                    _ => panic!("second part should be ImageUrl"),
                }
            }
            _ => panic!("round-trip should preserve Parts"),
        }
    }
}
