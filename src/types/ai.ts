// AI Runtime core types — mirrors Rust ai_runtime::types

export type AiScene =
  | "knowledge_lookup"
  | "exemplar_learning"
  | "drafting_assist"
  | "research_synthesis";

export type AutonomyLevel = "L0" | "L1" | "L2" | "L3";

export type SourceType =
  | "note"
  | "anchor"
  | "regulation"
  | "template"
  | "session"
  | "web";

export type TrustLevel =
  | "user_note"
  | "derived_cache"
  | "external_web"
  | "model_generated";

export type ToolAccessLevel =
  | "read_index"
  | "read_note_span"
  | "read_profile"
  | "network"
  | "write_cache"
  | "write_markdown"
  | "write_settings";

export interface SourceSpan {
  start: number;
  end: number;
}

export type WebSearchBackend = "minimax" | "duckduckgo";

export type WebSourceRank =
  | "official"
  | "academic"
  | "media"
  | "community"
  | "unknown";

export interface WebEvidenceMeta {
  url?: string | null;
  domain?: string | null;
  published_at?: string | null;
  fetched_at: string;
  search_backend: WebSearchBackend;
  source_rank: WebSourceRank;
  failure_reason?: string | null;
  fallback_from?: WebSearchBackend | null;
}

export interface ContextPacket {
  id: string;
  source_type: SourceType;
  source_path: string | null;
  title: string;
  heading_path: string | null;
  source_span: SourceSpan | null;
  content_hash: string;
  excerpt: string;
  retrieval_reason: string;
  score: number;
  trust_level: TrustLevel;
  citation_label: string;
  stale: boolean;
  /** 预览组装（执行前）为 true；正式执行后由后端刷新 */
  provisional?: boolean;
  web?: WebEvidenceMeta | null;
}

export type AssistantIntent =
  | "chat"
  | "knowledge"
  | "writing"
  | "citation"
  | "organize"
  | "research"
  | "chapter"
  | "document";

export type AssistantSurfaceState =
  | "conversation"
  | "inline_suggestion"
  | "diff_review"
  | "research_focus";

export type AssistantContextSource =
  | "none"
  | "document"
  | "selection"
  | "scope";

export type AssistantTaskStatus =
  | "idle"
  | "running"
  | "awaiting_confirmation"
  | "completed"
  | "error";

export interface AssistantActionState {
  intent: AssistantIntent;
  status: AssistantTaskStatus;
  label: string;
  surface?: AssistantSurfaceState;
  contextSource?: AssistantContextSource;
  detail?: string | null;
}

/** 编辑器写作上下文（统一助手 / 写作 IPC） */
export interface WritingEditorContext {
  selection: string;
  cursorContext: string;
}

/** 统一助手 IPC 请求（`assistant_execute`） */
export interface AssistantExecuteRequest {
  intent: AssistantIntent;
  message: string;
  notePath?: string | null;
  noteContent?: string | null;
  webAuthorized?: boolean;
  selection?: string | null;
  cursorContext?: string | null;
  paragraphText?: string | null;
  contextScope?: ContextScope;
  sessionId?: number | null;
  selectedPacketIds?: string[];
  chapter?: ChapterInfo | null;
  documentCheckType?: DocumentCheckType | null;
  organizeTaskType?: string | null;
  baseContentHash?: string | null;
  /** 为 true 时后端创建新 session，不加载同场景+笔记路径下的旧对话 */
  newSession?: boolean;
}

export interface AiChatExecutePayload {
  request_id: string;
  session_id: number;
  status: string;
  content?: string;
  tool_calls?: Array<{
    id: string;
    name?: string;
    function?: { name: string; arguments?: string };
  }>;
  tool_results?: Array<{
    tool_call_id: string;
    status: string;
    result?: unknown;
    error?: string;
  }>;
  harness_rounds?: number;
  usage?: TokenUsage;
  citation_valid?: boolean;
  /** 冷启动 + 工具检索合并后的证据包 */
  evidence_packets?: ContextPacket[];
  pending_confirmation?: boolean;
  /** 正式执行与预览证据不一致时的提示 */
  evidence_refresh_notice?: string | null;
  web_search_meta?: {
    injected: boolean;
    result_count: number;
    used_local_date: boolean;
    backend?: string;
  } | null;
}

export interface ChapterWritingResult {
  request_id: string;
  suggestions: WritingSuggestion[];
  patches: PatchProposal[];
  evidence_used: ContextPacket[];
  total_tokens: TokenUsage;
}

