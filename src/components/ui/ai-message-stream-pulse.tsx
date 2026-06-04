import { cn } from "@/lib/utils";

export function AiStreamPulse({ className }: { className?: string }) {
  return (
    <span className={cn("ai-stream-pulse", className)} aria-hidden>
      <span />
      <span />
      <span />
    </span>
  );
}

/** 流式空内容时的单行思考指示 */
export function AiThinkingIndicator({ className }: { className?: string }) {
  return (
    <div
      className={cn("ai-thinking-row", className)}
      role="status"
      aria-live="polite"
      aria-label="正在生成回复"
    >
      <AiStreamPulse />
      <span>正在思考…</span>
    </div>
  );
}
