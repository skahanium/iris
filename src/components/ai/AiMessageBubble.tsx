import {
  useCallback,
  useEffect,
  useMemo,
  memo,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";

import { ChevronDown } from "lucide-react";

import { MarkdownErrorBoundary } from "@/components/ui/markdown-error-boundary";

import { AiThinkingIndicator } from "@/components/ui/ai-message-stream-pulse";

import { createRenderableAssistantContent } from "@/lib/assistant-render-budget";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import {
  displayMentionTooltip,
  validDisplayMentions,
} from "@/lib/ai-context-scope";

import { cn } from "@/lib/utils";

import { useStreamingContent } from "@/hooks/useStreamingContent";
import { useMarkdownRenderWorker } from "@/hooks/useMarkdownRenderWorker";
import type { AssistantProcessItem } from "@/lib/assistant-process";
import type { DisplayMention } from "@/types/ai";
import { sanitizeHtml, toTrustedHtml } from "@/lib/sanitize";

interface AiMessageBubbleProps {
  role: "user" | "assistant";

  content?: string;

  streaming?: boolean;

  selected?: boolean;

  createdAt?: string;

  children?: ReactNode;

  className?: string;

  onCitationClick?: (ref: string) => void;

  /** User-attached image list. */
  images?: import("./AiMessageList").ImageAttachment[];

  /** Validated inline presentation metadata, separate from prompt input. */
  displayMentions?: DisplayMention[];

  /** Runtime-only safe process events. Never persisted as message content. */
  processItems?: AssistantProcessItem[];
}

const proseConversation =
  "iris-markdown-content select-text [&_a.ai-citation]:font-medium [&_a.ai-citation]:text-ai-citation [&_a.ai-citation]:underline [&_a.ai-citation]:decoration-ai-citation/40 [&_a.ai-citation]:underline-offset-2 hover:[&_a.ai-citation]:text-ai-citation-hover";

const STREAMING_SYNC_FALLBACK_CHAR_LIMIT = 40_000;

const codeCopyDefaultLabel = "\u590d\u5236";
const codeCopyDoneLabel = "\u5df2\u590d\u5236";
const codeCopyFailedLabel = "\u590d\u5236\u5931\u8d25";

function summarizeLogContent(value: string) {
  let hash = 0x811c9dc5;

  for (let i = 0; i < value.length; i += 1) {
    hash ^= value.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }

  return {
    length: value.length,
    hash: (hash >>> 0).toString(16).padStart(8, "0"),
  };
}

function escapeHtml(value: string): string {
  return value
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function placeholdersInsideRawHtml(
  markedContent: string,
  placeholders: readonly string[],
): Set<string> {
  if (typeof DOMParser === "undefined") return new Set(placeholders);

  const sourceDocument = new DOMParser().parseFromString(
    markedContent,
    "text/html",
  );
  const insideRawHtml = new Set<string>();
  const walker = sourceDocument.createTreeWalker(
    sourceDocument.body,
    NodeFilter.SHOW_TEXT,
  );
  let node = walker.nextNode();
  while (node) {
    if (node.parentElement !== sourceDocument.body) {
      placeholders.forEach((placeholder) => {
        if (node?.nodeValue?.includes(placeholder)) {
          insideRawHtml.add(placeholder);
        }
      });
    }
    node = walker.nextNode();
  }
  sourceDocument.body.querySelectorAll("*").forEach((element) => {
    Array.from(element.attributes).forEach((attribute) => {
      placeholders.forEach((placeholder) => {
        if (attribute.value.includes(placeholder)) {
          insideRawHtml.add(placeholder);
        }
      });
    });
  });
  return insideRawHtml;
}

function restoreMentionPlaceholders(
  rendered: string,
  markedContent: string,
  mentions: readonly DisplayMention[],
  placeholders: readonly string[],
): string {
  if (typeof DOMParser === "undefined") {
    return sanitizeHtml(
      placeholders.reduce(
        (html, placeholder, index) =>
          html.replaceAll(
            placeholder,
            escapeHtml(mentions[index]?.label ?? ""),
          ),
        rendered,
      ),
    );
  }

  const rawHtmlPlaceholders = placeholdersInsideRawHtml(
    markedContent,
    placeholders,
  );
  const renderedDocument = new DOMParser().parseFromString(
    rendered,
    "text/html",
  );
  const mentionByPlaceholder = new Map(
    placeholders.map((placeholder, index) => [placeholder, mentions[index]]),
  );

  renderedDocument.body.querySelectorAll("*").forEach((element) => {
    Array.from(element.attributes).forEach((attribute) => {
      let value = attribute.value;
      mentionByPlaceholder.forEach((mention, placeholder) => {
        if (mention) value = value.replaceAll(placeholder, mention.label);
      });
      if (value !== attribute.value)
        element.setAttribute(attribute.name, value);
    });
  });

  const walker = renderedDocument.createTreeWalker(
    renderedDocument.body,
    NodeFilter.SHOW_TEXT,
  );
  const textNodes: Text[] = [];
  let node = walker.nextNode();
  while (node) {
    if (
      placeholders.some((placeholder) => node?.nodeValue?.includes(placeholder))
    ) {
      textNodes.push(node as Text);
    }
    node = walker.nextNode();
  }

  textNodes.forEach((textNode) => {
    const text = textNode.nodeValue ?? "";
    const fragment = renderedDocument.createDocumentFragment();
    let cursor = 0;
    while (cursor < text.length) {
      let nextPlaceholder = "";
      let nextIndex = -1;
      mentionByPlaceholder.forEach((_mention, placeholder) => {
        const index = text.indexOf(placeholder, cursor);
        if (index >= 0 && (nextIndex < 0 || index < nextIndex)) {
          nextIndex = index;
          nextPlaceholder = placeholder;
        }
      });
      if (nextIndex < 0) {
        fragment.append(text.slice(cursor));
        break;
      }
      fragment.append(text.slice(cursor, nextIndex));
      const mention = mentionByPlaceholder.get(nextPlaceholder);
      if (mention) {
        const plainOnly =
          rawHtmlPlaceholders.has(nextPlaceholder) ||
          Boolean(textNode.parentElement?.closest("code, pre"));
        if (plainOnly) {
          fragment.append(mention.label);
        } else {
          const span = renderedDocument.createElement("span");
          span.className = "ai-display-mention";
          span.title = displayMentionTooltip(mention);
          span.textContent = mention.label;
          fragment.append(span);
        }
      }
      cursor = nextIndex + nextPlaceholder.length;
    }
    textNode.replaceWith(fragment);
  });

  return sanitizeHtml(renderedDocument.body.innerHTML);
}

function renderUserMessageHtml(
  content: string,
  displayMentions: readonly DisplayMention[],
): string {
  const mentions = validDisplayMentions(content, displayMentions);
  if (mentions.length === 0) {
    return renderMarkdownWithProfile(content, "chat_user", {
      streaming: false,
    }).output;
  }

  let placeholderPrefix = "IRISDISPLAYMENTIONPLACEHOLDER";
  while (content.includes(placeholderPrefix)) placeholderPrefix += "X";
  const placeholders = mentions.map(
    (_mention, index) => `${placeholderPrefix}${index}END`,
  );
  let markedContent = content;
  for (let index = mentions.length - 1; index >= 0; index -= 1) {
    const mention = mentions[index];
    const placeholder = placeholders[index];
    if (!mention || !placeholder) continue;
    markedContent = `${markedContent.slice(0, mention.range.from)}${placeholder}${markedContent.slice(mention.range.to)}`;
  }

  const rendered = renderMarkdownWithProfile(markedContent, "chat_user", {
    streaming: false,
  }).output;
  return restoreMentionPlaceholders(
    rendered,
    markedContent,
    mentions,
    placeholders,
  );
}

function formatProcessDuration(durationMs: number | null | undefined): string {
  if (typeof durationMs !== "number" || !Number.isFinite(durationMs)) {
    return "";
  }
  if (durationMs < 1000) return `${Math.max(0, Math.round(durationMs))}ms`;
  return `${(durationMs / 1000).toFixed(1)}s`;
}

function AssistantProcessTimeline({
  events,
  streaming,
  hasContent,
}: {
  events: AssistantProcessItem[];
  streaming: boolean;
  hasContent: boolean;
}) {
  const [open, setOpen] = useState(() => streaming && !hasContent);
  const autoCollapsedRef = useRef(false);

  useEffect(() => {
    if (events.length === 0) return;
    if (streaming && !hasContent && !autoCollapsedRef.current) {
      setOpen(true);
      return;
    }
    if (hasContent && !autoCollapsedRef.current) {
      setOpen(false);
      autoCollapsedRef.current = true;
    }
  }, [events.length, hasContent, streaming]);

  if (events.length === 0) return null;

  const latest = events.at(-1);

  return (
    <div
      className="border-b border-border/40 px-3 py-2 text-[11px] text-muted-foreground"
      data-testid="assistant-process-timeline"
    >
      <button
        type="button"
        className="flex w-full min-w-0 items-center gap-1.5 text-left"
        onClick={() => setOpen((value) => !value)}
        aria-expanded={open}
      >
        <ChevronDown
          className={cn(
            "h-3.5 w-3.5 shrink-0 transition-transform",
            !open && "-rotate-90",
          )}
        />
        <span className="shrink-0 font-medium text-foreground/75">
          处理过程
        </span>
        {!open && latest ? (
          <span className="min-w-0 truncate text-muted-foreground">
            {latest.label}
          </span>
        ) : null}
      </button>
      {open ? (
        <ol className="mt-2 max-h-52 space-y-1.5 overflow-y-auto pl-5 pr-1">
          {events.map((event) => {
            const duration = formatProcessDuration(event.durationMs);
            return (
              <li key={event.id} className="list-disc pl-0.5">
                <span>{event.label}</span>
                {duration ? (
                  <span className="text-muted-foreground/70">
                    {" "}
                    · {duration}
                  </span>
                ) : null}
              </li>
            );
          })}
        </ol>
      ) : null}
    </div>
  );
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
  const renderable = useMemo(
    () => createRenderableAssistantContent(content, { streaming }),
    [content, streaming],
  );
  const renderContent = useStreamingContent(renderable.content, streaming);

  const markdownContent = streaming ? renderContent : content;
  const boundedMarkdownContent = streaming
    ? markdownContent
    : renderable.content;

  const workerRender = useMarkdownRenderWorker({
    content: boundedMarkdownContent,
    enabled: streaming,
    streaming,
  });

  /** Last successfully rendered HTML — reused while worker is pending. */
  const lastHtmlRef = useRef<string>("");

  const html = useMemo(() => {
    if (streaming && !workerRender.failed) {
      if (workerRender.html !== null) {
        lastHtmlRef.current = workerRender.html;
        return workerRender.html;
      }
      // Worker is still computing. Reuse a previous frame when one exists;
      // otherwise render short first frames synchronously so streaming is visible.
      if (workerRender.pending) {
        if (lastHtmlRef.current) {
          return lastHtmlRef.current;
        }
        if (content.length > STREAMING_SYNC_FALLBACK_CHAR_LIMIT) {
          return '<p class="text-muted-foreground whitespace-pre-wrap">Rendering...</p>';
        }
      }
    }

    // Non-streaming or worker failed: render synchronously.
    try {
      const result = renderMarkdownWithProfile(
        boundedMarkdownContent || "",

        "chat_assistant",

        {
          streaming,
        },
      );

      const out = result.output;
      if (streaming) lastHtmlRef.current = out;
      return out;
    } catch (err) {
      console.warn("[ai-message] Markdown render failed", {
        contentSummary: summarizeLogContent(boundedMarkdownContent || ""),

        error:
          err instanceof Error
            ? { name: err.name, messageLength: err.message.length }
            : { name: typeof err, messageLength: String(err).length },
      });

      const escaped = (boundedMarkdownContent || "")

        .replace(/&/g, "&amp;")

        .replace(/</g, "&lt;")

        .replace(/>/g, "&gt;")

        .replace(/\n/g, "<br>");

      return `<p class="text-muted-foreground whitespace-pre-wrap">${escaped}</p>`;
    }
  }, [
    boundedMarkdownContent,
    content.length,
    streaming,
    workerRender.failed,
    workerRender.html,
    workerRender.pending,
  ]);

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

      const external = target.closest("a[href^='https://']");
      if (external instanceof HTMLAnchorElement) {
        e.preventDefault();
        e.stopPropagation();
        onCitationClick(external.href);
        return;
      }

      const anchor = target.closest("a.ai-citation, a[href^='#iris-cite-']");

      if (!anchor || !(anchor instanceof HTMLAnchorElement)) return;

      const ref =
        anchor.dataset.citeRef ??
        anchor.getAttribute("href")?.replace(/^#iris-cite-/, "");

      if (!ref) return;

      e.preventDefault();

      try {
        onCitationClick(decodeURIComponent(ref));
      } catch (decodeError) {
        console.warn(
          "[ai-message] decodeURIComponent failed, using raw ref:",
          decodeError,
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
      dangerouslySetInnerHTML={{ __html: toTrustedHtml(html) }}
      onClick={handleClick}
    />
  );
});

/** AI conversation message; selection is limited to `.ai-message-body`. */

export const AiMessageBubble = memo(function AiMessageBubble({
  role,

  content,

  streaming = false,

  selected = false,

  createdAt,

  children,

  className,

  onCitationClick,

  images,

  displayMentions,

  processItems = [],
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
      return renderUserMessageHtml(content || "", displayMentions ?? []);
    } catch {
      return (content || "")

        .replace(/&/g, "&amp;")

        .replace(/</g, "&lt;")

        .replace(/>/g, "&gt;")

        .replace(/\n/g, "<br>");
    }
  }, [isUser, content, displayMentions]);

  if (isUser) {
    return (
      <div
        className={cn(
          "ai-message-bubble ai-message-bubble-user ai-message-surface-user self-end",

          selected && "ring-1 ring-primary/50",

          className,
        )}
        data-selected={selected ? "" : undefined}
      >
        {images && images.length > 0 && (
          <div className="mb-1.5 flex flex-wrap gap-1.5">
            {images.map((img) => (
              <img
                key={img.id}
                src={`data:${img.mimeType};base64,${img.dataBase64}`}
                className="max-h-40 max-w-[15rem] rounded-lg border border-border/40 object-contain"
                alt={img.fileName || "attached image"}
              />
            ))}
          </div>
        )}
        <div
          className={cn("ai-message-body", proseConversation)}
          data-prose-surface="conversation"
          dangerouslySetInnerHTML={{ __html: toTrustedHtml(userHtml) }}
        />
      </div>
    );
  }

  const hasProcessEvents = processItems.length > 0;
  const showThinking = streaming && !content && !hasProcessEvents;

  return (
    <div
      className={cn(
        "ai-message-assistant ai-message-bubble ai-message-bubble-assistant ai-message-surface-assistant relative w-full max-w-full overflow-hidden",

        streaming && "ai-message-bubble-streaming",

        selected && "ring-1 ring-primary/50",

        className,
      )}
      data-role={role}
      data-streaming={streaming ? "" : undefined}
      data-selected={selected ? "" : undefined}
    >
      {hasProcessEvents ? (
        <AssistantProcessTimeline
          events={processItems}
          streaming={streaming}
          hasContent={Boolean(content)}
        />
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
