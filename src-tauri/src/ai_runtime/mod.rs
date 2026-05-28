//! Iris AI Runtime — core types, scene routing, tool permission, trace.
//!
//! Public API:
//! - `types` re-export: Scene, ContextPacket, ToolSpec, ToolAccessLevel, etc.
//! - `scene_router`: scene → workflow profile resolution
//! - `model_registry`: capability-slot → provider/model mapping
//! - `tool_executor`: tool definitions, permission checks, execution dispatch
//! - `trace`: request lifecycle tracing into `ai_traces` table
//! - `session`: session / session_messages CRUD
//! - `packet_builder`: ContextPacket construction from retrieval results

pub mod chapter_workflow;
pub mod citation_workflow;
pub mod context_planner;
pub mod document_workflow;
pub mod eval;
pub mod evidence_mixer;
pub mod guardrails;
pub mod model_gateway;
pub mod model_registry;
pub mod organize_workflow;
pub mod packet_builder;
pub mod packet_cache;
pub mod research_workflow;
pub mod retrieval_broker;
pub mod retrieval_scope;
pub mod scene_router;
pub mod session;
pub mod tool_executor;
pub mod trace;
pub mod writing_workflow;

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
/// 由 [`retrieval_broker::fuse_and_rank`] 统一评分融合。
///
/// # Examples
///
/// ```rust
/// use iris::ai_runtime::{ContextPacket, SourceType, TrustLevel};
///
/// let packet = ContextPacket {
///     id: "pkt_001".to_string(),
///     source_type: SourceType::Note,
///     source_path: Some("notes/sqlite.md".to_string()),
///     title: "SQLite 入门".to_string(),
///     heading_path: None,
///     source_span: None,
///     content_hash: "abc123".to_string(),
///     excerpt: "SQLite 是一个嵌入式数据库...".to_string(),
///     retrieval_reason: "vector_chunk".to_string(),
///     score: 0.92,
///     trust_level: TrustLevel::UserNote,
///     citation_label: "[C0]".to_string(),
///     stale: false,
/// };
/// assert_eq!(packet.source_type, SourceType::Note);
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextPacket {
    /// 唯一标识符，格式为 `{layer}-{rowid}`（如 `chunk-42`、`reg-7`）
    pub id: String,
    /// 数据来源类型
    pub source_type: SourceType,
    /// 源文件的相对路径（如 `notes/foo.md`）
    pub source_path: Option<String>,
    /// 人类可读的来源标题
    pub title: String,
    /// 章节路径（如 `第三章 > 第十二条`）
    pub heading_path: Option<String>,
    /// 源文件中的字符偏移范围
    pub source_span: Option<SourceSpan>,
    /// 内容哈希，用于变更检测
    pub content_hash: String,
    /// 摘录文本，通常截断到 300–500 字符
    pub excerpt: String,
    /// 检索原因标识（如 `vector_chunk`、`fts_keyword_match`、`exact_regulation_lookup`）
    pub retrieval_reason: String,
    /// 融合后的相关度评分，范围 `[0.0, 1.0]`
    pub score: f64,
    /// 信任等级，决定引用权重
    pub trust_level: TrustLevel,
    /// 引用标签，用于 LLM 输出中的引用格式（如 `[1]`、`《纪律处分条例》第6条`）
    pub citation_label: String,
    /// 是否已过期（源文件被修改后标记为 stale）
    pub stale: bool,
    /// 网页来源元数据（本地证据为 `None`）
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub web: Option<WebEvidenceMeta>,
}

/// 证据包的数据来源类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceType {
    /// 用户笔记（.md 文件）
    Note,
    /// 语义锚点（自动提取的段落级摘要）
    Anchor,
    /// 法规条款（结构化索引）
    Regulation,
    /// 文种模板（genre template）
    Template,
    /// 会话历史
    Session,
    /// 外部网页
    Web,
}

/// 源文件中的字符偏移范围。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceSpan {
    /// 起始字符偏移（包含）
    pub start: usize,
    /// 结束字符偏移（不包含）
    pub end: usize,
}

