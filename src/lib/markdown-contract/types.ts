/**
 * Markdown 契约内核 — 共享类型定义（子项目 1）
 *
 * 本文件定义全应用共享的 Markdown contract 接口。所有类型必须有 JSDoc 注释说明其用途。
 * 后续"编辑器重构"和"AI 展示重构"都必须消费这些类型。
 *
 * @module markdown-contract/types
 * @see MARKDOWN_CONTRACT_PLAN.md
 */

// ── Render Profile ──────────────────────────────────────────────

/**
 * 消费表面标识符。
 *
 * 同一个 Markdown 源在不同 profile 下的渲染策略和能力边界不同。
 * 差异来自各 profile 的能力（例如编辑器可以显示只读占位块，聊天区可以流式渲染）。
 *
 * 首批 5 个，后续可扩展。
 */
export type MarkdownProfile =
  | "chat_assistant"
  | "chat_user"
  | "editor_ingest"
  | "editor_export"
  | "vault_preview"
  | "research_card"
  | "patch_preview"
  | "citation_panel";

/** 首批必须实现的 profiles */
export const REQUIRED_PROFILES: readonly MarkdownProfile[] = [
  "chat_assistant",
  "chat_user",
  "editor_ingest",
  "editor_export",
  "vault_preview",
] as const;

// ── Capability Level ────────────────────────────────────────────

/**
 * 语法片段的能力等级。
 *
 * | Level          | 含义                                                   |
 * |----------------|-------------------------------------------------------|
 * | `native`       | 原生支持，可稳定渲染且 round-trip 无损                  |
 * | `render_only`  | 可高保真渲染，但暂不保证安全编辑 / 完整回写              |
 * | `preserve_only`| 必须原样保留，允许只读展示或占位，不允许破坏原文          |
 * | `unsupported`  | 当前无法安全支持，必须明确降级策略，不得默默吞掉          |
 */
export type MarkdownCapabilityLevel =
  | "native"
  | "render_only"
  | "preserve_only"
  | "unsupported";

// ── Syntax Kinds ────────────────────────────────────────────────

/**
 * 文档中可出现的语法族标识。
 *
 * 分类器（classifyMarkdownCapabilities）遍历 marked lexer token 树时，
 * 按 token.type 映射到以下 syntaxKind 之一。
 */
export type MarkdownSyntaxKind =
  // Native GFM
  | "heading"
  | "paragraph"
  | "bold"
  | "italic"
  | "strikethrough"
  | "inline_code"
  | "list"
  | "task_list"
  | "table"
  | "code_block"
  | "blockquote"
  | "link"
  | "image"
  | "horizontal_rule"
  | "wiki_link"
  | "text"
  | "space"
  // Render only
  | "callout"
  | "footnote_ref"
  | "footnote_def"
  // Preserve only
  | "raw_html"
  | "html_comment"
  // Unsupported
  | "unknown";

/** native 级别的 syntaxKind 集合 */
export const NATIVE_SYNTAX_KINDS = new Set<MarkdownSyntaxKind>([
  "heading",
  "paragraph",
  "bold",
  "italic",
  "strikethrough",
  "inline_code",
  "list",
  "task_list",
  "table",
  "code_block",
  "blockquote",
  "link",
  "image",
  "horizontal_rule",
  "wiki_link",
  "text",
  "space",
]);

/** render_only 级别的 syntaxKind 集合 */
export const RENDER_ONLY_SYNTAX_KINDS = new Set<MarkdownSyntaxKind>([
  "callout",
  "footnote_ref",
  "footnote_def",
]);

/** preserve_only 级别的 syntaxKind 集合 */
export const PRESERVE_ONLY_SYNTAX_KINDS = new Set<MarkdownSyntaxKind>([
  "raw_html",
  "html_comment",
]);

// ── Syntax Fragment ─────────────────────────────────────────────

/**
 * 被分类后的一个语法片段。
 *
 * 每个片段覆盖源文档中一段连续的字符范围，不重叠、无间隙。
 */
export interface MarkdownSyntaxFragment {
  /** 原始源码文本（Markdown 原文的切片） */
  raw: string;
  /** 语法族标识 */
  syntaxKind: MarkdownSyntaxKind;
  /** 在原文中的起始字符偏移（0-based） */
  offset: number;
  /** 在原文中的结束字符偏移（不包含，offset + raw.length） */
  endOffset: number;
  /** 能力等级 */
  capability: MarkdownCapabilityLevel;
  /** 可选的渲染产物（仅在 native 或 render_only 时有值） */
  rendered?: string;
}

