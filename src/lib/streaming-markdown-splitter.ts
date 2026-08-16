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

function isHardCompleteToken(token: Token): boolean {
  return (
    token.type === "heading" ||
    token.type === "hr" ||
    (token.type === "code" && hasClosedCodeFence(token))
  );
}

function isWhitespaceToken(token: Token): boolean {
  return token.type === "space";
}

export function splitStreamingMarkdown(
  content: string,
): StreamingMarkdownSplit {
  if (!content) {
    return {
      stableMarkdown: "",
      tailMarkdown: "",
      stableBlockCount: 0,
    };
  }

  const tokens = proseMarked.lexer(content) as Token[];
  let lastContentIndex = -1;
  for (let index = tokens.length - 1; index >= 0; index -= 1) {
    const token = tokens[index];
    if (token && !isWhitespaceToken(token)) {
      lastContentIndex = index;
      break;
    }
  }
  if (lastContentIndex < 0) {
    return {
      stableMarkdown: "",
      tailMarkdown: content,
      stableBlockCount: 0,
    };
  }

  const hasTrailingBoundary = tokens
    .slice(lastContentIndex + 1)
    .some(isWhitespaceToken);
  const finalContentIsStable =
    lastContentIndex < tokens.length - 1
      ? hasTrailingBoundary
      : isHardCompleteToken(tokens[lastContentIndex]!);
  const stableTokenEnd = finalContentIsStable
    ? tokens.length
    : lastContentIndex;
  const stableTokens = tokens.slice(0, stableTokenEnd);
  const stableMarkdown = stableTokens.map((token) => token.raw).join("");

  return {
    stableMarkdown,
    tailMarkdown: content.slice(stableMarkdown.length),
    stableBlockCount: stableTokens.filter((token) => !isWhitespaceToken(token))
      .length,
  };
}
