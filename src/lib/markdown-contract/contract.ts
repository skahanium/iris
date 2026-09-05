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
import {
  renderAiMarkdownToHtml,
  repairStreamingMarkdown,
} from "@/lib/markdown-render";
import {
  markdownToHtmlPage,
  createMarkedInstance,
  repairTightStrongPunctuationBoundaries,
} from "@/lib/markdown";
import { sanitizeHtml } from "@/lib/sanitize";
import { contentHash64 as markdownContentHash } from "@/lib/content-hash";
import { ingestMarkdownForEditorSafely } from "@/lib/editor-ingest";
import { markdownToMarkdownViaProductionEditor } from "@/lib/editor-roundtrip";
import { classifyMarkdownCapabilities } from "./classify";
export { classifyMarkdownCapabilities };

import type {
  IngestedMarkdown,
  IngestOptions,
  MarkdownCapabilityWarning,
  MarkdownContractResult,
  MarkdownFragmentStats,
  MarkdownProfile,
  MarkdownSyntaxFragment,
  RenderOptions,
  StreamRepairRecord,
} from "./types";
import { DEFAULT_PROFILE_RULES } from "./types";

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

const RENDER_CACHE_FORMAT_VERSION = "render-cache-v2";
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
    RENDER_CACHE_FORMAT_VERSION,
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
// Phase 2.1: Source Ingest
// ═══════════════════════════════════════════════════════════════════

export function ingestMarkdown(
  source: string,
  options?: IngestOptions,
): IngestedMarkdown {
  const profile: MarkdownProfile = options?.profile ?? "chat_assistant";
  const streaming = options?.streaming ?? false;
  const context = options?.context;

  const fragments = classifyMarkdownCapabilities(source);

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
// Phase 2.2: Normalize / Classify (implementation lives in ./classify)
// ═══════════════════════════════════════════════════════════════════

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
      // Apply bold boundary repair so **text：** and __text：__ render correctly.
      return sanitizeHtml(
        contractMarked.parse(repairTightStrongPunctuationBoundaries(md), {
          async: false,
        }) as string,
      );
    case "editor_ingest":
      return ingestMarkdownForEditorSafely({ bodyMarkdown: md }).tipTapHtml;
    case "editor_export":
      return markdownToMarkdownViaProductionEditor(md);
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

  const fragments = classifyMarkdownCapabilities(source);
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
