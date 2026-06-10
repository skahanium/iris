import {
  useCallback,
  useMemo,
  memo,
  type MouseEvent,
  type ReactNode,
} from "react";

import { Check, Copy, RotateCcw } from "lucide-react";

import { MarkdownErrorBoundary } from "@/components/ui/markdown-error-boundary";

import { AiThinkingIndicator } from "@/components/ui/ai-message-stream-pulse";

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

const proseConversation =
  "iris-markdown-content select-text [&_a.ai-citation]:font-medium [&_a.ai-citation]:text-ai-citation [&_a.ai-citation]:underline [&_a.ai-citation]:decoration-ai-citation/40 [&_a.ai-citation]:underline-offset-2 hover:[&_a.ai-citation]:text-ai-citation-hover";

const codeCopyDefaultLabel = "复制";
const codeCopyDoneLabel = "已复制";
const codeCopyFailedLabel = "复制失败";

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

  const handleCodeCopy = useCallback(async (button: HTMLButtonElement) => {
    const code = button.closest(".ai-code-block")?.querySelector("pre code");
    const text = code?.textContent ?? "";

    if (!text) return;

    const originalLabel = button.textContent || codeCopyDefaultLabel;

    try {
      if (!navigator.clipboard?.writeText) {
        throw new Error("Clipboard API is unavailable");
      }

      await navigator.clipboard.writeText(text);
      button.textContent = codeCopyDoneLabel;
    } catch {
      button.textContent = codeCopyFailedLabel;
    }

    window.setTimeout(() => {
      button.textContent = originalLabel;
    }, 1200);
  }, []);

  const handleClick = useCallback(
    (e: MouseEvent<HTMLDivElement>) => {
      const target = e.target as HTMLElement;

      const copyButton = target.closest("button[data-ai-code-copy]");

      if (copyButton instanceof HTMLButtonElement) {
        e.preventDefault();
        e.stopPropagation();
        void handleCodeCopy(copyButton);
        return;
      }

      if (!onCitationClick) return;

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

    [handleCodeCopy, onCitationClick],
  );

  return (
    <div
      className={cn(
        "ai-message-body",

        proseConversation,

        streaming && content && "opacity-[0.92]",
      )}
      data-prose-surface="conversation"
      dangerouslySetInnerHTML={{ __html: html }}
      onClick={handleClick}
    />
  );
});

/** AI 对话消息（选区限制在 `.ai-message-body` 内） */

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
          "ai-message-bubble ai-message-bubble-user ai-message-surface-user self-end",

          className,
        )}
      >
        <div
          className={cn("ai-message-body", proseConversation)}
          data-prose-surface="conversation"
          dangerouslySetInnerHTML={{ __html: userHtml }}
        />
      </div>
    );
  }

  const showThinking = streaming && !content;

  return (
    <div
      className={cn(
        "ai-message-assistant ai-message-bubble ai-message-bubble-assistant ai-message-surface-assistant group relative w-full max-w-full overflow-hidden",

        streaming && "ai-message-bubble-streaming",

        selected && "ring-2 ring-primary/60",

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

      {showThinking ? <AiThinkingIndicator /> : null}

      {content ? (
        <MarkdownErrorBoundary>
          <AssistantBody
            content={content}
            streaming={streaming}
            onCitationClick={onCitationClick}
          />
        </MarkdownErrorBoundary>
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