/// 证据信任等级，按可信度从高到低排列。
///
/// 用于 [`filter_by_trust`] 过滤低信任度证据。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    /// 用户手写笔记 — 最高信任度
    UserNote,
    /// 系统派生/缓存（语义锚点、法规索引等）
    DerivedCache,
    /// 外部网页检索结果
    ExternalWeb,
    /// 模型生成的内容 — 最低信任度
    ModelGenerated,
}

// ─── Tool Access Level ───────────────────────────────────

/// 工具访问权限等级，决定工具可执行的操作范围。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolAccessLevel {
    /// 只读索引（FTS / 向量搜索）
    ReadIndex,
    /// 只读笔记片段
    ReadNoteSpan,
    /// 只读用户画像
    ReadProfile,
    /// 网络访问（需用户授权）
    Network,
    /// 写入缓存（不修改用户文件）
    WriteCache,
    /// 写入 Markdown 文件（需用户确认）
    WriteMarkdown,
    /// 写入设置（需用户确认）
    WriteSettings,
}

// ─── Tool Spec ───────────────────────────────────────────

/// 工具规格定义，描述一个可供 LLM 调用的工具。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSpec {
    /// 工具名称（如 `search_hybrid`、`read_note`）
    pub name: String,
    /// 工具功能描述，会注入 LLM 的 tool definition
    pub description: String,
    /// JSON Schema 格式的输入参数定义
    pub input_schema: serde_json::Value,
    /// 访问权限等级
    pub access_level: ToolAccessLevel,
    /// 允许使用此工具的场景白名单
    pub scene_allowlist: Vec<AiScene>,
    /// 是否需要用户确认后才能执行
    pub requires_confirmation: bool,
    /// 最大返回结果数
    pub max_results: Option<u32>,
}

// ─── Request / Response types ────────────────────────────

/// AI 请求，从前端发起，包含场景、查询和上下文信息。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AiRequest {
    /// 使用场景
    pub scene: AiScene,
    /// 当前笔记路径（可选，用于上下文绑定）
    pub note_path: Option<String>,
    /// 当前笔记内容哈希（用于变更检测）
    pub note_content_hash: Option<String>,
    /// 用户查询文本
    pub query: String,
    /// 会话 ID（续接已有会话时传入）
    pub session_id: Option<i64>,
    /// 用户手动选中的证据包 ID 列表
    pub selected_packet_ids: Option<Vec<String>>,
}

/// 组装后的上下文，包含证据包、可用工具和状态摘要。
///
/// 由 context_planner 组装，直接传入 model_gateway 构建 prompt。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssembledContext {
    /// 证据包列表
    pub packets: Vec<ContextPacket>,
    /// 当前场景可用的工具列表
    pub tools: Vec<ToolSpec>,
    /// 上下文状态摘要
    pub context_status: ContextStatus,
}

/// 上下文状态摘要，用于前端显示和调试。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContextStatus {
    /// 已加载的法规条款数
    pub regulations_loaded: usize,
    /// 已加载的范文数
    pub model_essays_loaded: usize,
    /// 已加载的语义锚点数
    pub anchors_loaded: usize,
    /// 已加载的链接数
    pub links_loaded: usize,
    /// 估算的总 token 数
    pub total_tokens_estimate: usize,
}

// ─── Tool Confirmation ───────────────────────────────────

/// 工具调用确认请求，前端弹窗后用户做出的决策。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolConfirmRequest {
    /// 关联的 AI 请求 ID
    pub request_id: String,
    /// LLM 返回的 tool_call ID
    pub tool_call_id: String,
    /// 用户决策
    pub decision: ToolConfirmDecision,
    /// 用户修改后的参数（仅 `Modify` 决策时有值）
    pub modified_args: Option<serde_json::Value>,
}

/// 用户对工具调用的确认决策。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolConfirmDecision {
    /// 批准执行
    Approve,
    /// 拒绝执行
    Reject,
    /// 修改参数后执行
    Modify,
}

// ─── Tool Call Result ─────────────────────────────────────

