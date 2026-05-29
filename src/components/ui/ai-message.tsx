import { aiMarked } from "@/lib/ai/marked-ai";
import { useCallback, useMemo, type MouseEvent, type ReactNode } from "react";

import {
  linkifyAiCitations,
  tagCitationLinksInHtml,
} from "@/lib/ai/citation-markdown";
import { sanitizeHtml } from "@/lib/sanitize";
import { cn } from "@/lib/utils";
import { MarkdownErrorBoundary } from "@/components/ui/markdown-error-boundary";

export function AiStreamPulse() {
  return (
    <span className="ai-stream-pulse" aria-hidden>
      <span />
      <span />
      <span />
    </span>
  );
}

interface AiMessageProps {
  role: "user" | "assistant" | "system";
  content?: string;
  streaming?: boolean;
  children?: ReactNode;
  className?: string;
  onCitationClick?: (ref: string) => void;
}

function AssistantMarkdown({
  content,
  onCitationClick,
}: {
  content: string;
  onCitationClick?: (ref: string) => void;
}) {
  const html = useMemo(() => {
    try {
      const linked = linkifyAiCitations(content || "");
      const raw = aiMarked.parse(linked, { async: false }) as string;
      return sanitizeHtml(tagCitationLinksInHtml(raw));
    } catch {
      // If parsing fails, render as escaped plain text
      const escaped = (content || "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>");
      return `<pre class="text-muted-foreground text-xs whitespace-pre-wrap">${escaped}</pre>`;
    }
  }, [content]);

  const handleClick = useCallback(
    (e: MouseEvent<HTMLDivElement>) => {
      if (!onCitationClick) return;
      const target = e.target as HTMLElement;
      const anchor = target.closest("a.ai-citation, a[href^='#iris-cite-']");
      if (!anchor || !(anchor instanceof HTMLAnchorElement)) return;
      const ref =
        anchor.dataset.citeRef ??
        anchor.getAttribute("href")?.replace(/^#iris-cite-/, "");
      if (!ref) return;
      e.preventDefault();
      try {
        onCitationClick(decodeURIComponent(ref));
      } catch {
        onCitationClick(ref);
      }
    },
    [onCitationClick],
  );

  return (
    <div
      className="ai-msg text-sm leading-relaxed [&_a.ai-citation]:font-medium [&_a.ai-citation]:text-ai-citation [&_a.ai-citation]:underline [&_a.ai-citation]:decoration-ai-citation/40 [&_a.ai-citation]:underline-offset-2 hover:[&_a.ai-citation]:text-ai-citation-hover [&_code]:rounded [&_code]:bg-editor-code-bg [&_code]:px-1 [&_code]:font-mono [&_code]:text-editor-code-fg [&_p]:mb-2 [&_ul]:mb-2 [&_ul]:list-disc [&_ul]:pl-5"
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={onCitationClick ? handleClick : undefined}
    />
  );
}

export function AiMessage({
  role,
  content,
  streaming = false,
  children,
  className,
  onCitationClick,
}: AiMessageProps) {
  if (role === "system") {
    return (
      <div
        className={cn(
          "ai-msg-system text-xs italic text-muted-foreground",
          className,
        )}
      >
        {content}
      </div>
    );
  }

  if (role === "user") {
    return (
      <div className={cn("ai-msg-user text-sm", className)}>{content}</div>
    );
  }

  return (
    <div className={cn("ai-msg-assistant", className)}>
      {content ? (
        <MarkdownErrorBoundary>
          <AssistantMarkdown
            content={content}
            onCitationClick={onCitationClick}
          />
        </MarkdownErrorBoundary>
      ) : null}
      {streaming && !content ? <AiStreamPulse /> : null}
      {children}
    </div>
  );
}
