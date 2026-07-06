/**
 * Markdown 契约内核 — 核心流水线实现（子项目 1 阶段 2）
 *
 * 实现四段流水线：
 * 1. ingestMarkdown           — 源摄取
 * 2. classifyMarkdownCapabilities — 能力分级
 * 3. serializePreservedMarkdown    — 原文回吐
 * 4. renderMarkdownWithProfile     — 按 profile 渲染
 *
 * @module markdown-contract/contract
 */
import type { Token, Tokens } from "marked";

import {
  renderAiMarkdownToHtml,
  repairStreamingMarkdown,
} from "@/lib/markdown-render";
import {
  markdownBodyToEditorHtml,
  editorBodyHtmlToMarkdown,
  markdownToHtmlPage,
  createMarkedInstance,
} from "@/lib/markdown";
import { sanitizeHtml } from "@/lib/sanitize";
import { assistantContentHash as markdownContentHash } from "@/lib/assistant-stream-buffer";

import type {
  ClassifyOptions,
  IngestedMarkdown,
  IngestOptions,
  MarkdownCapabilityLevel,
  MarkdownCapabilityWarning,
  MarkdownContractResult,
  MarkdownFragmentStats,
  MarkdownProfile,
  MarkdownSyntaxFragment,
  MarkdownSyntaxKind,
  RenderOptions,
  StreamRepairRecord,
} from "./types";
import {
  DEFAULT_PROFILE_RULES,
  NATIVE_SYNTAX_KINDS,
  RENDER_ONLY_SYNTAX_KINDS,
  PRESERVE_ONLY_SYNTAX_KINDS,
} from "./types";
import { reconcileFragmentsWithSource } from "./fragment-reconcile";
import { isDangerousHtml } from "./html-safety";

const contractMarked = createMarkedInstance({ gfm: true, breaks: true });

// ═══════════════════════════════════════════════════════════════════
// Render Result Cache (cross-mount LRU)
// ═══════════════════════════════════════════════════════════════════
//
// Virtualized message rows unmount when scrolled out of view and remount on
// return. Without a cache, each remount re-parses markdown from scratch
// (2× marked parse + per-code-block lowlight highlighting + DOMPurify),
// causing the "blank then re-load" flicker on scroll. This module-level LRU
// cache keyed on (source, profile, streaming) survives row unmount/remount,
// so re-entering a measured row returns the pre-parsed HTML in O(1).
//
// Streaming results are NOT cached: mid-stream content is incomplete and will
// grow, so caching would return stale snapshots. Only finalized (streaming=
// false or omitted) renders are cached.

const RENDER_CACHE_MAX = 64;
const RENDER_CACHE_ENTRY_BYTES_MAX = 240_000;
const RENDER_CACHE_TOTAL_BYTES_MAX = 1_500_000;

interface RenderCacheEntry {
  estimatedBytes: number;
  result: MarkdownContractResult;
}

const renderCache = new Map<string, RenderCacheEntry>();
let renderCacheEstimatedBytes = 0;

/** Build a cache key from the render inputs without retaining raw source text. */
function renderCacheKey(
  source: string,
  profile: MarkdownProfile,
  streaming: boolean,
  context?: string,
): string {
  const contextHash = context ? markdownContentHash(context) : "no-context";
  return [
    profile,
    streaming ? "1" : "0",
    contextHash,
    source.length,
    markdownContentHash(source),
  ].join("\u0000");
}

/** Clear the render cache (for tests). */
export function clearMarkdownRenderCache(): void {
  renderCache.clear();
  renderCacheEstimatedBytes = 0;
}

export function getMarkdownRenderCacheStats(): {
  entryCount: number;
  estimatedBytes: number;
} {
  return {
    entryCount: renderCache.size,
    estimatedBytes: renderCacheEstimatedBytes,
  };
}

/**
 * Look up a cached render result. Moves the entry to the end of the Map
 * (most-recently-used) to implement LRU eviction.
 */
function getCachedResult(key: string): MarkdownContractResult | undefined {
  const cached = renderCache.get(key);
  if (cached === undefined) return undefined;
  // Map preserves insertion order; delete + re-insert = move to end (MRU).
  renderCache.delete(key);
  renderCache.set(key, cached);
  return cached.result;
}

function estimatedStringBytes(value: string): number {
  return value.length * 2;
}

