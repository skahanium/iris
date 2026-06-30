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
  | "write_settings"
  | "manage_skills";

export type AgentPermissionAtom =
  | "vault.read"
  | "vault.search"
  | "vault.write.patch"
  | "vault.create_note"
  | "vault.rename_move"
  | "vault.delete_to_trash"
  | "vault.assets.read"
  | "vault.assets.write"
  | "vault.versioning"
  | "fs.pick_file"
  | "fs.pick_folder"
  | "fs.import_to_vault"
  | "fs.export"
  | "fs.read_authorized_folder"
  | "fs.write_authorized_export"
  | "doc.convert"
  | "doc.ocr"
  | "doc.extract_pdf"
  | "doc.extract_table"
  | "doc.normalize_markdown"
  | "doc.fix_links"
  | "doc.extract_citations"
  | "web.search"
  | "web.fetch"
  | "web.to_markdown"
  | "web.download_to_assets"
  | "web.citation_extract"
  | "net.localhost"
  | "process.run_markdown_tool"
  | "process.run_readonly"
  | "process.run_mutating"
  | "process.run_network"
  | "process.long_running"
  | "process.kill_owned"
  | "git.read_status"
  | "git.read_diff"
  | "git.read_log"
  | "git.write_commit"
  | "clipboard.write"
  | "clipboard.read"
  | "browser.read_page"
  | "browser.screenshot"
  | "browser.control_page"
  | "secret.exists"
  | "secret.create_update"
  | "secret.read_plaintext"
  | "app_state.read"
  | "app_state.write";

export type PermissionRiskLevel = "low" | "medium" | "high" | "critical";

export type PermissionScopeKind =
  | "request"
  | "session"
  | "vault"
  | "folder"
  | "skill"
  | "global";

export type PermissionDecision =
  | "allow"
  | "allow_once"
  | "allow_for_session"
  | "deny_once"
  | "deny_always_for_this_skill"
  | "open_settings";

export interface PermissionEffectSummary {
  permissionName: AgentPermissionAtom;
  scopeKind: PermissionScopeKind;
  scopeSummary: string;
  riskLevel: PermissionRiskLevel;
  reversibleBy: string;
  blockedReason?: string | null;
}

export interface ToolPermissionPreflight {
  toolName: string;
  decision: PermissionDecision;
  effects: PermissionEffectSummary[];
  blocked: boolean;
}

/** UTF-8 byte offsets into a Markdown source string. */
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
  provider_id?: string | null;
  provider_kind?: string | null;
  raw_result_hash?: string | null;
  extraction_method?: string | null;
  conflict_group?: string | null;
  conflict_note?: string | null;
  failure_reason?: string | null;
  fallback_from?: WebSearchBackend | null;
}