export interface DocumentCheckResult {
  request_id: string;
  check_type: DocumentCheckType;
  analysis_summary?: string | null;
  outline_result?: { issues: Array<{ description: string }> } | null;
  citation_gap_result?: {
    uncited_claims: Array<{ statement: string }>;
  } | null;
  style_result?: {
    inconsistencies: Array<{ description: string }>;
  } | null;
  patches: PatchProposal[];
  evidence_used: ContextPacket[];
  total_tokens: TokenUsage;
}

/** Wire artifact from harness task layer (camelCase via IPC). */
export interface HarnessArtifactWire {
  kind: string;
  title: string;
  status: string;
  sourceTask: string;
  evidenceCount: number;
  payload: unknown;
}

export type AssistantExecuteBody =
  | { kind: "chat"; payload: AiChatExecutePayload }
  | { kind: "writing"; payload: WritingTaskResult }
  | { kind: "citation"; payload: CitationCheckResult }
  | { kind: "organize"; payload: OrganizeTaskResult }
  | { kind: "research"; payload: ResearchFocusPayload }
  | { kind: "chapter"; payload: ChapterWritingResult }
  | { kind: "document"; payload: DocumentCheckResult };

/** Flattened harness metadata + task body (serde flatten on backend). */
export type AssistantExecuteResponse = AssistantExecuteBody & {
  requestId: string;
  runStatus: string;
  artifacts: HarnessArtifactWire[];
  evidenceRefreshNotice?: string | null;
};

/** 研究任务结果（与 `ResearchFocusView` 对齐） */
export interface ResearchFocusPayload {
  request_id: string;
  topic: string;
  rounds: number;
  summary: string;
  evidence_matrix: {
    total_evidence_count: number;
    coverage_score: number;
    global_gaps: string[];
    propositions: Array<{
      id: string;
      statement: string;
      evidence: Array<{
        id: string;
        title: string;
        citation_label: string;
        score: number;
      }>;
      gaps: string[];
    }>;
  };
  argument_chain: {
    has_contradictions: boolean;
    chain_strength: number;
    links: Array<{
      from_proposition_id: string;
      to_proposition_id: string;
      link_type: string;
      strength: number;
    }>;
  };
  total_tokens: TokenUsage;
}

/** 文档级检查类型 */
export type DocumentCheckType =
  | "outline_check"
  | "citation_gap_check"
  | "style_consistency"
  | "cross_doc_reference";

export interface ChapterInfo {
  heading_level: number;
  heading_text: string;
  content_start: number;
  content_end: number;
  content: string;
  heading_path: string;
}

export interface ToolSpec {
  name: string;
  description: string;
  input_schema: Record<string, unknown>;
  access_level: ToolAccessLevel;
  scene_allowlist: AiScene[];
  requires_confirmation: boolean;
  max_results: number | null;
}

export interface ToolCallResult {
  tool_name: string;
  success: boolean;
  output: unknown;
  duration_ms: number;
  tokens_used?: number;
  error?: string;
}

/** Retrieval scope from `@` mentions (IPC camelCase). */
export interface ContextScope {
  paths: string[];
  pathPrefixes: string[];
  corpusIds?: string[];
}

export interface ContextStatus {
  regulations_loaded: number;
  model_essays_loaded: number;
  anchors_loaded: number;
  links_loaded: number;
  total_tokens_estimate: number;
}

/** 最近一次发送时联网检索注入情况（由后端返回） */
export interface WebSearchMeta {
  injected: boolean;
  result_count: number;
  used_local_date: boolean;
}

export interface AssembledContext {
  packets: ContextPacket[];
  tools: ToolSpec[];
  context_status: ContextStatus;
  /** 预览上下文（尚未正式发送） */
  provisional?: boolean;
  /** 由 context planner 生成的检索执行计划 */
  execution_plan?: ExecutionPlan | null;
}

export interface ToolConfirmRequest {
  request_id: string;
  tool_call_id: string;
  decision: "approve" | "reject" | "modify";
  modified_args?: unknown;
}

export type ToolCallStatus =
  | "pending"
  | "running"
  | "completed"
  | "failed"
  | "rejected";

export interface ToolCallInfo {
  id: string;
  name: string;
  arguments?: Record<string, unknown>;
  status: ToolCallStatus;
  result_summary?: string;
  error?: string;
  duration_ms?: number;
  tokens_used?: number;
}

/// 检索计划步骤
export interface RetrievalStep {
  layer: "fts" | "vector" | "graph" | "exact" | "template";
  query: string;
  expected_results: number;
  priority: number;
}

/// 执行计划
export interface ExecutionPlan {
  steps: RetrievalStep[];
  estimated_tokens: number;
  estimated_duration_ms: number;
}

