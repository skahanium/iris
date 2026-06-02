import { memo, useCallback, type MouseEvent } from "react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { ResearchResultMessage } from "@/components/ai/ResearchResultMessage";
import type { ResearchFocusPayload } from "@/types/ai";

import type { ToolCallInfo } from "@/types/ai";

export interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
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

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="flex flex-col gap-3 px-3 py-3">
        {messages.length === 0 ? (
          <p className="py-8 text-center text-xs text-muted-foreground">
            输入问题开始对话。证据包在上方，工具与 Token 状态见底栏。
          </p>
        ) : null}
        {showStandaloneThinking ? (
          <AiMessageBubble role="assistant" streaming />
        ) : null}
        {messages.map((m, i) => {
          const isLast = i === messages.length - 1;
          const assistantStreaming =
            streaming &&
            m.role === "assistant" &&
            m.kind !== "research" &&
            isLast &&
            !m.content;
          const isSelected = selectedIndices?.has(i) ?? false;

          if (m.role === "assistant" && m.kind === "research" && m.research) {
            return (
              <div key={`${i}-research`} className="flex w-full justify-start">
                <ResearchResultMessage
                  result={m.research}
                  onExpandDetail={() => onExpandResearch?.(m.research!)}
                  className="w-full max-w-full"
                />
              </div>
            );
          }

          if (m.role === "assistant") {
            const msgContent = m.content || "";
            return (
              <div
                key={`${i}-${m.role}`}
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
                      msgContent
                        ? () => handleCopyMessage(msgContent)
                        : undefined
                    }
                  />
                </div>
              </div>
            );
          }

          if (m.role === "user") {
            return (
              <div
                key={`${i}-${m.role}`}
                className="flex w-full justify-end"
                onClick={(e) => handleBubbleClick(i, e)}
              >
                <AiMessageBubble
                  role="user"
                  content={m.content}
                  selected={isSelected}
                />
              </div>
            );
          }

          return (
            <AiMessage
              key={`${i}-${m.role}`}
              role={m.role}
              content={m.content}
            />
          );
        })}
      </div>
    </ScrollArea>
  );
});