/// 工具调用结果（含可观测性元数据）。
///
/// 每次工具调用后生成，用于 trace 记录和前端展示。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCallResult {
    /// 工具名称
    pub tool_name: String,
    /// 是否成功
    pub success: bool,
    /// 工具输出（JSON 格式）
    pub output: serde_json::Value,
    /// 执行耗时（毫秒）
    pub duration_ms: u64,
    /// 消耗的 token 数（可选）
    pub tokens_used: Option<u32>,
    /// 错误信息（失败时有值）
    pub error: Option<String>,
}

// ─── PatchProposal ────────────────────────────────────────

/// 受控编辑补丁 — AI 对 Markdown 的所有正文写入都必须走此结构。
///
/// 应用规则：
/// - `base_content_hash` 不匹配时禁止应用
/// - `range` 越界时禁止应用
/// - 原文与当前范围不一致时禁止应用
/// - 接受补丁前展示 diff
/// - 接受补丁后走现有 `file_write` 和版本快照链路
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchProposal {
    /// 补丁唯一 ID
    pub id: String,
    /// 目标文件相对路径
    pub target_path: String,
    /// 基准内容哈希（SHA-256），用于变更检测
    pub base_content_hash: String,
    /// 替换范围（字符偏移）
    pub range: SourceSpan,
    /// 原始文本（range 内的文本）
    pub original_text: String,
    /// 替换文本
    pub replacement_text: String,
    /// 关联的证据包 ID 列表
    pub evidence_packet_ids: Vec<String>,
    /// 风险等级
    pub risk_level: RiskLevel,
    /// 警告信息列表
    pub warnings: Vec<String>,
    /// 创建时间（ISO 8601）
    pub created_at: String,
}

/// 补丁风险等级。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    /// 低风险：小范围修改，不影响文档结构
    Low,
    /// 中风险：较大范围修改或涉及结构调整
    Medium,
    /// 高风险：大范围修改或可能影响文档完整性
    High,
}

// ─── Chunked Patch Types ─────────────────────────────────

/// 分块补丁 — 多个相关补丁的集合。
///
/// 用于章节级和文档级写作，将大范围修改分解为多个可独立应用的小块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkedPatchProposal {
    /// 分块补丁唯一 ID
    pub id: String,
    /// 目标文件相对路径
    pub target_path: String,
    /// 基准内容哈希（SHA-256）
    pub base_content_hash: String,
    /// 补丁块列表（按应用顺序排列）
    pub chunks: Vec<PatchChunk>,
    /// 整体描述
    pub description: String,
    /// 整体风险等级
    pub risk_level: RiskLevel,
    /// 警告信息列表
    pub warnings: Vec<String>,
    /// 创建时间（ISO 8601）
    pub created_at: String,
}

/// 补丁块。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchChunk {
    /// 块 ID
    pub id: String,
    /// 替换范围（字符偏移）
    pub range: SourceSpan,
    /// 原始文本
    pub original_text: String,
    /// 替换文本
    pub replacement_text: String,
    /// 块类型
    pub chunk_type: ChunkType,
    /// 应用顺序
    pub order: usize,
}

/// 块类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChunkType {
    /// 改写
    Rewrite,
    /// 插入
    Insert,
    /// 删除
    Delete,
    /// 移动
    Move,
}

/// 补丁应用结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchApplyResult {
    /// 是否成功应用
    pub success: bool,
    /// 应用后的新内容哈希
    pub new_content_hash: Option<String>,
    /// 错误信息（失败时有值）
    pub error: Option<String>,
    /// 警告信息
    pub warnings: Vec<String>,
}

/// 补丁验证错误。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PatchValidationError {
    /// 内容哈希不匹配（文件已被修改）
    HashMismatch { expected: String, actual: String },
    /// 范围越界
    RangeOutOfBounds {
        range_start: usize,
        range_end: usize,
        content_length: usize,
    },
    /// 原文不一致
    TextMismatch { expected: String, actual: String },
    /// 文件不存在
    FileNotFound { path: String },
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
    // ─── 选区级（已有）───
    /// 续写
    Continue,
    /// 改写
    Rewrite,
    /// 补依据
    AddEvidence,
    /// 生成提纲
    Outline,
    /// 统一语气
    UnifyTone,
    // ─── 章节级（新增）───
    /// 章节改写
    ChapterRewrite,
    /// 章节续写
    ChapterContinue,
    /// 章节重排
    ChapterRestructure,
    // ─── 文档级（新增）───
    /// 大纲检查
    OutlineCheck,
    /// 引用缺口检查
    CitationGapCheck,
    /// 风格一致性检查
    StyleConsistency,
    /// 跨文档引用建议
    CrossDocReference,
}

