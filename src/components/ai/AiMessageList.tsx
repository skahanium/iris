import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";

import { Check, Copy, RotateCcw } from "lucide-react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { useToast } from "@/components/ui/use-toast";
import type { MentionToken } from "@/lib/ai-context-scope";
import {
  restoreChatLineContent,
  type AiPayloadRef,
} from "@/lib/ai-payload-store";
import {
  citationRecordsFromContextPackets,
  replaceAiCitationsForDocument,
} from "@/lib/ai/evidence-citations";
import type { ContentPart, ContextPacket, ToolCallInfo } from "@/types/ai";

export interface ImageAttachment {
  id: string;
  dataBase64: string;
  mimeType: string;
  fileName?: string;
  sizeBytes: number;
}

export interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
  /** Reference to full content when React state only keeps a bounded projection. */
  contentRef?: AiPayloadRef;
  /** 多模态原始数据（传给后端）；纯文本时为 undefined */
  contentParts?: ContentPart[];
  /** 前端渲染用图片列表 */
  images?: ImageAttachment[];
  /** 前端展示用 @ 文档/文件夹引用元信息，不参与后端 prompt。 */
  mentions?: MentionToken[];
  seq?: number;
  created_at?: string;
  toolCalls?: ToolCallInfo[];
  /** 历史会话恢复用：该助手消息产生的证据包。 */
  evidencePackets?: ContextPacket[];
}

interface AiMessageListProps {
  messages: ChatLine[];
  streaming: boolean;
  selectedIndices?: Set<number>;
  onCitationClick?: (ref: string) => void;
  onRetract?: (index: number) => void;
  onSelect?: (
    index: number,
    event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
  ) => void;
}

type MessageRow =
  | { type: "empty" }
  | { type: "thinking" }
  | { type: "message"; message: ChatLine; messageIndex: number };

const SCROLL_WRITE_EPSILON_PX = 1;

function isRenderableMessageRow(
  message: ChatLine,
  messageIndex: number,
  messages: ChatLine[],
  streaming: boolean,
): boolean {
  if (message.role !== "assistant") return true;
  if (message.content.trim()) return true;
  return streaming && messageIndex === messages.length - 1;
}

function MessageSelectControl({
  selected,
  onSelect,
}: {
  selected: boolean;
  onSelect?: (event: {
    shiftKey: boolean;
    metaKey: boolean;
    ctrlKey: boolean;
  }) => void;
}) {
  if (!onSelect) return <span className="h-6 w-6" aria-hidden="true" />;

  return (
    <button
      type="button"
      aria-label={selected ? "取消选择此消息" : "选择此消息"}
      aria-pressed={selected}
      title={selected ? "取消选择此消息" : "选择此消息"}
      className={[
        "flex h-6 w-6 items-center justify-center rounded-md border text-[10px] transition",
        selected
          ? "border-primary bg-primary text-primary-foreground opacity-100"
          : "border-border/60 bg-panel/85 text-muted-foreground opacity-0 hover:border-primary/50 hover:text-foreground group-focus-within/ai-message-row:opacity-100 group-hover/ai-message-row:opacity-100",
      ].join(" ")}
      onClick={(event) => {
        event.preventDefault();
        event.stopPropagation();
        onSelect({
          shiftKey: event.shiftKey,
          metaKey: event.metaKey,
          ctrlKey: event.ctrlKey,
        });
      }}
    >
      <Check className="h-3.5 w-3.5" />
    </button>
  );
}

function AssistantMessageActions({
  onCopy,
  onRetract,
  copyDisabled,
}: {
  onCopy?: () => void;
  onRetract?: () => void;
  copyDisabled?: boolean;
}) {
  if (!onCopy && !onRetract) {
    return <span className="h-6 w-6" aria-hidden="true" />;
  }

  return (
    <div className="flex flex-col items-center gap-0.5 opacity-0 transition-opacity group-focus-within/ai-message-row:opacity-100 group-hover/ai-message-row:opacity-100">
      {onCopy ? (
        <button
          type="button"
          title="复制此消息"
          disabled={copyDisabled}
          className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground/45 hover:bg-muted hover:text-muted-foreground"
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            if (copyDisabled) return;
            onCopy();
          }}
        >
          <Copy className="h-3.5 w-3.5" />
        </button>
      ) : null}

      {onRetract ? (
        <button
          type="button"
          title="撤回此消息及后续所有消息"
          className="flex h-6 w-6 items-center justify-center rounded text-muted-foreground/45 hover:bg-muted hover:text-muted-foreground"
          onClick={(event) => {
            event.preventDefault();
            event.stopPropagation();
            onRetract();
          }}
        >
          <RotateCcw className="h-3.5 w-3.5" />
        </button>
      ) : null}
    </div>
  );
}

