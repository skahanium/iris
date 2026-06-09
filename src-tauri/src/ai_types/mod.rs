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

/// 源文件中的字符偏移范围。
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
    ManageSkills,
}

// ─── Tool Spec ───────────────────────────────────────────

/// 工具规格定义，描述一个可供 LLM 调用的工具。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
    pub access_level: ToolAccessLevel,
    pub scene_allowlist: Vec<AiScene>,
    pub requires_confirmation: bool,
    pub max_results: Option<u32>,
    /// Scenes where this tool is naturally relevant.
    /// Empty means universally available. New field parallel to scene_allowlist;
    /// Phase 4 will remove scene_allowlist once policy engine is complete.
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

/// A single message in the conversation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LlmMessage {
    pub role: MessageRole,
    pub content: String,
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
            content: String::new(),
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
