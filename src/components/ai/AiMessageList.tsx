import { ScrollArea } from "@/components/ui/scroll-area";
import { AiMessage } from "@/components/ui/ai-message";

import { ToolCallList, type ToolCallInfo } from "./ToolCallBubble";

export interface ChatLine {
  role: "user" | "assistant" | "system";
  content: string;
  toolCalls?: ToolCallInfo[];
}

interface AiMessageListProps {
  messages: ChatLine[];
  streaming: boolean;
}

export function AiMessageList({ messages, streaming }: AiMessageListProps) {
  return (
    <ScrollArea className="min-h-0 flex-1">
      <div className="space-y-3 px-3 py-3">
        {messages.length === 0 ? (
          <p className="py-8 text-center text-xs text-muted-foreground">
            输入问题开始对话。证据与工具调用将显示在下方。
          </p>
        ) : null}
        {messages.map((m, i) => {
          const isLast = i === messages.length - 1;
          const assistantStreaming =
            streaming && m.role === "assistant" && isLast && !m.content;

          return (
            <div key={`${i}-${m.role}`}>
              {m.role === "assistant" ? (
                <>
                  <AiMessage
                    role="assistant"
                    content={m.content || undefined}
                    streaming={assistantStreaming}
                  />
                  {m.toolCalls && m.toolCalls.length > 0 ? (
                    <ToolCallList toolCalls={m.toolCalls} />
                  ) : null}
                </>
              ) : (
                <AiMessage role={m.role} content={m.content} />
              )}
            </div>
          );
        })}
      </div>
    </ScrollArea>
  );
}
