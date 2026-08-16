//! Splits streaming Markdown into stable, already-final blocks and a raw
//! streaming tail. The tail is updated in place; stable blocks are rendered
//! once and never recreated while their token count remains unchanged.

import type { Token } from "marked";

import { proseMarked } from "@/lib/markdown-render";

export interface StreamingMarkdownSplit {
  /** Raw Markdown of all blocks that will not change anymore. */
  stableMarkdown: string;
  /** Raw Markdown that is still being typed and must remain in the tail. */
  tailMarkdown: string;
  /** Number of lexer tokens included in `stableMarkdown`. */
  stableBlockCount: number;
}

function hasClosedCodeFence(token: Token): boolean {
  if (token.type !== "code") return false;
  const fenceLines = token.raw.match(/^[ \t]*```/gm) ?? [];
  return fenceLines.length >= 2;
}

function isStableToken(token: Token, isLast: boolean): boolean {
  if (!isLast) return true;
  if (token.type === "space") return true;
  if (token.type === "heading" || token.type === "hr") return true;
  if (token.type === "code") return hasClosedCodeFence(token);
  return false;
}

export function splitStreamingMarkdown(content: string): StreamingMarkdownSplit {
  if (!content) {
    return {
      stableMarkdown: "",
      tailMarkdown: "",
      stableBlockCount: 0,
    };
  }

  const tokens = proseMarked.lexer(content) as Token[];
  let stableBlockCount = 0;
  while (
    stableBlockCount < tokens.length &&
    isStableToken(tokens[stableBlockCount]!, stableBlockCount === tokens.length - 1)
  ) {
    stableBlockCount += 1;
  }

  const stableMarkdown = tokens
    .slice(0, stableBlockCount)
    .map((token) => token.raw)
    .join("");
  return {
    stableMarkdown,
    tailMarkdown: content.slice(stableMarkdown.length),
    stableBlockCount,
  };
}