export interface WebEvidenceBrokerItem {
  url: string;
  canonical_url: string;
  title: string;
  domain: string;
  snippet: string;
  fetched_excerpt?: string | null;
  provider_id: string;
  provider_kind: string;
  cost_class: string;
  raw_result_hash: string;
  extraction_method: string;
  trust_level: "external_untrusted" | string;
  retrieval_reason: string;
  search_backend: WebSearchBackend;
  source_rank: WebSourceRank;
  freshness_label?: string | null;
  failure_reason?: string | null;
  conflict_group?: string | null;
  conflict_note?: string | null;
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

export type AgentIntent =
  | "chat"
  | "ask_notes"
  | "rewrite_selection"
  | "write"
  | "research"
  | "organize"
  | "citation_check"
  | "chapter"
  | "document_check"
  | "vision_chat"
  | "skill_management";

export type CapabilitySlot =
  | "fast"
  | "writer"
  | "reasoner"
  | "long_context"
  | "vision"
  | "agent_tools"
  | "embedding"
  | "reranker"
  | "local_private";

export type TaskPlanIntent =
  | "chat"
  | "ask_notes"
  | "creative_write"
  | "rewrite_selection"
  | "citation_check"
  | "research"
  | "organize"
  | "document_check"
  | "chapter"
  | "vision_chat"
  | "skill_management";

export type TaskPlanConfidence = "high" | "medium" | "low";

export type RetrievalMode =
  | "none"
  | "current_reference"
  | "local_notes"
  | "scoped_notes"
  | "long_document";

export type WebMode = "disabled" | "brokered";

export type ExecutionMode =
  | "direct_answer"
  | "context_answer"
  | "writing_candidate"
  | "patch_proposal"
  | "structured_task"
  | "long_task"
  | "clarification";

export type OutputMode =
  | "markdown_message"
  | "artifact_backed_message"
  | "confirmation_required"
  | "diagnostic";

export interface ContextReference {
  id: string;
  kind: "selection" | "paragraph" | "heading" | "note" | "artifact";
  filePath: string | null;
  contentHash: string | null;
  utf8Range: SourceSpan | null;
  editorRange: { from: number; to: number } | null;
  excerpt: string;
  headingPath?: string | null;
  anchor?: string | null;
  stale: boolean;
  invalidReason?: string | null;
}

export interface ArtifactPlanItem {
  kind:
    | "evidence_sources"
    | "writing_change"
    | "structured_result"
    | "task_process";
  reason: string;
  valueGate: string;
}

export interface TaskPlan {
  intent: TaskPlanIntent;
  confidence: TaskPlanConfidence;
  contextReferences: ContextReference[];
  retrievalMode: RetrievalMode;
  webMode: WebMode;
  modelSlot: CapabilitySlot;
  executionMode: ExecutionMode;
  outputMode: OutputMode;
  artifactPlan: ArtifactPlanItem[];
  requiresClarification: boolean;
  clarificationQuestion?: string | null;
  sourceHints: string[];
}

export interface CapabilityRouteSummary {
  slot: CapabilitySlot;
  providerId: string;
  model: string;
  fallbackChain: CapabilitySlot[];
  reason: string;
  probeStatus: string;
  degraded: boolean;
}

export interface PersonaLayerSummary {
  layer: string;
  summary: string;
}

export type SkillCapabilitySupportStatus =
  | "supported"
  | "supported_with_confirmation"
  | "planned"
  | "unsupported_by_product_scope"
  | "blocked_by_policy"
  | "missing_user_grant";

export interface BlockedCapabilitySummary {
  skillName: string;
  capability: string;
  status: SkillCapabilitySupportStatus;
  riskLevel: string;
  permission?: ToolAccessLevel | null;
  fallbackGuidance: string;
}

export type SkillConfirmationStatus = "confirmed" | "needs_confirmation";

export interface SkillScopeRule {
  kind: string;
  pattern: string;
}

export interface SkillActivationItemSummary {
  name: string;
  scope: string;
  score: number;
  matchReason: string;
  injectedSections: string[];
  degradedReasons: string[];
  requestedTools: string[];
  confirmationRequiredTools: string[];
  blockedCapabilities: BlockedCapabilitySummary[];
}

export interface SkillActivationPlanSummary {
  activatedSkills: SkillActivationItemSummary[];
  requestedTools: string[];
  confirmationRequiredTools: string[];
  blockedCapabilities: BlockedCapabilitySummary[];
  skillOverlaySummary: string;
  degraded: boolean;
}

export interface AgentAuditSummary {
  toolEvents: number;
  confirmedTools: number;
  deniedTools: number;
  sanitized: boolean;
}

export interface PermissionPreflightSummary {
  summary: string;
  requiredConfirmations: string[];
  blockedCapabilities: BlockedCapabilitySummary[];
  missingUserGrants: string[];
  exposedTools: string[];
  degraded: boolean;
}

export interface IntentDetectionResult {
  detectedIntent: AgentIntent;
  confidence: number;
  reason: string;
  alternatives: AgentIntent[];
  fallbackBehavior: string;
  sourceHints: string[];
}

export interface AgentRunPlanSummary {
  requestId: string;
  detectedIntent: AgentIntent;
  legacyScene: AiScene;
  contextSummary: string[];
  toolSummary: string;
  permissionSummary: string;
  progressState: string;
  blockedReasons: string[];
  degraded: boolean;
  modelRoute?: CapabilityRouteSummary | null;
  personaLayers?: PersonaLayerSummary[];
  skillActivationPlan?: SkillActivationPlanSummary | null;
  blockedCapabilities?: BlockedCapabilitySummary[];
  auditSummary?: AgentAuditSummary | null;
}

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
  | "paused_budget"
  | "paused_recoverable"
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
  aiDomain?: "normal" | "classified";
  agentIntent?: AgentIntent;
  intent?: AssistantIntent;
  intentDetection?: IntentDetectionResult | null;
  taskPlan?: TaskPlan;
  contextReferences?: ContextReference[];
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
  /** 图片附件（多模态消息） */
  images?: import("./ipc").ImageAttachmentDto[];
}

export interface AiChatExecutePayload {
  request_id: string;
  task_id?: string;
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
  usage_source?: "provider" | "estimated";
  citation_valid?: boolean;
  /** 冷启动 + 工具检索合并后的证据包 */
  evidence_packets?: ContextPacket[];
  pending_confirmation?: boolean;
  deliberation_state?: DeliberationState | null;
  verification_summary?: VerificationSummary | null;
  resumed?: boolean;
  /** 正式执行与预览证据不一致时的提示 */
  evidence_refresh_notice?: string | null;
  web_search_meta?: {
    injected: boolean;
    result_count: number;
    used_local_date: boolean;
    backend?: string;
  } | null;
}