function estimatedRenderResultBytes(result: MarkdownContractResult): number {
  let total = estimatedStringBytes(result.output);
  for (const fragment of result.preserveFragments) {
    total += estimatedStringBytes(fragment.raw);
  }
  for (const warning of result.warnings) {
    total += estimatedStringBytes(warning.message);
  }
  for (const repair of result.streamRepairs) {
    total +=
      estimatedStringBytes(repair.before) + estimatedStringBytes(repair.after);
  }
  return total;
}

function evictOldestCacheEntry(): boolean {
  const oldestKey = renderCache.keys().next().value;
  if (oldestKey === undefined) return false;
  const oldest = renderCache.get(oldestKey);
  if (oldest) {
    renderCacheEstimatedBytes = Math.max(
      0,
      renderCacheEstimatedBytes - oldest.estimatedBytes,
    );
  }
  renderCache.delete(oldestKey);
  return true;
}

/** Store a render result within the LRU byte budget. */
function setCachedResult(key: string, result: MarkdownContractResult): void {
  const estimatedBytes = estimatedRenderResultBytes(result);
  if (estimatedBytes > RENDER_CACHE_ENTRY_BYTES_MAX) return;

  const existing = renderCache.get(key);
  if (existing) {
    renderCacheEstimatedBytes = Math.max(
      0,
      renderCacheEstimatedBytes - existing.estimatedBytes,
    );
    renderCache.delete(key);
  }

  while (renderCache.size >= RENDER_CACHE_MAX) {
    if (!evictOldestCacheEntry()) break;
  }
  while (
    renderCacheEstimatedBytes + estimatedBytes >
    RENDER_CACHE_TOTAL_BYTES_MAX
  ) {
    if (!evictOldestCacheEntry()) break;
  }

  if (
    renderCacheEstimatedBytes + estimatedBytes >
    RENDER_CACHE_TOTAL_BYTES_MAX
  ) {
    return;
  }

  renderCache.set(key, { estimatedBytes, result });
  renderCacheEstimatedBytes += estimatedBytes;
}

// ═══════════════════════════════════════════════════════════════════
// Token Walker & Fragment Builder
// ═══════════════════════════════════════════════════════════════════

/** Internal accumulator for fragment building */
interface FragmentAccumulator {
  fragments: MarkdownSyntaxFragment[];
  offset: number;
}

/** Map marked token type → syntaxKind */
function syntaxKindFromToken(token: Token): MarkdownSyntaxKind | null {
  const t = token.type;
  if (t === "heading") return "heading";
  if (t === "paragraph") return "paragraph";
  if (t === "text") return "text";
  if (t === "space") return "space";
  if (t === "strong") return "bold";
  if (t === "em") return "italic";
  if (t === "del") return "strikethrough";
  if (t === "codespan") return "inline_code";
  if (t === "code") return "code_block";
  if (t === "list") return null; // list container, process items separately
  if (t === "list_item") return null; // list item, process child tokens
  if (t === "table") return "table";
  if (t === "blockquote") return null; // detect callout below
  if (t === "link") return "link";
  if (t === "image") return "image";
  if (t === "hr") return "horizontal_rule";
  if (t === "html") return null; // detect comment vs raw below
  if (t === "br") return "text";
  return null;
}

/** Determine if a blockquote is a callout */
function isCalloutBlockquote(raw: string): boolean {
  return />\s*\[![a-zA-Z][a-zA-Z0-9-]*\]/.test(raw);
}

/** Determine if an HTML token is a comment */
function isHtmlComment(raw: string): boolean {
  return /^\s*<!--/.test(raw);
}

/**
 * Split a text token's raw into sub-fragments for wiki-links and footnotes.
 * Returns an array of { raw, syntaxKind, offset } relative to the token start.
 */
