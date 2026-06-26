import { cn } from "@/lib/utils";

export { AiStreamPulse } from "./ai-message-stream-pulse";

interface AiMessageProps {
  role: "user" | "assistant" | "system";
  content?: string;
  streaming?: boolean;
  className?: string;
  onCitationClick?: (ref: string) => void;
}

/** 基础消息壳：业务气泡由 components/ai/AiMessageBubble 承担。 */
export function AiMessage({
  role,
  content,
  streaming = false,
  className,
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
    <div
      className={cn(
        "ai-message-bubble max-w-full rounded-lg px-3 py-2 text-sm leading-relaxed",
        role === "user"
          ? "ai-message-surface-user"
          : "ai-message-surface-assistant",
        streaming && "ai-message-bubble-streaming",
        className,
      )}
      data-streaming={streaming || undefined}
    >
      {content}
    </div>
  );
}