/// 写作意图级别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WritingIntentLevel {
    /// 选区级
    Selection,
    /// 章节级
    Chapter,
    /// 文档级
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
    /// 建议 ID
    pub id: String,
    /// 写作意图
    pub intent: WritingIntent,
    /// 解释说明
    pub explanation: String,
    /// 置信度
    pub confidence: f64,
}

/// 写作任务输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingTaskInput {
    /// 目标文件相对路径
    pub target_path: String,
    /// 基准内容哈希
    pub base_content_hash: String,
    /// 选中文本（可选）
    pub selection: Option<String>,
    /// 光标邻域上下文
    pub cursor_context: String,
    /// 写作目标
    pub writing_goal: String,
    /// 是否允许联网
    pub web_authorized: bool,
}

/// 写作任务结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WritingTaskResult {
    /// 请求 ID
    pub request_id: String,
    /// 写作建议列表
    pub suggestions: Vec<WritingSuggestion>,
    /// 补丁列表
    pub patches: Vec<PatchProposal>,
    /// 使用的证据包
    pub evidence_used: Vec<ContextPacket>,
    /// Token 消耗
    pub total_tokens: TokenUsage,
}

/// Token 使用量。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenUsage {
    pub prompt_tokens: u32,
    pub completion_tokens: u32,
    pub total_tokens: u32,
}

// ─── Citation Check Types ────────────────────────────────

/// 引用检查输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationCheckInput {
    /// 段落/选区文本
    pub paragraph_text: String,
    /// 文档路径
    pub document_path: String,
    /// 检索范围（可选）
    pub scope: Option<CitationCheckScope>,
    /// 是否允许联网
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
    /// 请求 ID
    pub request_id: String,
    /// 事实声明列表
    pub claims: Vec<FactClaim>,
    /// 覆盖度评估
    pub coverage: CitationCoverage,
    /// 建议列表
    pub suggestions: Vec<CitationSuggestion>,
    /// 使用的证据包
    pub evidence_used: Vec<ContextPacket>,
    /// Token 消耗
    pub total_tokens: TokenUsage,
}

/// 事实声明。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactClaim {
    /// 声明 ID
    pub id: String,
    /// 声明内容
    pub statement: String,
    /// 是否有支持证据
    pub has_support: bool,
    /// 支持证据包 ID
    pub supporting_evidence: Vec<String>,
    /// 冲突证据包 ID
    pub conflicting_evidence: Vec<String>,
}

/// 引用覆盖度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationCoverage {
    /// 充分支持
    WellSupported,
    /// 部分支持
    PartiallySupported,
    /// 支持不足
    WeaklySupported,
    /// 无依据
    Unsupported,
    /// 存在冲突
    Contradicted,
}

/// 引用建议动作。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CitationAction {
    /// 添加引用
    AddCitation,
    /// 改写
    Rewrite,
    /// 删除声明
    RemoveClaim,
    /// 添加限定词
    AddQualifier,
}

/// 引用建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CitationSuggestion {
    /// 关联的声明 ID
    pub claim_id: String,
    /// 建议动作
    pub action: CitationAction,
    /// 建议的引用文本（可选）
    pub suggested_citation: Option<String>,
    /// 解释说明
    pub explanation: String,
}

// ─── Organize Workflow Types ─────────────────────────────

/// 整理建议类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeSuggestionType {
    /// 建议重命名标题
    RenameTitle,
    /// 建议添加标签
    AddTag,
    /// 建议移动到文件夹
    MoveToFolder,
    /// 建议归入语料库
    AssignCorpus,
    /// 建议添加块级链接
    AddBlockLink,
    /// 建议提取模板
    ExtractTemplate,
}

