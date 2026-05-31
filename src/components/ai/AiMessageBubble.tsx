import { useCallback, useMemo, type MouseEvent, type ReactNode } from "react";

import { MarkdownErrorBoundary } from "@/components/ui/markdown-error-boundary";
import { AiStreamPulse } from "@/components/ui/ai-message-stream-pulse";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { cn } from "@/lib/utils";

interface AiMessageBubbleProps {
  role: "user" | "assistant";
  content?: string;
  streaming?: boolean;
  children?: ReactNode;
  className?: string;
  onCitationClick?: (ref: string) => void;
}

function AssistantBody({
  content,
  streaming = false,
  onCitationClick,
}: {
  content: string;
  streaming?: boolean;
  onCitationClick?: (ref: string) => void;
}) {
  const html = useMemo(() => {
    try {
      const result = renderMarkdownWithProfile(
        content || "",
        "chat_assistant",
        {
          streaming,
        },
      );
      return result.output;
    } catch (err) {
      console.warn("[ai-message] Markdown render failed", {
        content: (content || "").slice(0, 200),
        error: String(err),
      });
      const escaped = (content || "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>");
      return `<p class="text-muted-foreground whitespace-pre-wrap">${escaped}</p>`;
    }
  }, [content, streaming]);

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
      } catch (e) {
        console.warn(
          "[ai-message] decodeURIComponent failed, using raw ref:",
          e,
        );
        onCitationClick(ref);
      }
    },
    [onCitationClick],
  );

  return (
    <div
      className="ai-message-body ai-msg iris-prose select-text text-[13px] leading-snug [&_a.ai-citation]:font-medium [&_a.ai-citation]:text-ai-citation [&_a.ai-citation]:underline [&_a.ai-citation]:decoration-ai-citation/40 [&_a.ai-citation]:underline-offset-2 hover:[&_a.ai-citation]:text-ai-citation-hover [&_code]:rounded [&_code]:bg-editor-code-bg [&_code]:px-1 [&_code]:font-mono [&_code]:text-editor-code-fg [&_p]:mb-1.5 [&_ul]:mb-1.5 [&_ul]:list-disc [&_ul]:pl-5"
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={onCitationClick ? handleClick : undefined}
    />
  );
}

/** AI 对话气泡壳（选区限制在 `.ai-message-body` 内） */
export function AiMessageBubble({
  role,
  content,
  streaming = false,
  children,
  className,
  onCitationClick,
}: AiMessageBubbleProps) {
  if (role === "user") {
    const userHtml = useMemo(() => {
      try {
        const result = renderMarkdownWithProfile(content || "", "chat_user", {
          streaming: false,
        });
        return result.output;
      } catch {
        return (content || "")
          .replace(/&/g, "&amp;")
          .replace(/</g, "&lt;")
          .replace(/>/g, "&gt;")
          .replace(/\n/g, "<br>");
      }
    }, [content]);

    return (
      <div
        className={cn(
          "ai-message-bubble ai-message-bubble-user max-w-[92%] self-end",
          className,
        )}
      >
        <div
          className="ai-message-body iris-prose select-text px-3 py-2.5 text-[13px] leading-snug text-foreground"
          dangerouslySetInnerHTML={{ __html: userHtml }}
        />
      </div>
    );
  }

  return (
    <div
      className={cn(
        "ai-message-bubble ai-message-bubble-assistant max-w-full overflow-hidden",
        streaming && "ai-message-bubble-streaming",
        className,
      )}
      data-streaming={streaming ? "" : undefined}
    >
      {content ? (
        <MarkdownErrorBoundary>
          <AssistantBody
            content={content}
            streaming={streaming}
            onCitationClick={onCitationClick}
          />
        </MarkdownErrorBoundary>
      ) : null}
      {streaming && !content ? (
        <div className="px-3 py-2.5">
          <AiStreamPulse />
        </div>
      ) : null}
      {children}
    </div>
  );
}
