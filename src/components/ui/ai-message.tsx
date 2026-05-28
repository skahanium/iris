import { marked } from "marked";
import { useMemo, type ReactNode } from "react";

import { sanitizeHtml } from "@/lib/sanitize";
import { cn } from "@/lib/utils";

export function AiStreamPulse() {
  return (
    <span className="ai-stream-pulse" aria-hidden>
      <span />
      <span />
      <span />
    </span>
  );
}

interface AiMessageProps {
  role: "user" | "assistant" | "system";
  content?: string;
  streaming?: boolean;
  children?: ReactNode;
  className?: string;
}

function AssistantMarkdown({ content }: { content: string }) {
  const html = useMemo(() => {
    const raw = marked.parse(content || "", { async: false }) as string;
    return sanitizeHtml(raw);
  }, [content]);

  return (
    <div
      className="ai-msg text-sm leading-relaxed [&_code]:rounded [&_code]:bg-editor-code-bg [&_code]:px-1 [&_code]:font-mono [&_code]:text-editor-code-fg [&_p]:mb-2 [&_ul]:mb-2 [&_ul]:list-disc [&_ul]:pl-5"
      dangerouslySetInnerHTML={{ __html: html }}
    />
  );
}

export function AiMessage({
  role,
  content,
  streaming = false,
  children,
  className,
}: AiMessageProps) {
  if (role === "system") {
    return (
      <div
        className={cn(
          "ai-msg-system text-xs italic text-muted-foreground",
          className,
        )}
      >
        {content}
      </div>
    );
  }

  if (role === "user") {
    return (
      <div className={cn("ai-msg-user text-sm", className)}>{content}</div>
    );
  }

  return (
    <div className={cn("ai-msg-assistant", className)}>
      {content ? <AssistantMarkdown content={content} /> : null}
      {streaming && !content ? <AiStreamPulse /> : null}
      {children}
    </div>
  );
}