// Scene display metadata
export interface SceneMeta {
  scene: AiScene;
  label: string;
  description: string;
  icon: string;
  defaultScope: "global" | "document";
}

export type EvidenceRelationType =
  | "supports"
  | "contradicts"
  | "prerequisite"
  | "consequence"
  | "parallel";

export interface EvidenceRelation {
  sourceId: string;
  targetId: string;
  relationType: EvidenceRelationType;
  confidence: number;
}

export interface EvidenceChain {
  packets: ContextPacket[];
  relations: EvidenceRelation[];
}

// ─── PatchProposal ───────────────────────────────────────

/** 受控编辑补丁 — AI 对 Markdown 的所有正文写入都必须走此结构 */
export interface PatchProposal {
  /** 补丁唯一 ID */
  id: string;
  /** 目标文件相对路径 */
  target_path: string;
  /** 基准内容哈希（SHA-256），用于变更检测 */
  base_content_hash: string;
  /** 替换范围（字符偏移） */
  range: SourceSpan;
  /** 原始文本（range 内的文本） */
  original_text: string;
  /** 替换文本 */
  replacement_text: string;
  /** 关联的证据包 ID 列表 */
  evidence_packet_ids: string[];
  /** 风险等级 */
  risk_level: RiskLevel;
  /** 警告信息列表 */
  warnings: string[];
  /** 创建时间（ISO 8601） */
  created_at: string;
}

/** 补丁风险等级 */
export type RiskLevel = "low" | "medium" | "high";

/** 补丁应用结果 */
export interface PatchApplyResult {
  /** 是否成功应用 */
  success: boolean;
  /** 应用后的新内容哈希 */
  new_content_hash?: string;
  /** 错误信息（失败时有值） */
  error?: string;
  /** 警告信息 */
  warnings: string[];
}

/** 补丁验证错误类型 */
export type PatchValidationErrorType =
  | "hash_mismatch"
  | "range_out_of_bounds"
  | "text_mismatch"
  | "file_not_found";

/** 补丁验证错误详情 */
export interface PatchValidationError {
  type: PatchValidationErrorType;
  message: string;
  expected?: string;
  actual?: string;
}

// ─── Chunked Patch Types ─────────────────────────────────

/** 块类型 */
export type ChunkType = "rewrite" | "insert" | "delete" | "move";

/** 补丁块 */
export interface PatchChunk {
  /** 块 ID */
  id: string;
  /** 替换范围（字符偏移） */
  range: SourceSpan;
  /** 原始文本 */
  original_text: string;
  /** 替换文本 */
  replacement_text: string;
  /** 块类型 */
  chunk_type: ChunkType;
  /** 应用顺序 */
  order: number;
}

/** 分块补丁 — 多个相关补丁的集合 */
export interface ChunkedPatchProposal {
  /** 分块补丁唯一 ID */
  id: string;
  /** 目标文件相对路径 */
  target_path: string;
  /** 基准内容哈希（SHA-256） */
  base_content_hash: string;
  /** 补丁块列表（按应用顺序排列） */
  chunks: PatchChunk[];
  /** 整体描述 */
  description: string;
  /** 整体风险等级 */
  risk_level: RiskLevel;
  /** 警告信息列表 */
  warnings: string[];
  /** 创建时间（ISO 8601） */
  created_at: string;
}

// ─── Writing Workflow ────────────────────────────────────

/** 写作任务输入 */
export interface WritingTaskInput {
  /** 目标文件相对路径 */
  target_path: string;
  /** 基准内容哈希 */
  base_content_hash: string;
  /** 选中文本（可选） */
  selection?: string;
  /** 光标邻域上下文 */
  cursor_context: string;
  /** 写作目标 */
  writing_goal: string;
  /** 是否允许联网 */
  web_authorized: boolean;
}

/** 写作意图 */
export type WritingIntent =
  // 选区级
  | "continue"
  | "rewrite"
  | "add_evidence"
  | "outline"
  | "unify_tone"
  // 章节级
  | "chapter_rewrite"
  | "chapter_continue"
  | "chapter_restructure"
  // 文档级
  | "outline_check"
  | "citation_gap_check"
  | "style_consistency"
  | "cross_doc_reference";

/** 写作意图级别 */
export type WritingIntentLevel = "selection" | "chapter" | "document";

/** 写作建议 */
export interface WritingSuggestion {
  /** 建议 ID */
  id: string;
  /** 写作意图 */
  intent: WritingIntent;
  /** 解释说明 */
  explanation: string;
  /** 置信度 */
  confidence: number;
}

