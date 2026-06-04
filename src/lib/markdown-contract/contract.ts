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
import { marked } from "marked";
import type { Token, Tokens } from "marked";

import {
  renderAiMarkdownToHtml,
  repairStreamingMarkdown,
} from "@/lib/markdown-render";
import {
  markdownBodyToEditorHtml,
  editorBodyHtmlToMarkdown,
  markdownToHtmlPage,
} from "@/lib/markdown";
import { sanitizeHtml } from "@/lib/sanitize";

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
  NATIVE_SYNTAX_KINDS,
  RENDER_ONLY_SYNTAX_KINDS,
  PRESERVE_ONLY_SYNTAX_KINDS,
} from "./types";

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
  return />\s*\[![a-zA-Z]+\]/.test(raw);
}

/** Determine if an HTML token is a comment */
function isHtmlComment(raw: string): boolean {
  return /^\s*<!--/.test(raw);
}

/** Determine if an HTML token is dangerous (script, object, etc.) */
function isDangerousHtml(raw: string): boolean {
  return /<\s*(script|object|embed|iframe|form|applet)\b/i.test(raw);
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
): void {
  acc.fragments.push({
    raw,
    syntaxKind,
    offset: acc.offset,
    endOffset: acc.offset + raw.length,
    capability: determineCapability(syntaxKind, raw),
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

    /** Block separator in source (`\n\n` between blocks); ingest maps to empty spacer paragraphs. */
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
        pushFragment(acc, raw, "html_comment");
      } else {
        pushFragment(acc, raw, "raw_html");
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

  const tokens = marked.lexer(source);
  const acc: FragmentAccumulator = { fragments: [], offset: 0 };

  walkTokens(tokens, acc);

  // Fill gaps: marked may consume [^label]: definitions as link references,
  // removing them from the token tree. Scan gaps for leftover footnote defs.
  fillFragmentGaps(source, acc);

  // Handle trailing gap (source after last fragment)
  const sorted = [...acc.fragments].sort((a, b) => a.offset - b.offset);
  if (sorted.length > 0) {
    const lastFrag = sorted[sorted.length - 1]!;
    if (lastFrag.endOffset < source.length) {
      const trailing = source.slice(lastFrag.endOffset);
      scanTrailingGapForFootnoteDefs(trailing, lastFrag.endOffset, source, acc);
    }
  }

  // Re-sort final fragments
  acc.fragments.sort((a, b) => a.offset - b.offset);

  // Update offset to match source end
  acc.offset = source.length;

  return acc.fragments;
}

/**
 * Scan trailing source text for footnote definitions that were consumed
 * by marked as link reference definitions.
 */
function scanTrailingGapForFootnoteDefs(
  gapText: string,
  gapOffset: number,
  _fullSource: string,
  acc: FragmentAccumulator,
): void {
  // Footnote definition pattern: [^label]: content
  const defRegex = /\[\^[^\]]+\]:\s*.*(?:\n(?![ \t]*\n|\[\^[^\]]+\]:))*/g;
  let match: RegExpExecArray | null;

  while ((match = defRegex.exec(gapText)) !== null) {
    const defRaw = match[0];
    const absOffset = gapOffset + match.index;

    // Add whitespace before the definition if present
    const before = gapText.slice(0, match.index);
    if (before) {
      acc.fragments.push({
        raw: before,
        syntaxKind: "space",
        offset: gapOffset,
        endOffset: absOffset,
        capability: "native",
      });
    }

    acc.fragments.push({
      raw: defRaw,
      syntaxKind: "footnote_def",
      offset: absOffset,
      endOffset: absOffset + defRaw.length,
      capability: "render_only",
    });
  }

  // Add any remaining text after the last match
  if (match === null && gapText.trim()) {
    acc.fragments.push({
      raw: gapText,
      syntaxKind: "text",
      offset: gapOffset,
      endOffset: gapOffset + gapText.length,
      capability: "native",
    });
  }
}

/**
 * Scan the source for gaps between fragments and fill them.
 * This handles footnote definitions [^label]: content that were consumed
 * by marked as link reference definitions.
 */
