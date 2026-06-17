import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useMemo, useRef, type MouseEvent } from "react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { ResearchResultMessage } from "@/components/ai/ResearchResultMessage";
import type { MentionToken } from "@/lib/ai-context-scope";
import type { ContentPart, ResearchFocusPayload } from "@/types/ai";

import type { ToolCallInfo } from "@/types/ai";

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
  kind?: "research";
  research?: ResearchFocusPayload;
}

interface AiMessageListProps {
  messages: ChatLine[];
  streaming: boolean;
  selectedIndices?: Set<number>;
  onCitationClick?: (ref: string) => void;
  onExpandResearch?: (result: ResearchFocusPayload) => void;
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
  onExpandResearch,
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
  const rowVirtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => viewportRef.current,
    estimateSize: () => 112,
    overscan: 8,
  });

  const handleBubbleClick = (index: number, e: MouseEvent) => {
    if (!onSelect) return;
    const target = e.target as HTMLElement;
    if (target.closest("a, button")) return;
    onSelect(index, {
      shiftKey: e.shiftKey,
      metaKey: e.metaKey,
      ctrlKey: e.ctrlKey,
    });
  };

  const handleCopyMessage = useCallback(async (content: string) => {
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
      streaming &&
      m.role === "assistant" &&
      m.kind !== "research" &&
      isLast &&
      !m.content;
    const isSelected = selectedIndices?.has(i) ?? false;

    if (m.role === "assistant" && m.kind === "research" && m.research) {
      const result = m.research;
      return (
        <div className="flex w-full justify-start">
          <ResearchResultMessage
            result={result}
            onExpandDetail={() => onExpandResearch?.(result)}
            className="w-full max-w-full"
          />
        </div>
      );
    }

    if (m.role === "assistant") {
      const msgContent = m.content || "";
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
              onRetract={onRetract ? () => onRetract(i) : undefined}
              onCopy={
                msgContent ? () => handleCopyMessage(msgContent) : undefined
              }
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
