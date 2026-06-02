import {
  useCallback,
  useMemo,
  memo,
  type MouseEvent,
  type ReactNode,
} from "react";
import { Check, Copy, RotateCcw } from "lucide-react";

import { MarkdownErrorBoundary } from "@/components/ui/markdown-error-boundary";
import { AiStreamPulse } from "@/components/ui/ai-message-stream-pulse";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { cn } from "@/lib/utils";
import { useStreamingContent } from "@/hooks/useStreamingContent";

interface AiMessageBubbleProps {
  role: "user" | "assistant";
  content?: string;
  streaming?: boolean;
  selected?: boolean;
  createdAt?: string;
  children?: ReactNode;
  className?: string;
  onCitationClick?: (ref: string) => void;
  onRetract?: () => void;
  onCopy?: () => void;
}

const AssistantBody = memo(function AssistantBody({
  content,
  streaming = false,
  onCitationClick,
}: {
  content: string;
  streaming?: boolean;
  onCitationClick?: (ref: string) => void;
}) {
  const renderContent = useStreamingContent(content, streaming);

  const html = useMemo(() => {
    try {
      const result = renderMarkdownWithProfile(
        renderContent || "",
        "chat_assistant",
        {
          streaming,
        },
      );
      return result.output;
    } catch (err) {
      console.warn("[ai-message] Markdown render failed", {
        content: (renderContent || "").slice(0, 200),
        error: String(err),
      });
      const escaped = (renderContent || "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>");
      return `<p class="text-muted-foreground whitespace-pre-wrap">${escaped}</p>`;
    }
  }, [renderContent, streaming]);

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
});

/** AI 对话气泡壳（选区限制在 `.ai-message-body` 内） */
export const AiMessageBubble = memo(function AiMessageBubble({
  role,
  content,
  streaming = false,
  selected = false,
  createdAt,
  children,
  className,
  onCitationClick,
  onRetract,
  onCopy,
}: AiMessageBubbleProps) {
  const isUser = role === "user";

  const timeLabel = useMemo(() => {
    if (!createdAt) return null;
    try {
      const d = new Date(createdAt);
      return d.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    } catch {
      return null;
    }
  }, [createdAt]);

  const userHtml = useMemo(() => {
    if (!isUser) return "";
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
  }, [isUser, content]);

  if (isUser) {
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
        "ai-message-bubble ai-message-bubble-assistant group relative max-w-full overflow-hidden",
        streaming && "ai-message-bubble-streaming min-h-[2em] contain-layout",
        selected && "rounded-md ring-2 ring-primary/60",
        className,
      )}
      data-streaming={streaming ? "" : undefined}
    >
      {selected ? (
        <div className="absolute left-1.5 top-1.5 z-10 flex h-5 w-5 items-center justify-center rounded bg-primary text-primary-foreground">
          <Check className="h-3 w-3" />
        </div>
      ) : null}
      {(onRetract || onCopy) && !streaming ? (
        <div className="absolute right-1.5 top-1.5 z-10 flex items-center gap-0.5 opacity-0 transition-opacity group-hover:opacity-100">
          {onCopy ? (
            <button
              type="button"
              title="复制此消息"
              className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground/40 hover:bg-muted hover:text-muted-foreground"
              onClick={onCopy}
            >
              <Copy className="h-3.5 w-3.5" />
            </button>
          ) : null}
          {onRetract ? (
            <button
              type="button"
              title="撤回此消息及后续所有消息"
              className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground/40 hover:bg-muted hover:text-muted-foreground"
              onClick={onRetract}
            >
              <RotateCcw className="h-3.5 w-3.5" />
            </button>
          ) : null}
        </div>
      ) : null}
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
      {timeLabel ? (
        <span className="px-3 pb-1.5 text-[10px] text-muted-foreground/40">
          {timeLabel}
        </span>
      ) : null}
    </div>
  );
});
