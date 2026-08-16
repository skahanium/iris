//! Streaming assistant Markdown body with stable completed blocks.
//!
//! Only the raw tail node is updated while tokens stream. Completed blocks are
//! rendered once and appended before the tail when the lexer proves that they
//! are final (paragraph separation, closed fences, heading boundaries).

import {
  useLayoutEffect,
  useRef,
  type MouseEvent as ReactMouseEvent,
} from "react";

import { proseMarked } from "@/lib/markdown-render";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { toTrustedHtml } from "@/lib/sanitize";
import { splitStreamingMarkdown } from "@/lib/streaming-markdown-splitter";
import type { Token } from "marked";

export function StreamingMessageBody({
  content,
  contentIdentity,
  className,
  dataProseSurface,
  onClick,
}: {
  content: string;
  contentIdentity?: string;
  className?: string;
  dataProseSurface?: string;
  onClick?: (event: ReactMouseEvent<HTMLDivElement>) => void;
}) {
  const containerRef = useRef<HTMLDivElement | null>(null);
  const stableBlockCountRef = useRef(-1);
  const stableMarkdownRef = useRef("");
  const lastIdentityRef = useRef<string | null | undefined>(null);

  useLayoutEffect(() => {
    const container = containerRef.current;
    if (!container) return;

    if (lastIdentityRef.current !== contentIdentity) {
      lastIdentityRef.current = contentIdentity;
      stableBlockCountRef.current = -1;
      stableMarkdownRef.current = "";
      container.replaceChildren();
    }

    const split = splitStreamingMarkdown(content);
    const mustReset =
      stableBlockCountRef.current < 0 ||
      split.stableBlockCount < stableBlockCountRef.current ||
      !content.startsWith(stableMarkdownRef.current);

    if (mustReset) {
      container.replaceChildren();
      stableBlockCountRef.current = 0;
      stableMarkdownRef.current = "";
    }

    let tail = container.querySelector<HTMLDivElement>("[data-streaming-tail]");
    if (!tail) {
      tail = document.createElement("div");
      tail.className = "ai-streaming-tail";
      tail.setAttribute("data-streaming-tail", "");
      container.appendChild(tail);
    }

    if (split.stableBlockCount > stableBlockCountRef.current) {
      const tokens = proseMarked.lexer(content) as Token[];
      const newStableMarkdown = tokens
        .slice(stableBlockCountRef.current, split.stableBlockCount)
        .map((token) => token.raw)
        .join("");
      if (newStableMarkdown.trim()) {
        const html = renderMarkdownWithProfile(newStableMarkdown, "chat_assistant", {
          streaming: false,
        }).output;
        tail.insertAdjacentHTML("beforebegin", toTrustedHtml(html) as unknown as string);
      }
    }

    stableBlockCountRef.current = split.stableBlockCount;
    stableMarkdownRef.current = split.stableMarkdown;
    tail.textContent = split.tailMarkdown;
  }, [content, contentIdentity]);

  return (
    <div
      ref={containerRef}
      className={className}
      data-prose-surface={dataProseSurface}
      data-ai-streaming-markdown
      onClick={onClick}
    />
  );
}