function splitTextToken(
  raw: string,
  tokenOffset: number,
): Array<{ raw: string; syntaxKind: MarkdownSyntaxKind; offset: number }> {
  const result: Array<{
    raw: string;
    syntaxKind: MarkdownSyntaxKind;
    offset: number;
  }> = [];

  // Combined regex: wiki-links [[title]] | footnote refs [^label] | footnote defs [^label]:
  const regex =
    /\[\[([^\]\n]+)\]\]|(?<!\[)\[\^([^\]]+)\](?!:)|(?<=\n|^)\[\^([^\]]+)\]:/g;

  let lastIndex = 0;
  let match: RegExpExecArray | null;

  while ((match = regex.exec(raw)) !== null) {
    const before = raw.slice(lastIndex, match.index);
    if (before) {
      result.push({
        raw: before,
        syntaxKind: "text",
        offset: tokenOffset + lastIndex,
      });
    }

    const fullMatch = match[0];
    if (match[1] !== undefined) {
      // [[WikiLink]]
      result.push({
        raw: fullMatch,
        syntaxKind: "wiki_link",
        offset: tokenOffset + match.index,
      });
    } else if (match[2] !== undefined) {
      // [^label] (inline ref)
      result.push({
        raw: fullMatch,
        syntaxKind: "footnote_ref",
        offset: tokenOffset + match.index,
      });
    } else if (match[3] !== undefined) {
      // [^label]: (definition, includes colon)
      result.push({
        raw: fullMatch,
        syntaxKind: "footnote_def",
        offset: tokenOffset + match.index,
      });
    }

    lastIndex = match.index + fullMatch.length;
  }

  const remainder = raw.slice(lastIndex);
  if (remainder) {
    result.push({
      raw: remainder,
      syntaxKind: "text",
      offset: tokenOffset + lastIndex,
    });
  }

  return result;
}

/** Push a fragment to the accumulator */
function pushFragment(
  acc: FragmentAccumulator,
  raw: string,
  syntaxKind: MarkdownSyntaxKind,
  options: { inline?: boolean } = {},
): void {
  acc.fragments.push({
    raw,
    syntaxKind,
    offset: acc.offset,
    endOffset: acc.offset + raw.length,
    capability: determineCapability(syntaxKind, raw),
    inline: options.inline,
  });
  acc.offset += raw.length;
}

/** Determine capability level from syntaxKind */
function determineCapability(
  syntaxKind: MarkdownSyntaxKind,
  raw?: string,
): MarkdownCapabilityLevel {
  if (NATIVE_SYNTAX_KINDS.has(syntaxKind)) return "native";
  if (RENDER_ONLY_SYNTAX_KINDS.has(syntaxKind)) return "render_only";
  if (PRESERVE_ONLY_SYNTAX_KINDS.has(syntaxKind)) {
    if (raw && isDangerousHtml(raw)) return "unsupported";
    return "preserve_only";
  }
  return "unsupported";
}

// Forward declarations
function walkTokens(tokens: Token[], acc: FragmentAccumulator): void;

/**
 * Walk a list of block-level tokens and emit fragments.
 */
function walkTokens(tokens: Token[], acc: FragmentAccumulator): void {
  for (const token of tokens) {
    const raw = token.raw ?? "";
    const type = token.type;

    /** Block separator in source (`\n\n` between blocks); editor ingest ignores it as editable content. */
    if (type === "space") {
      pushFragment(acc, raw, "space");
      continue;
    }

    if (type === "hr") {
      pushFragment(acc, raw, "horizontal_rule");
      continue;
    }

    if (type === "heading") {
      pushFragment(acc, raw, "heading");
      continue;
    }

    if (type === "paragraph") {
      // Check if this paragraph is a footnote definition
      if (/^\s*\[\^[^\]]+\]:/.test(raw)) {
        pushFragment(acc, raw, "footnote_def");
      } else {
        // Walk inline tokens for the paragraph
        const paraToken = token as Tokens.Paragraph;
        if (paraToken.tokens && paraToken.tokens.length > 0) {
          walkInlineTokensBlock(raw, paraToken.tokens, acc);
        } else {
          pushFragment(acc, raw, "paragraph");
        }
      }
      continue;
    }

    if (type === "code") {
      pushFragment(acc, raw, "code_block");
      continue;
    }

    if (type === "table") {
      pushFragment(acc, raw, "table");
      continue;
    }

    if (type === "html") {
      if (isHtmlComment(raw)) {
        pushFragment(acc, raw, "html_comment");
      } else {
        pushFragment(acc, raw, "raw_html");
      }
      continue;
    }

    if (type === "blockquote") {
      if (isCalloutBlockquote(raw)) {
        pushFragment(acc, raw, "callout");
      } else {
        pushFragment(acc, raw, "blockquote");
      }
      continue;
    }

    if (type === "list") {
      const listToken = token as Tokens.List;
      if (listToken.items) {
        for (const item of listToken.items) {
          if (item.task) {
            // Task list: emit the item's raw (includes checkbox)
            pushFragment(acc, item.raw ?? "", "task_list");
          } else {
            // Regular list: emit the item
            pushFragment(acc, item.raw ?? "", "list");
          }
        }
      } else {
        pushFragment(acc, raw, "list");
      }
      continue;
    }

    // Fallback for any unhandled token types
    const kind = syntaxKindFromToken(token);
    if (kind) {
      pushFragment(acc, raw, kind);
    } else {
      pushFragment(acc, raw, "unknown");
    }
  }
}