export type VerificationStatus = "pending" | "passed" | "failed";

export interface VerificationItem {
  id: string;
  description: string;
  status: VerificationStatus;
}

export interface DeliberationState {
  request_id: string;
  session_id: number;
  current_goal: string;
  plan_outline: string[];
  assumptions: string[];
  open_questions: string[];
  evidence_gaps: string[];
  verification_items: VerificationItem[];
  status: string;
}

export interface VerificationSummary {
  passed: boolean;
  items: VerificationItem[];
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
  check_type: string;
  outline_result?: OutlineCheckResult;
  citation_gap_result?: CitationGapCheckResult;
  style_result?: StyleCheckResult;
  patches: PatchProposal[];
  evidence_used: ContextPacket[];
  total_tokens: TokenUsage;
  analysis_summary?: string | null;
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
  taskId?: string;
  runStatus: string;
  artifacts: HarnessArtifactWire[];
  evidenceRefreshNotice?: string | null;
  intentDetection?: IntentDetectionResult | null;
  taskPlan?: TaskPlan | null;
  runPlanSummary?: AgentRunPlanSummary | null;
  permissionPreflightSummary?: PermissionPreflightSummary | null;
};

/** 研究任务结果（对话摘要 + artifact 工作区视图使用）。 */
export interface ResearchFocusPayload {
  request_id: string;
  topic: string;
  rounds: number;
  summary: string;
  evidence_matrix: {
    total_evidence_count: number;
    coverage_score: number;
    global_gaps: string[];
    propositions: ResearchProposition[];
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
  research_state?: ResearchState;
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
  /** 重要文稿协作状态 */
  writing_state?: WritingState;
}

export interface WritingRevisionRecord {
  patch_id: string;
  scope: string;
  reason: string;
  risk: string;
  rollback: string;
  evidence_packet_ids: string[];
}

export interface WritingState {
  request_id: string;
  target_path: string;
  document_goal: string;
  audience: string;
  genre: string;
  structure_outline: string[];
  key_arguments: string[];
  material_packet_ids: string[];
  citation_labels: string[];
  style_constraints: string[];
  revision_records: WritingRevisionRecord[];
  draft_version_hash: string;
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

// ─── Research Execute Result ─────────────────────────────

export interface EvidenceBrief {
  id: string;
  title: string;
  citation_label: string;
  score: number;
}

/** 研究命题 — EvidenceMatrix 的子元素 */
export interface ResearchProposition {
  id: string;
  statement: string;
  evidence: EvidenceBrief[];
  gaps: string[];
}

/** 证据矩阵 — research_execute 的核心结构 */
export interface EvidenceMatrix {
  topic: string;
  propositions: ResearchProposition[];
  global_gaps: string[];
  total_evidence_count: number;
  coverage_score: number;
}

/** 论证链中的单条链接 */
export interface ArgumentLink {
  from_proposition_id: string;
  to_proposition_id: string;
  link_type: string;
  strength: number;
}

/** 论证链 — 描述命题之间的推理关系 */
export interface ArgumentChain {
  links: ArgumentLink[];
  has_contradictions: boolean;
  chain_strength: number;
}

/** `research_execute` IPC 返回值 */
export interface ResearchExecuteResult {
  request_id: string;
  topic: string;
  rounds: number;
  evidence_matrix: EvidenceMatrix;
  argument_chain: ArgumentChain;
  summary: string;
  total_tokens: TokenUsage;
  research_state?: ResearchState;
}

export interface EvidenceItem {
  evidence_id: string;
  citation_label: string;
  source_type: string;
  title: string;
  credibility: string;
  freshness: string;
  score: number;
}

export interface ConclusionBoundary {
  statement: string;
  evidence_item_ids: string[];
  boundary: string;
  inference: boolean;
}

export interface ResearchState {
  request_id: string;
  research_question: string;
  sub_questions: string[];
  sources: EvidenceItem[];
  credibility_summary: string;
  freshness_summary: string;
  conflicts: string[];
  counter_arguments: string[];
  evidence_gaps: string[];
  preliminary_conclusions: ConclusionBoundary[];
}

/** 研究进度事件（`ai:research_progress`） */
export interface ResearchProgressEvent {
  request_id: string;
  topic: string;
  state: string;
  current_round: number;
  max_rounds: number;
  queries_executed: string[];
  new_evidence_count: number;
  total_evidence_count: number;
  tokens_used: number;
  token_budget: number;
  progress_pct: number;
  round_terminated_early: boolean;
}

// ─── AI Send Message Result ──────────────────────────────

/** AI 工具调用 */
export interface AiToolCall {
  id: string;
  function: { name: string; arguments: string };
}

/** AI 工具执行结果 */
export interface AiToolResult {
  tool_call_id: string;
  status: string;
  result?: unknown;
}

export interface ToolExecutionOutcome {
  status: "succeeded" | "failed" | "rejected" | string;
  sideEffectCommitted: boolean;
  toolName?: string | null;
  resultSummary?: string | null;
}

export interface AssistantResumeOutcome {
  status: "resumed" | "skipped" | "failed" | string;
  failureClass?: string | null;
  userMessage?: string | null;
}

/** `ai_send_message` IPC 返回值 */
export interface AiSendMessageResult {
  request_id: string;
  task_id?: string;
  session_id: number;
  status: string;
  content?: string;
  tool_calls?: AiToolCall[];
  tool_results?: AiToolResult[];
  usage?: TokenUsage;
  usage_source?: "provider" | "estimated";
  citation_valid?: boolean;
  harness_rounds?: number;
  evidence_packets?: ContextPacket[];
  pending_confirmation?: boolean;
  deliberation_state?: DeliberationState | null;
  verification_summary?: VerificationSummary | null;
  evidence_refresh_notice?: string | null;
  resumed?: boolean;
  tool_confirmation_partial?: boolean;
  resume_error_code?: string;
  resume_error_message?: string;
  toolExecutionOutcome?: ToolExecutionOutcome;
  assistantResumeOutcome?: AssistantResumeOutcome;
}

// ─── Document Check Sub-types ────────────────────────────

/** 大纲检查 — 单条问题 */
export interface OutlineIssue {
  issue_type: string;
  heading_path: string;
  description: string;
  severity: string;
  position: number;
}

/** 大纲检查 — 单条建议 */
export interface OutlineSuggestionItem {
  suggestion: string;
  position: number;
  requires_patch: boolean;
}

/** 大纲条目 */
export interface OutlineEntry {
  level: number;
  text: string;
  position: number;
  word_count: number;
}

/** 大纲检查结果（DocumentCheckResult 子结构） */
export interface OutlineCheckResult {
  issues: OutlineIssue[];
  suggestions: OutlineSuggestionItem[];
  outline_entries: OutlineEntry[];
}

/** 弱引用记录 */
export interface WeakCitation {
  claim: string;
  current_citation: string;
  reason: string;
  suggested_citation?: string;
}

/** 引用缺口检查结果（DocumentCheckResult 子结构） */
export interface CitationGapCheckResult {
  uncited_claims: FactClaim[];
  weak_citations: WeakCitation[];
  suggestions: CitationSuggestion[];
}

/** 风格不一致记录 */
export interface StyleInconsistency {
  inconsistency_type: string;
  location: string;
  description: string;
  examples: string[];
}

/** 风格检查 — 单条建议 */
export interface StyleSuggestionItem {
  suggestion: string;
  locations: string[];
  requires_patch: boolean;
}

/** 风格一致性检查结果（DocumentCheckResult 子结构） */
export interface StyleCheckResult {
  inconsistencies: StyleInconsistency[];
  suggestions: StyleSuggestionItem[];
  consistency_score: number;
}

// ─── IPC Result Aliases ──────────────────────────────────

/** `writing_execute` IPC 返回值（结构同 WritingTaskResult） */
export type WritingExecuteResult = WritingTaskResult;

/** `chapter_writing_execute` IPC 返回值（结构同 ChapterWritingResult） */
export type ChapterWritingExecuteResult = ChapterWritingResult;

/** 消息内容：纯文本字符串或多模态片段数组 */
export type MessageContent = string | ContentPart[];

/** 内容片段（遵循 OpenAI multimodal 格式） */
export type ContentPart =
  | { type: "text"; text: string }
  | {
      type: "image_url";
      image_url: { url: string; detail?: "auto" | "low" | "high" };
    };

/** `organize_execute` IPC 返回值（结构同 OrganizeTaskResult） */
export type OrganizeExecuteResult = OrganizeTaskResult;

// ─── AI Dual-Domain Types ────────────────────────────────

export type AiDomain = "normal" | "classified";

export type AiConversationRef =
  | { domain: "normal"; sessionId: number | null }
  | { domain: "classified"; threadId: string | null; documentPath: string };

export interface AssistantRequestContext {
  domain: AiDomain;
  notePath: string | null;
  contextReferences: ContextReference[];
  classifiedThreadId?: string | null;
}
