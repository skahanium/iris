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

pub mod context_planner;
pub mod eval;
pub mod guardrails;
pub mod model_gateway;
pub mod model_registry;
pub mod packet_builder;
pub mod packet_cache;
pub mod research_workflow;
pub mod retrieval_broker;
pub mod retrieval_scope;
pub mod scene_router;
pub mod session;
pub mod tool_executor;
pub mod trace;

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
}

/// 证据包的数据来源类型。
#[derive(Debug, Clone, Serialize, Deserialize)]
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