function fillFragmentGaps(source: string, acc: FragmentAccumulator): void {
  // Sort fragments by offset (they should already be sorted)
  const sorted = [...acc.fragments].sort((a, b) => a.offset - b.offset);

  for (let i = 0; i < sorted.length - 1; i++) {
    const current = sorted[i]!;
    const next = sorted[i + 1]!;
    const gapStart = current.endOffset;
    const gapEnd = next.offset;

    if (gapStart < gapEnd) {
      const gapText = source.slice(gapStart, gapEnd);

      // Check if the gap only contains whitespace/newlines
      if (/^\s+$/.test(gapText)) {
        // Insert a space fragment
        const gapFrag: MarkdownSyntaxFragment = {
          raw: gapText,
          syntaxKind: "space",
          offset: gapStart,
          endOffset: gapEnd,
          capability: "native",
        };
        // Insert into sorted position by rebuilding the array
        const insertIdx = acc.fragments.findIndex((f) => f.offset > gapStart);
        if (insertIdx === -1) {
          acc.fragments.push(gapFrag);
        } else {
          acc.fragments.splice(insertIdx, 0, gapFrag);
        }
      } else {
        // Non-whitespace gap — likely a consumed footnote definition
        // Check for [^label]: pattern
        const match = /^\s*\[\^[^\]]+\]:\s*.*$/m.exec(gapText);
        if (match) {
          const defRaw = match[0];
          const defFrag: MarkdownSyntaxFragment = {
            raw: defRaw,
            syntaxKind: "footnote_def",
            offset: gapStart + gapText.indexOf(defRaw),
            endOffset: gapStart + gapText.indexOf(defRaw) + defRaw.length,
            capability: "render_only",
          };
          const insertIdx = acc.fragments.findIndex(
            (f) => f.offset > defFrag.offset,
          );
          if (insertIdx === -1) {
            acc.fragments.push(defFrag);
          } else {
            acc.fragments.splice(insertIdx, 0, defFrag);
          }
          // Also handle any surrounding whitespace
          const beforeDef = gapText.slice(0, gapText.indexOf(defRaw));
          const afterDef = gapText.slice(
            gapText.indexOf(defRaw) + defRaw.length,
          );
          if (beforeDef.trim()) {
            const beforeFrag: MarkdownSyntaxFragment = {
              raw: beforeDef,
              syntaxKind: "text",
              offset: gapStart,
              endOffset: gapStart + beforeDef.length,
              capability: "native",
            };
            const idx = acc.fragments.findIndex(
              (f) => f.offset > beforeFrag.offset,
            );
            if (idx === -1) acc.fragments.push(beforeFrag);
            else acc.fragments.splice(idx, 0, beforeFrag);
          }
          if (afterDef.trim()) {
            const afterFrag: MarkdownSyntaxFragment = {
              raw: afterDef,
              syntaxKind: "space",
              offset: defFrag.endOffset,
              endOffset: defFrag.endOffset + afterDef.length,
              capability: "native",
            };
            const idx = acc.fragments.findIndex(
              (f) => f.offset > afterFrag.offset,
            );
            if (idx === -1) acc.fragments.push(afterFrag);
            else acc.fragments.splice(idx, 0, afterFrag);
          }
        }
      }
    }
  }

  // Update offset
  acc.offset = source.length;
  // Re-sort
  acc.fragments.sort((a, b) => a.offset - b.offset);
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

  const parts: string[] = [];
  for (const frag of preserveFragments) {
    parts.push(frag.raw);
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
): MarkdownCapabilityWarning[] {
  const warnings: MarkdownCapabilityWarning[] = [];
  for (const f of fragments) {
    if (f.capability === "unsupported") {
      warnings.push({
        fragment: f,
        message: `Unsupported syntax: ${f.syntaxKind}`,
        severity: "warn",
      });
    }
  }
  return warnings;
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
      before: source,
      after: repaired,
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
      return sanitizeHtml(renderAiMarkdownToHtml(md, { streaming: false }));
    case "chat_user":
      // User messages: render Markdown with sanitization, no citation linkification
      return sanitizeHtml(marked.parse(md, { async: false }) as string);
    case "editor_ingest":
      return markdownBodyToEditorHtml(md);
    case "editor_export":
      return editorBodyHtmlToMarkdown(markdownBodyToEditorHtml(md));
    case "vault_preview":
      return markdownToHtmlPage(md, options?.context);
    case "research_card":
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
  const fragments = buildFragments(source);
  const output = renderByProfile(source, profile, streaming, options);
  const warnings = buildWarnings(fragments);
  const streamRepairs = buildStreamRepairs(source, streaming);
  const preserveFragments = fragments.filter(
    (f) => f.capability === "preserve_only" || f.capability === "unsupported",
  );
  const stats = computeStats(fragments);

  return {
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
}