/** 写作任务结果 */
export interface WritingTaskResult {
  /** 请求 ID */
  request_id: string;
  /** 写作建议列表 */
  suggestions: WritingSuggestion[];
  /** 补丁列表 */
  patches: PatchProposal[];
  /** 使用的证据包 */
  evidence_used: ContextPacket[];
  /** Token 消耗 */
  total_tokens: TokenUsage;
}

/** Token 使用量 */
export interface TokenUsage {
  prompt_tokens: number;
  completion_tokens: number;
  total_tokens: number;
  prompt_cache_hit_tokens?: number;
  prompt_cache_miss_tokens?: number;
}

// ─── Citation Check Workflow ─────────────────────────────

/** 引用检查输入 */
export interface CitationCheckInput {
  /** 段落/选区文本 */
  paragraph_text: string;
  /** 文档路径 */
  document_path: string;
  /** 检索范围（可选） */
  scope?: ContextScope;
  /** 是否允许联网 */
  web_authorized: boolean;
}

/** 事实声明 */
export interface FactClaim {
  /** 声明 ID */
  id: string;
  /** 声明内容 */
  statement: string;
  /** 是否有支持证据 */
  has_support: boolean;
  /** 支持证据包 ID */
  supporting_evidence: string[];
  /** 冲突证据包 ID */
  conflicting_evidence: string[];
}

/** 引用覆盖度 */
export type CitationCoverage =
  | "well_supported"
  | "partially_supported"
  | "weakly_supported"
  | "unsupported"
  | "contradicted";

/** 引用建议动作 */
export type CitationAction =
  | "add_citation"
  | "rewrite"
  | "remove_claim"
  | "add_qualifier";

/** 引用建议 */
export interface CitationSuggestion {
  /** 关联的声明 ID */
  claim_id: string;
  /** 建议动作 */
  action: CitationAction;
  /** 建议的引用文本（可选） */
  suggested_citation?: string;
  /** 解释说明 */
  explanation: string;
}

/** 引用检查结果 */
export interface CitationCheckResult {
  /** 请求 ID */
  request_id: string;
  /** 事实声明列表 */
  claims: FactClaim[];
  /** 覆盖度评估 */
  coverage: CitationCoverage;
  /** 建议列表 */
  suggestions: CitationSuggestion[];
  /** 使用的证据包 */
  evidence_used: ContextPacket[];
  /** Token 消耗 */
  total_tokens: TokenUsage;
}

// ─── Organize Workflow ───────────────────────────────────

/** 整理建议类型 */
export type OrganizeSuggestionType =
  | "rename_title"
  | "add_tag"
  | "move_to_folder"
  | "assign_corpus"
  | "add_block_link"
  | "extract_template";

/** 整理建议 */
export interface OrganizeSuggestion {
  /** 建议 ID */
  id: string;
  /** 建议类型 */
  suggestion_type: OrganizeSuggestionType;
  /** 目标文件路径 */
  target_path: string;
  /** 当前值（可选） */
  current_value?: string;
  /** 建议值 */
  suggested_value: string;
  /** 建议理由 */
  reason: string;
  /** 来源 */
  source: string;
  /** 置信度 (0.0-1.0) */
  confidence: number;
  /** 关联的证据包 ID */
  evidence_packet_ids: string[];
}

/** 批量变更计划 */
export interface OrganizeBatch {
  /** 计划 ID */
  id: string;
  /** 计划标题 */
  title: string;
  /** 计划描述 */
  description: string;
  /** 建议列表 */
  suggestions: OrganizeSuggestion[];
  /** 创建时间 */
  created_at: string;
}

/** 整理任务范围 */
export interface OrganizeTaskScope {
  /** 文件路径列表 */
  paths: string[];
  /** 路径前缀 */
  path_prefixes: string[];
  /** 语料库 ID */
  corpus_ids?: string[];
}

/** 整理任务类型 */
export type OrganizeTaskType =
  | "full_audit"
  | "title_suggestions"
  | "tag_suggestions"
  | "folder_suggestions"
  | "link_suggestions";

/** 整理任务输入 */
export interface OrganizeTaskInput {
  /** 整理范围（可选） */
  scope?: OrganizeTaskScope;
  /** 任务类型 */
  task_type: OrganizeTaskType;
}

/** 整理任务结果 */
export interface OrganizeTaskResult {
  /** 请求 ID */
  request_id: string;
  /** 批量变更计划 */
  batch: OrganizeBatch;
  /** Token 消耗 */
  total_tokens: TokenUsage;
}