/**
 * Walk inline tokens inside a block (paragraph, heading, etc.)
 * Handles text splitting for wiki-links and footnotes,
 * and detects footnote references disguised as link tokens.
 */
function walkInlineTokensBlock(
  _blockRaw: string,
  inlineTokens: Token[],
  acc: FragmentAccumulator,
): void {
  for (const token of inlineTokens) {
    const raw = token.raw ?? "";
    const type = token.type;

    if (type === "text") {
      // Split text for wiki-links and footnotes
      const subs = splitTextToken(raw, acc.offset);
      for (const sub of subs) {
        acc.fragments.push({
          raw: sub.raw,
          syntaxKind: sub.syntaxKind,
          offset: sub.offset,
          endOffset: sub.offset + sub.raw.length,
          capability: determineCapability(sub.syntaxKind),
        });
      }
      acc.offset += raw.length;
      continue;
    }

    if (type === "link") {
      // marked may parse [^1] as a link token (with the definition as href)
      // Detect footnote references: raw starts with [^ and ends with ]
      if (/^\[\^[^\]]+\]$/.test(raw)) {
        pushFragment(acc, raw, "footnote_ref");
      } else {
        pushFragment(acc, raw, "link");
      }
      continue;
    }

    if (type === "strong") {
      pushFragment(acc, raw, "bold");
      continue;
    }

    if (type === "em") {
      pushFragment(acc, raw, "italic");
      continue;
    }

    if (type === "del") {
      pushFragment(acc, raw, "strikethrough");
      continue;
    }

    if (type === "codespan") {
      pushFragment(acc, raw, "inline_code");
      continue;
    }

    if (type === "image") {
      pushFragment(acc, raw, "image");
      continue;
    }

    if (type === "html") {
      if (isHtmlComment(raw)) {
        pushFragment(acc, raw, "html_comment", { inline: true });
      } else {
        pushFragment(acc, raw, "raw_html", { inline: true });
      }
      continue;
    }

    if (type === "br") {
      pushFragment(acc, raw, "text");
      continue;
    }

    // Fallback
    const kind = syntaxKindFromToken(token);
    pushFragment(acc, raw, kind ?? "unknown");
  }
}

/** Build fragments from raw markdown source using marked lexer */
function buildFragments(source: string): MarkdownSyntaxFragment[] {
  if (!source) return [];

  const tokens = contractMarked.lexer(source);
  const acc: FragmentAccumulator = { fragments: [], offset: 0 };

  walkTokens(tokens, acc);

  acc.fragments = reconcileFragmentsWithSource(source, acc.fragments);
  acc.offset = source.length;

  return acc.fragments;
}

// ═══════════════════════════════════════════════════════════════════
// Phase 2.1: Source Ingest
// ═══════════════════════════════════════════════════════════════════

export function ingestMarkdown(
  source: string,
  options?: IngestOptions,
): IngestedMarkdown {
  const profile: MarkdownProfile = options?.profile ?? "chat_assistant";
  const streaming = options?.streaming ?? false;
  const context = options?.context;

  const fragments = buildFragments(source);

  return {
    raw: source,
    source: {
      profile,
      streaming,
      context,
    },
    fragments,
  };
}

// ═══════════════════════════════════════════════════════════════════
// Phase 2.2: Normalize / Classify
// ═══════════════════════════════════════════════════════════════════

export function classifyMarkdownCapabilities(
  source: string,
  _options?: ClassifyOptions,
): MarkdownSyntaxFragment[] {
  return buildFragments(source);
}

// ═══════════════════════════════════════════════════════════════════
// Phase 2.3: Preservation / Fallback
// ═══════════════════════════════════════════════════════════════════

export function serializePreservedMarkdown(
  source: string,
  preserveFragments: MarkdownSyntaxFragment[],
): string {
  if (!source) return "";
  if (preserveFragments.length === 0) return source;

  const sorted = [...preserveFragments].sort((a, b) => a.offset - b.offset);

  const parts: string[] = [];
  let cursor = 0;
  for (const frag of sorted) {
    if (frag.offset > cursor) {
      parts.push(source.slice(cursor, frag.offset));
    }
    parts.push(frag.raw);
    cursor = frag.endOffset;
  }
  if (cursor < source.length) {
    parts.push(source.slice(cursor));
  }
  return parts.join("");
}

