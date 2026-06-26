import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useMemo, useRef, type MouseEvent } from "react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import type { MentionToken } from "@/lib/ai-context-scope";
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
  const rows = useMemo<MessageRow[]>(() => {
    if (messages.length === 0) return [{ type: "empty" }];
    return [
      ...(showStandaloneThinking ? [{ type: "thinking" } as const] : []),
      ...messages.map((message, messageIndex) => ({
        type: "message" as const,
        message,
        messageIndex,
      })),
    ];
  }, [messages, showStandaloneThinking]);

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

  // Stable per-index callback cache. Inline arrows like `() => onRetract(i)`
  // create new function refs every render, breaking AiMessageBubble's memo
  // during streaming (every bubble re-diffs at ~20fps). This Map persists
  // across renders so each index always gets the same function ref.
  const retractCallbackRef = useRef<Map<number, () => void>>(new Map());
  const copyCallbackRef = useRef<Map<number, () => void>>(new Map());

  // Prune stale entries when the message list shrinks (e.g., retract/session
  // switch) so the Maps don't retain dead-index callbacks indefinitely.
  if (retractCallbackRef.current.size > messages.length) {
    retractCallbackRef.current = new Map();
    copyCallbackRef.current = new Map();
  }

  const handleBubbleClick = useCallback(
    (index: number, e: MouseEvent) => {
      if (!onSelect) return;
      const target = e.target as HTMLElement;
      if (target.closest("a, button")) return;
      onSelect(index, {
        shiftKey: e.shiftKey,
        metaKey: e.metaKey,
        ctrlKey: e.ctrlKey,
      });
    },
    [onSelect],
  );

  const handleCopyMessage = useCallback(async (message: ChatLine) => {
    const ledger = citationRecordsFromContextPackets(message.evidencePackets);
    const content =
      message.role === "assistant"
        ? replaceAiCitationsForDocument(message.content, ledger).markdown
        : message.content;
    try {
      await navigator.clipboard.writeText(content);
    } catch {
      /* ignore */
    }
  }, []);

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
    const assistantStreaming =
      streaming && m.role === "assistant" && isLast && !m.content;
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
      if (!copyCb && msgContent) {
        const messageRef = m;
        copyCb = () => void handleCopyMessage(messageRef);
        copyCallbackRef.current.set(i, copyCb);
      }
      return (
        <div
          className="flex w-full justify-start"
          onClick={(e) => handleBubbleClick(i, e)}
        >
          <div className="min-w-0 max-w-full flex-1">
            <AiMessageBubble
              role="assistant"
              content={msgContent || undefined}
              streaming={assistantStreaming}
              selected={isSelected}
              createdAt={m.created_at}
              onCitationClick={onCitationClick}
              onRetract={retractCb}
              onCopy={copyCb}
            />
          </div>
        </div>
      );
    }

    if (m.role === "user") {
      return (
        <div
          className="flex w-full justify-end"
          onClick={(e) => handleBubbleClick(i, e)}
        >
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
    <ScrollArea className="min-h-0 flex-1" viewportRef={viewportRef}>
      <div
        className="relative py-3"
        style={{ height: `${rowVirtualizer.getTotalSize()}px` }}
      >
        {rowVirtualizer.getVirtualItems().map((virtualRow) => {
          const row = rows[virtualRow.index];
          if (!row) return null;
          return (
            <div
              key={virtualRow.key}
              ref={rowVirtualizer.measureElement}
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
  );
});
