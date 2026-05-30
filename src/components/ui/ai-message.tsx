import { AiMessageBubble } from "@/components/ai/AiMessageBubble";
import { cn } from "@/lib/utils";

export { AiStreamPulse } from "./ai-message-stream-pulse";

interface AiMessageProps {
  role: "user" | "assistant" | "system";
  content?: string;
  streaming?: boolean;
  className?: string;
  onCitationClick?: (ref: string) => void;
}

/** 兼容导出：系统消息与旧调用方 */
export function AiMessage({
  role,
  content,
  streaming = false,
  className,
  onCitationClick,
}: AiMessageProps) {
  if (role === "system") {
    return (
      <div
        className={cn(
          "ai-msg-system text-[11px] italic leading-snug text-muted-foreground",
          className,
        )}
      >
        {content}
      </div>
    );
  }

  return (
    <AiMessageBubble
      role={role}
      content={content}
      streaming={streaming}
      className={className}
      onCitationClick={onCitationClick}
    />
  );
}