// ═══════════════════════════════════════════════════════════════════
// Phase 2.4: Render Profiles
// ═══════════════════════════════════════════════════════════════════

function computeStats(
  fragments: MarkdownSyntaxFragment[],
): MarkdownFragmentStats {
  const stats: MarkdownFragmentStats = {
    native: 0,
    render_only: 0,
    preserve_only: 0,
    unsupported: 0,
    total: fragments.length,
  };
  for (const f of fragments) {
    switch (f.capability) {
      case "native":
        stats.native++;
        break;
      case "render_only":
        stats.render_only++;
        break;
      case "preserve_only":
        stats.preserve_only++;
        break;
      case "unsupported":
        stats.unsupported++;
        break;
    }
  }
  return stats;
}

function buildWarnings(
  fragments: MarkdownSyntaxFragment[],
  profile: MarkdownProfile,
): MarkdownCapabilityWarning[] {
  const warnings: MarkdownCapabilityWarning[] = [];
  for (const f of fragments) {
    if (f.capability === "unsupported") {
      const rule = DEFAULT_PROFILE_RULES[f.capability][profile];
      warnings.push({
        fragment: f,
        message:
          rule.capabilityHint ??
          `Unsupported syntax: ${f.syntaxKind} (${rule.strategy})`,
        severity: "warn",
      });
    }
  }
  return warnings;
}

function summarizeRepairText(value: string): string {
  if (value.length <= 20_000) return value;
  return `[omitted:${value.length}:${markdownContentHash(value)}]`;
}

function buildStreamRepairs(
  source: string,
  streaming: boolean,
): StreamRepairRecord[] {
  if (!streaming) return [];

  const repaired = repairStreamingMarkdown(source);
  if (repaired === source) return [];

  return [
    {
      before: summarizeRepairText(source),
      after: summarizeRepairText(repaired),
      repairKind: "streaming_repaired",
      offset: source.length,
    },
  ];
}

function renderByProfile(
  source: string,
  profile: MarkdownProfile,
  streaming: boolean,
  options?: RenderOptions,
): string {
  const md = streaming ? repairStreamingMarkdown(source) : source;

  switch (profile) {
    case "chat_assistant":
      return sanitizeHtml(
        renderAiMarkdownToHtml(md, { streaming: false, codeCopy: false }),
      );
    case "chat_user":
      // User messages: render Markdown with sanitization, no citation linkification
      return sanitizeHtml(contractMarked.parse(md, { async: false }) as string);
    case "editor_ingest":
      return markdownBodyToEditorHtml(md);
    case "editor_export":
      return editorBodyHtmlToMarkdown(markdownBodyToEditorHtml(md));
    case "vault_preview":
      return markdownToHtmlPage(md, options?.context);
    case "artifact_readonly":
    case "patch_preview":
    case "citation_panel":
      return sanitizeHtml(renderAiMarkdownToHtml(md, { streaming: false }));
    default:
      return sanitizeHtml(renderAiMarkdownToHtml(md, { streaming: false }));
  }
}

export function renderMarkdownWithProfile(
  source: string,
  profile: MarkdownProfile,
  options?: RenderOptions,
): MarkdownContractResult {
  const streaming = options?.streaming ?? false;

  // Only cache finalized (non-streaming) renders. Streaming content is
  // mid-flight and will grow; caching would return stale snapshots.
  if (!streaming) {
    const key = renderCacheKey(source, profile, false, options?.context);
    const cached = getCachedResult(key);
    if (cached !== undefined) return cached;
  }

  const fragments = buildFragments(source);
  const output = renderByProfile(source, profile, streaming, options);
  const warnings = buildWarnings(fragments, profile);
  const streamRepairs = buildStreamRepairs(source, streaming);
  const preserveFragments = fragments.filter(
    (f) => f.capability === "preserve_only" || f.capability === "unsupported",
  );
  const stats = computeStats(fragments);

  const result: MarkdownContractResult = {
    output,
    preserveFragments,
    warnings,
    streamRepairs,
    meta: {
      profile,
      streaming,
      stats,
      renderedAt: Date.now(),
    },
  };

  // Cache finalized renders for cross-mount reuse.
  if (!streaming) {
    const key = renderCacheKey(source, profile, false, options?.context);
    setCachedResult(key, result);
  }

  return result;
}