/// 整理建议。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeSuggestion {
    /// 建议 ID
    pub id: String,
    /// 建议类型
    pub suggestion_type: OrganizeSuggestionType,
    /// 目标文件路径
    pub target_path: String,
    /// 当前值（可选）
    pub current_value: Option<String>,
    /// 建议值
    pub suggested_value: String,
    /// 建议理由
    pub reason: String,
    /// 来源（如 "anchor_similarity", "pattern_analysis"）
    pub source: String,
    /// 置信度 (0.0-1.0)
    pub confidence: f64,
    /// 关联的证据包 ID
    pub evidence_packet_ids: Vec<String>,
}

/// 批量变更计划。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeBatch {
    /// 计划 ID
    pub id: String,
    /// 计划标题
    pub title: String,
    /// 计划描述
    pub description: String,
    /// 建议列表
    pub suggestions: Vec<OrganizeSuggestion>,
    /// 创建时间
    pub created_at: String,
}

/// 整理任务输入。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskInput {
    /// 整理范围（可选）
    pub scope: Option<OrganizeTaskScope>,
    /// 任务类型
    pub task_type: OrganizeTaskType,
}

/// 整理任务范围。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskScope {
    /// 文件路径列表
    pub paths: Vec<String>,
    /// 路径前缀
    pub path_prefixes: Vec<String>,
    /// 语料库 ID
    pub corpus_ids: Option<Vec<String>>,
}

/// 整理任务类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizeTaskType {
    /// 全面审计
    FullAudit,
    /// 标题建议
    TitleSuggestions,
    /// 标签建议
    TagSuggestions,
    /// 文件夹归类建议
    FolderSuggestions,
    /// 链接建议
    LinkSuggestions,
}

/// 整理任务结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrganizeTaskResult {
    /// 请求 ID
    pub request_id: String,
    /// 批量变更计划
    pub batch: OrganizeBatch,
    /// Token 消耗
    pub total_tokens: TokenUsage,
}

// ─── Research Workflow State (Phase 4) ──────────────────

/// 研究任务状态 — 用于半自治研究的逐轮进度追踪。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResearchTaskState {
    /// 空闲/未开始
    Idle,
    /// 正在拆解子命题
    Planning,
    /// 正在检索证据
    Retrieving,
    /// 正在分析证据和构建矩阵
    Analyzing,
    /// 已完成
    Completed,
    /// 用户暂停
    Paused,
    /// 已失败
    Failed,
    /// 用户中止
    Aborted,
}

/// 研究任务逐轮进度 — 每轮结束后推送前端。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchProgress {
    /// 请求 ID
    pub request_id: String,
    /// 研究主题
    pub topic: String,
    /// 当前状态
    pub state: ResearchTaskState,
    /// 当前轮次（从 1 开始）
    pub current_round: u32,
    /// 总轮次上限
    pub max_rounds: u32,
    /// 本轮执行的查询列表
    pub queries_executed: Vec<String>,
    /// 本轮新检索的证据数
    pub new_evidence_count: usize,
    /// 累计证据总数
    pub total_evidence_count: usize,
    /// 已消耗 token
    pub tokens_used: u32,
    /// Token 预算
    pub token_budget: usize,
    /// 进度百分比 (0.0 - 1.0)
    pub progress_pct: f64,
    /// 本轮是否提前终止（EVIDENCE_SUFFICIENT）
    pub round_terminated_early: bool,
}

/// 研究笔记生成请求。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNoteRequest {
    /// 研究主题
    pub topic: String,
    /// 研究结果摘要
    pub summary: String,
    /// 证据矩阵
    pub evidence_count: usize,
    /// 覆盖度评分
    pub coverage_score: f64,
    /// 目标路径（可选）
    pub target_path: Option<String>,
}

/// 研究笔记生成结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResearchNoteResult {
    /// 生成的 Markdown 内容
    pub content: String,
    /// 建议的文件路径
    pub suggested_path: String,
    /// 包含的节数
    pub section_count: usize,
}