export const AiMessageList = memo(function AiMessageList({
  messages,
  streaming,
  selectedIndices,
  onCitationClick,
  onRetract,
  onSelect,
}: AiMessageListProps) {
  const last = messages[messages.length - 1];
  const showStandaloneThinking =
    streaming &&
    (messages.length === 0 ||
      last?.role === "user" ||
      (last?.role === "system" &&
        !messages.some((m) => m.role === "assistant")));
  const viewportRef = useRef<HTMLDivElement | null>(null);
  const [scrollFollow, setScrollFollow] = useState<"following" | "detached">(
    "following",
  );
  const toast = useToast();
  const rows = useMemo<MessageRow[]>(() => {
    if (messages.length === 0) return [{ type: "empty" }];
    return [
      ...(showStandaloneThinking ? [{ type: "thinking" } as const] : []),
      ...messages.flatMap((message, messageIndex) =>
        isRenderableMessageRow(message, messageIndex, messages, streaming)
          ? [
              {
                type: "message" as const,
                message,
                messageIndex,
              },
            ]
          : [],
      ),
    ];
  }, [messages, showStandaloneThinking, streaming]);

  // Content-aware row height estimate. The old fixed `() => 112` was wrong
  // by up to 10× for tall assistant messages, causing the virtualizer to
  // compute incorrect total height / offsets on first scroll-through (blank
  // gaps + scroll position jumps until measureElement lands real values).
  // This heuristic uses content length to get within ~2× of the true height,
  // so measureElement only nudges instead of recalculating drastically.
  const estimateSizeByContent = useCallback(
    (index: number): number => {
      const row = rows[index];
      if (!row || row.type !== "message") return 80;
      const content = row.message.content || "";
      // Base 56px (bubble chrome) + ~0.55px per char (wrapping at ~42 chars/
      // line at 11px font in a ~480px-wide panel → ~13px line height). Code
      // blocks (``` fences) add extra height — count them roughly.
      const fenceCount = (content.match(/```/g) ?? []).length;
      const codeBlockExtra = Math.floor(fenceCount / 2) * 80;
      const textHeight = Math.max(56, content.length * 0.55);
      // Cap at 2000 to avoid absurd estimates for very long messages.
      return Math.min(2000, textHeight + codeBlockExtra + 24);
    },
    [rows],
  );

  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => viewportRef.current,
    estimateSize: estimateSizeByContent,
    overscan: 8,
  });
  const rowVirtualizerRef = useRef(rowVirtualizer);
  rowVirtualizerRef.current = rowVirtualizer;
  const pendingMeasureNodesRef = useRef<Set<HTMLDivElement>>(new Set());
  const measureFrameRef = useRef<number | null>(null);
  const measureRowElement = useCallback((node: HTMLDivElement | null) => {
    if (!node) return;
    pendingMeasureNodesRef.current.add(node);
    if (measureFrameRef.current !== null) return;

    measureFrameRef.current = window.requestAnimationFrame(() => {
      measureFrameRef.current = null;
      const nodes = Array.from(pendingMeasureNodesRef.current);
      pendingMeasureNodesRef.current.clear();

      for (const measureNode of nodes) {
        if (!measureNode.isConnected) continue;
        rowVirtualizerRef.current.measureElement(measureNode);
      }
    });
  }, []);
  const virtualTotalSize = rowVirtualizer.getTotalSize();
  const virtualItems = rowVirtualizer.getVirtualItems();

  const isNearBottom = useCallback((viewport: HTMLDivElement) => {
    const threshold = 48;
    const distanceFromBottom =
      viewport.scrollHeight - viewport.scrollTop - viewport.clientHeight;
    return distanceFromBottom <= threshold;
  }, []);

  const handleScroll = useCallback(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;
    const next = isNearBottom(viewport) ? "following" : "detached";
    setScrollFollow((prev) => (prev === next ? prev : next));
  }, [isNearBottom]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport) return;

    viewport.addEventListener("scroll", handleScroll, { passive: true });
    const next = isNearBottom(viewport) ? "following" : "detached";
    setScrollFollow((prev) => (prev === next ? prev : next));

    return () => {
      viewport.removeEventListener("scroll", handleScroll);
    };
  }, [handleScroll, isNearBottom]);

  useEffect(() => {
    const viewport = viewportRef.current;
    if (!viewport || scrollFollow !== "following") return;

    const nextScrollTop = Math.max(
      0,
      viewport.scrollHeight - viewport.clientHeight,
    );
    if (
      Math.abs(viewport.scrollTop - nextScrollTop) <= SCROLL_WRITE_EPSILON_PX
    ) {
      return;
    }

    viewport.scrollTop = nextScrollTop;
  }, [messages, rows.length, virtualTotalSize, scrollFollow, streaming]);

  useEffect(() => {
    const pendingMeasureNodes = pendingMeasureNodesRef.current;

    return () => {
      if (measureFrameRef.current !== null) {
        window.cancelAnimationFrame(measureFrameRef.current);
        measureFrameRef.current = null;
      }
      pendingMeasureNodes.clear();
    };
  }, []);

  // Stable per-index callback cache. Inline arrows like `() => onRetract(i)`
  // create new function refs every render, breaking AiMessageBubble's memo
  // during streaming (every bubble re-diffs at ~20fps). This Map persists
  // across renders so each index always gets the same function ref.
  const retractCallbackRef = useRef<Map<number, () => void>>(new Map());
  const copyCallbackRef = useRef<Map<number, () => void>>(new Map());
  const messagesRef = useRef(messages);

  useEffect(() => {
    messagesRef.current = messages;
  }, [messages]);

  useEffect(() => {
    for (const key of retractCallbackRef.current.keys()) {
      if (key >= messages.length) retractCallbackRef.current.delete(key);
    }
    for (const key of copyCallbackRef.current.keys()) {
      if (key >= messages.length) copyCallbackRef.current.delete(key);
    }
  }, [messages.length]);

  const handleMessageSelect = useCallback(
    (
      index: number,
      event: { shiftKey: boolean; metaKey: boolean; ctrlKey: boolean },
    ) => {
      onSelect?.(index, event);
    },
    [onSelect],
  );

  const handleCopyMessage = useCallback(
    async (message: ChatLine) => {
      const ledger = citationRecordsFromContextPackets(message.evidencePackets);
      const content =
        message.role === "assistant"
          ? replaceAiCitationsForDocument(
              restoreChatLineContent(message),
              ledger,
            ).markdown
          : restoreChatLineContent(message);
      try {
        if (!navigator.clipboard?.writeText) {
          throw new Error("Clipboard API is unavailable");
        }
        await navigator.clipboard.writeText(content);
        toast("已复制回答", { tone: "success" });
      } catch {
        toast("复制失败", { tone: "error" });
      }
    },
    [toast],
  );

  const renderRow = (row: MessageRow) => {
    if (row.type === "empty") {
      return (
        <p className="py-8 text-center text-xs text-muted-foreground">
          输入问题开始对话。证据包在上方，工具与 Token 状态见底栏。
        </p>
      );
    }

    if (row.type === "thinking") {
      return <AiMessageBubble role="assistant" streaming />;
    }

    const m = row.message;
    const i = row.messageIndex;
    const isLast = i === messages.length - 1;
    const assistantStreaming = streaming && m.role === "assistant" && isLast;
    const isSelected = selectedIndices?.has(i) ?? false;

    if (m.role === "assistant") {
      const msgContent = m.content || "";
      // Fetch or create stable callbacks for this index. The Map persists
      // across renders, so the same index always gets the same function ref,
      // preserving AiMessageBubble's memo during streaming re-renders.
      let retractCb = retractCallbackRef.current.get(i);
      if (!retractCb && onRetract) {
        retractCb = () => onRetract(i);
        retractCallbackRef.current.set(i, retractCb);
      }
      let copyCb = copyCallbackRef.current.get(i);
      if (!copyCb) {
        copyCb = () => {
          const latestMessage = messagesRef.current[i];
          if (latestMessage) void handleCopyMessage(latestMessage);
        };
        copyCallbackRef.current.set(i, copyCb);
      }
      return (
        <div className="group/ai-message-row grid w-full grid-cols-[1.75rem_minmax(0,1fr)] items-start gap-1">
          <div className="flex flex-col items-center gap-1 pt-1">
            <MessageSelectControl
              selected={isSelected}
              onSelect={
                onSelect ? (event) => handleMessageSelect(i, event) : undefined
              }
            />
            <AssistantMessageActions
              onCopy={copyCb}
              onRetract={retractCb}
              copyDisabled={!msgContent}
            />
          </div>
          <div className="min-w-0 max-w-full flex-1">
            <AiMessageBubble
              role="assistant"
              content={msgContent || undefined}
              streaming={assistantStreaming}
              selected={isSelected}
              createdAt={m.created_at}
              onCitationClick={onCitationClick}
            />
          </div>
        </div>
      );
    }

    if (m.role === "user") {
      return (
        <div className="group/ai-message-row flex w-full items-start justify-end gap-1">
          <div className="pt-1">
            <MessageSelectControl
              selected={isSelected}
              onSelect={
                onSelect ? (event) => handleMessageSelect(i, event) : undefined
              }
            />
          </div>
          <AiMessageBubble
            role="user"
            content={m.content}
            selected={isSelected}
            images={m.images}
            mentions={m.mentions}
          />
        </div>
      );
    }

    return <AiMessage role={m.role} content={m.content} />;
  };

  return (
    <>
      <ScrollArea className="min-h-0 flex-1" viewportRef={viewportRef}>
        <div
          className="relative py-3"
          style={{ height: `${virtualTotalSize}px` }}
        >
          {virtualItems.map((virtualRow) => {
            const row = rows[virtualRow.index];
            if (!row) return null;
            return (
              <div
                key={virtualRow.key}
                ref={measureRowElement}
                data-index={virtualRow.index}
                className="absolute left-0 top-0 w-full px-3"
                style={{ transform: `translateY(${virtualRow.start}px)` }}
              >
                <div className="pb-4">{renderRow(row)}</div>
              </div>
            );
          })}
        </div>
      </ScrollArea>
    </>
  );
});
