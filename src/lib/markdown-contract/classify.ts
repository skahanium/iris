/**
 * Markdown capability classification.
 *
 * This module is intentionally free of editor/render imports so both the
 * contract pipeline and the editor ingest pipeline can share one classifier
 * without creating a circular dependency.
 */
import type { Token, Tokens } from "marked";

import { createMarkedInstance } from "@/lib/markdown";
import type {
  ClassifyOptions,
  MarkdownCapabilityLevel,
  MarkdownSyntaxFragment,
  MarkdownSyntaxKind,
} from "./types";
import {
  NATIVE_SYNTAX_KINDS,
  PRESERVE_ONLY_SYNTAX_KINDS,
  RENDER_ONLY_SYNTAX_KINDS,
} from "./types";
import { reconcileFragmentsWithSource } from "./fragment-reconcile";
import { isDangerousHtml } from "./html-safety";

const contractMarked = createMarkedInstance({ gfm: true, breaks: true });

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

/** Internal accumulator for fragment building */
interface FragmentAccumulator {
  fragments: MarkdownSyntaxFragment[];
  offset: number;
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
        // Emit the whole list as one fragment so mixed normal/task items stay
        // in the same list instead of being split into separate lists.
        const allTasks = listToken.items.every((item) => item.task);
        pushFragment(acc, raw, allTasks ? "task_list" : "list");
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

/** Classify Markdown source into capability fragments. */
export function classifyMarkdownCapabilities(
  source: string,
  _options?: ClassifyOptions,
): MarkdownSyntaxFragment[] {
  return buildFragments(source);
}
