import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";
import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { ResearchResultMessage } from "@/components/ai/ResearchResultMessage";
import type { ResearchFocusPayload } from "@/types/ai";

import { ToolCallList, type ToolCallInfo } from "./ToolCallBubble";

export interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
  toolCalls?: ToolCallInfo[];
  kind?: "research";
  research?: ResearchFocusPayload;
}

interface AiMessageListProps {
  messages: ChatLine[];
  streaming: boolean;
  onCitationClick?: (ref: string) => void;
  onExpandResearch?: (result: ResearchFocusPayload) => void;
}

export function AiMessageList({
  messages,
  streaming,
  onCitationClick,
  onExpandResearch,
}: AiMessageListProps) {
  const last = messages[messages.length - 1];
  const showStandaloneThinking =
    streaming &&
    (messages.length === 0 ||
      last?.role === "user" ||
      (last?.role === "system" &&
        !messages.some((m) => m.role === "assistant")));

  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="flex flex-col gap-3 px-3 py-3">
        {messages.length === 0 ? (
          <p className="py-8 text-center text-xs text-muted-foreground">
            输入问题开始对话。证据与工具调用将显示在下方。
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
            return (
              <div key={`${i}-${m.role}`} className="flex w-full justify-start">
                <div className="min-w-0 max-w-full flex-1 space-y-2">
                  <AiMessageBubble
                    role="assistant"
                    content={m.content || undefined}
                    streaming={assistantStreaming}
                    onCitationClick={onCitationClick}
                  />
                  {m.toolCalls && m.toolCalls.length > 0 ? (
                    <ToolCallList toolCalls={m.toolCalls} />
                  ) : null}
                </div>
              </div>
            );
          }

          if (m.role === "user") {
            return (
              <div key={`${i}-${m.role}`} className="flex w-full justify-end">
                <AiMessageBubble role="user" content={m.content} />
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
}