// ── Ingest ──────────────────────────────────────────────────────

/**
 * 源摄取操作的选项。
 */
export interface IngestOptions {
  /** 来源 profile */
  profile?: MarkdownProfile;
  /** 是否处于流式输入状态 */
  streaming?: boolean;
  /** 附加上下文（如文件路径、消息角色等） */
  context?: string;
}

/**
 * 源摄取操作的元数据。
 */
export interface MarkdownIngestSource {
  /** 来源 profile */
  profile: MarkdownProfile;
  /** 是否流式输入 */
  streaming: boolean;
  /** 附加上下文 */
  context?: string;
}

/**
 * 源摄取操作的完整产物。
 */
export interface IngestedMarkdown {
  /** 原始 Markdown 文本（不可变） */
  raw: string;
  /** 来源元数据 */
  source: MarkdownIngestSource;
  /** 解析后、未经分类的语法片段列表（由 marked lexer 产出） */
  fragments: MarkdownSyntaxFragment[];
}

// ── Classify Options ────────────────────────────────────────────

/**
 * 能力分类操作的选项。
 */
export interface ClassifyOptions {
  /** 附加上下文 */
  context?: string;
}

// ── Contract Result ─────────────────────────────────────────────

/**
 * 一次 Markdown 渲染的契约级输出。
 *
 * 包含产物 HTML、保留的原文片段、能力告警、流式修复日志和元数据。
 * 所有消费者通过此结构获取分析结果。
 */
export interface MarkdownContractResult {
  /** 渲染产物字符串（HTML 或 Markdown，取决于 profile） */
  output: string;
  /** 保留的原文片段列表（用于编辑器 serializePreservedMarkdown 回吐） */
  preserveFragments: MarkdownSyntaxFragment[];
  /** 渲染过程中产生的能力告警 */
  warnings: MarkdownCapabilityWarning[];
  /** 流式修复操作记录（仅在 streaming: true 时有内容） */
  streamRepairs: StreamRepairRecord[];
  /** 渲染产物元数据 */
  meta: MarkdownRenderMeta;
}

/**
 * 一个能力告警。
 *
 * 当源文档包含当前 profile 无法完全支持的语法时产生。
 */
export interface MarkdownCapabilityWarning {
  /** 触发告警的语法片段 */
  fragment: MarkdownSyntaxFragment;
  /** 人类可读的告警消息 */
  message: string;
  /** 告警严重级别 */
  severity: "info" | "warn";
}

/**
 * 一条流式修复记录。
 *
 * 流式输入中，未闭合的语法标记需要修补才能安全渲染。
 * 修复只作用于展示，不会写入持久化层。
 */
export interface StreamRepairRecord {
  /** 修复前的原始片段 */
  before: string;
  /** 修复后的片段 */
  after: string;
  /** 修复类型（如 close_fence、close_bold、close_strikethrough） */
  repairKind: string;
  /** 修复发生的位置（源文本字符偏移） */
  offset: number;
}

/**
 * 渲染元数据。
 */
export interface MarkdownRenderMeta {
  /** 使用的 profile */
  profile: MarkdownProfile;
  /** 是否流式模式 */
  streaming: boolean;
  /** 语法片段分类统计 */
  stats: MarkdownFragmentStats;
  /** Unix 毫秒时间戳 */
  renderedAt: number;
}

/**
 * 语法片段能力级别统计。
 */
export interface MarkdownFragmentStats {
  native: number;
  render_only: number;
  preserve_only: number;
  unsupported: number;
  /** native + render_only + preserve_only + unsupported */
  total: number;
}

// ── Render Options ──────────────────────────────────────────────

/**
 * 统一渲染入口 renderMarkdownWithProfile 的选项。
 */
export interface RenderOptions {
  /** 是否流式模式 */
  streaming?: boolean;
  /** 附加上下文 */
  context?: string;
  /** 是否对 preserve_only 片段使用占位标记（编辑器等需要） */
  usePlaceholders?: boolean;
}

// ── Profile-specific Rendering Rules ────────────────────────────

/**
 * 每个 profile 对特定能力等级的渲染策略。
 *
 * 策略说明：
 * - `full_render`: 完全渲染为 HTML
 * - `placeholder`: 用占位标记包裹，显示为只读块
 * - `raw_preserve`: 原样保留原文
 * - `omit`: 省略不渲染
 * - `warning_block`: 渲染为带有能力提示的警告块
 */
export interface ProfileRenderRule {
  profile: MarkdownProfile;
  /** 对该 profile 而言此能力等级的行为 */
  strategy:
    | "full_render"
    | "placeholder"
    | "raw_preserve"
    | "omit"
    | "warning_block";
  /** 在 preserve_only 下的占位 CSS class */
  placeholderClass?: string;
  /** 在 render_only 下的提示文字 */
  capabilityHint?: string;
}

/**
 * 默认的 profile 渲染规则表。
 *
 * 定义了每个能力等级在每个 profile 下应该如何渲染。
 * 这是契约的核心数据，所有渲染逻辑必须引此表决定策略。
 */
export const DEFAULT_PROFILE_RULES: Record<
  MarkdownCapabilityLevel,
  Record<MarkdownProfile, ProfileRenderRule>
> = {
  native: {
    chat_assistant: { profile: "chat_assistant", strategy: "full_render" },
    chat_user: { profile: "chat_user", strategy: "full_render" },
    editor_ingest: { profile: "editor_ingest", strategy: "full_render" },
    editor_export: { profile: "editor_export", strategy: "full_render" },
    vault_preview: { profile: "vault_preview", strategy: "full_render" },
    research_card: { profile: "research_card", strategy: "full_render" },
    patch_preview: { profile: "patch_preview", strategy: "full_render" },
    citation_panel: { profile: "citation_panel", strategy: "full_render" },
  },
  render_only: {
    chat_assistant: {
      profile: "chat_assistant",
      strategy: "full_render",
      capabilityHint: "此语法不支持编辑",
    },
    chat_user: {
      profile: "chat_user",
      strategy: "full_render",
      capabilityHint: "此语法不支持编辑",
    },
    editor_ingest: {
      profile: "editor_ingest",
      strategy: "placeholder",
      placeholderClass: "iris-preserve-readonly",
      capabilityHint: "此语法暂不可编辑",
    },
    editor_export: { profile: "editor_export", strategy: "raw_preserve" },
    vault_preview: { profile: "vault_preview", strategy: "full_render" },
    research_card: { profile: "research_card", strategy: "full_render" },
    patch_preview: { profile: "patch_preview", strategy: "full_render" },
    citation_panel: { profile: "citation_panel", strategy: "full_render" },
  },
  preserve_only: {
    chat_assistant: {
      profile: "chat_assistant",
      strategy: "raw_preserve",
      placeholderClass: "iris-preserve-raw",
    },
    chat_user: {
      profile: "chat_user",
      strategy: "raw_preserve",
      placeholderClass: "iris-preserve-raw",
    },
    editor_ingest: {
      profile: "editor_ingest",
      strategy: "placeholder",
      placeholderClass: "iris-preserve-readonly",
      capabilityHint: "此语法暂不支持编辑",
    },
    editor_export: { profile: "editor_export", strategy: "raw_preserve" },
    vault_preview: { profile: "vault_preview", strategy: "raw_preserve" },
    research_card: { profile: "research_card", strategy: "raw_preserve" },
    patch_preview: { profile: "patch_preview", strategy: "raw_preserve" },
    citation_panel: { profile: "citation_panel", strategy: "raw_preserve" },
  },
  unsupported: {
    chat_assistant: {
      profile: "chat_assistant",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
      capabilityHint: "此语法当前不支持",
    },
    chat_user: {
      profile: "chat_user",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
      capabilityHint: "此语法当前不支持",
    },
    editor_ingest: {
      profile: "editor_ingest",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
      capabilityHint: "此语法当前不支持",
    },
    editor_export: { profile: "editor_export", strategy: "raw_preserve" },
    vault_preview: {
      profile: "vault_preview",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
    },
    research_card: {
      profile: "research_card",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
    },
    patch_preview: {
      profile: "patch_preview",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
    },
    citation_panel: {
      profile: "citation_panel",
      strategy: "warning_block",
      placeholderClass: "iris-unsupported",
    },
  },
};
