import { NodeViewWrapper } from "@tiptap/react";
import type { NodeViewProps } from "@tiptap/react";
import { Check, Loader2, RefreshCw, X } from "lucide-react";
import { useCallback, useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";

import type {
  InlineAiAction,
  InlineAiStatus,
} from "./extensions/InlineAiExtension";
import { INLINE_AI_ACTION_LABELS } from "./extensions/InlineAiExtension";

// ─── Component ───────────────────────────────────────────

export function InlineAiNodeView({
  node,
  updateAttributes,
  editor,
}: NodeViewProps) {
  const status = node.attrs.status as InlineAiStatus;
  const action = node.attrs.action as InlineAiAction;
  const context = node.attrs.context as string;
  const originalText = node.attrs.originalText as string;

  const [content, setContent] = useState("");
  const streamBuf = useRef("");

  // Execute AI action when status changes to "pending"
  useEffect(() => {
    if (status !== "pending") return;

    let cancelled = false;

    const execute = async () => {
      updateAttributes({ status: "streaming" });

      try {
        // Build the prompt based on action
        const prompt = buildPrompt(action, context, originalText);

        // Send to AI backend
        const { aiSendMessage } = await import("@/lib/ipc");
        const result = await aiSendMessage({
          scene: "drafting_assist",
          session_id: null,
          message: prompt,
        });

        if (cancelled) return;

        // Update content
        if (result.content) {
          setContent(result.content);
          updateAttributes({ status: "ready" });
        } else {
          updateAttributes({ status: "error" });
        }
      } catch (err) {
        if (!cancelled) {
          console.error("Inline AI error:", err);
          updateAttributes({ status: "error" });
        }
      }
    };

    void execute();

    return () => {
      cancelled = true;
    };
  }, [status, action, context, originalText, updateAttributes]);

  // Handle accept
  const handleAccept = useCallback(() => {
    editor.commands.acceptInlineAi();
  }, [editor]);

  // Handle reject
  const handleReject = useCallback(() => {
    editor.commands.rejectInlineAi();
  }, [editor]);

  // Handle retry
  const handleRetry = useCallback(() => {
    setContent("");
    streamBuf.current = "";
    updateAttributes({ status: "pending" });
  }, [updateAttributes]);

  const actionLabel = INLINE_AI_ACTION_LABELS[action] ?? action;

  return (
    <NodeViewWrapper
      className={cn(
        "my-2 rounded-lg border-2 border-dashed p-3 transition-colors",
        status === "pending" || status === "streaming"
          ? "border-primary/50 bg-primary/5"
          : status === "ready"
            ? "border-emerald-500/50 bg-emerald-500/5"
            : "border-destructive/50 bg-destructive/5",
      )}
      data-type="inline-ai"
    >
      {/* Header */}
      <div className="mb-2 flex items-center justify-between">
        <div className="flex items-center gap-2">
          {(status === "pending" || status === "streaming") && (
            <Loader2 className="h-4 w-4 animate-spin text-primary" />
          )}
          {status === "ready" && <Check className="h-4 w-4 text-emerald-500" />}
          {status === "error" && <X className="h-4 w-4 text-destructive" />}

          <span className="text-sm font-medium">
            {status === "streaming"
              ? `AI ${actionLabel}中…`
              : status === "pending"
                ? `准备${actionLabel}…`
                : status === "ready"
                  ? `AI ${actionLabel}完成`
                  : `${actionLabel}失败`}
          </span>
        </div>

        <div className="flex items-center gap-1">
          {status === "ready" && (
            <>
              <Button
                type="button"
                size="sm"
                variant="ghost"
                className="h-7 px-2"
                onClick={handleReject}
              >
                <X className="mr-1 h-3 w-3" />
                撤销
              </Button>
              <Button
                type="button"
                size="sm"
                className="h-7 px-2"
                onClick={handleAccept}
              >
                <Check className="mr-1 h-3 w-3" />
                采纳
              </Button>
            </>
          )}

          {status === "error" && (
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-7 px-2"
              onClick={handleRetry}
            >
              <RefreshCw className="mr-1 h-3 w-3" />
              重试
            </Button>
          )}
        </div>
      </div>

      {/* Original text preview */}
      {originalText && status !== "ready" && (
        <div className="mb-2 rounded bg-muted/50 p-2">
          <p className="mb-1 text-[10px] text-muted-foreground">原文：</p>
          <p className="line-clamp-3 text-xs text-muted-foreground">
            {originalText}
          </p>
        </div>
      )}

      {/* Content area */}
      {content && (
        <div className="whitespace-pre-wrap text-sm leading-relaxed">
          {content}
        </div>
      )}

      {/* Streaming placeholder */}
      {status === "streaming" && !content && (
        <div className="flex items-center gap-2 text-sm text-muted-foreground">
          <Loader2 className="h-3 w-3 animate-spin" />
          <span>生成中…</span>
        </div>
      )}

      {/* Keyboard shortcuts hint */}
      {status === "ready" && (
        <p className="mt-2 text-[10px] text-muted-foreground">
          Enter 采纳 · Esc 撤销
        </p>
      )}
    </NodeViewWrapper>
  );
}

// ─── Prompt Builder ──────────────────────────────────────

function buildPrompt(
  action: InlineAiAction,
  context: string,
  originalText: string,
): string {
  switch (action) {
    case "continue":
      return `请继续以下内容的写作，保持风格和主题一致：\n\n${context}`;
    case "rewrite":
      return `请改写以下文本，保持原意但改善表达：\n\n${originalText}`;
    case "expand":
      return `请扩写以下内容，增加更多细节和论述：\n\n${originalText}`;
    case "simplify":
      return `请简化以下内容，保留核心信息但更简洁：\n\n${originalText}`;
    case "cite":
      return `请为以下段落推荐合适的法规引用：\n\n${context}`;
    case "check":
      return `请检查以下内容的一致性和规范性，指出问题：\n\n${context}`;
    default:
      return `请处理以下内容：\n\n${context}`;
  }
}
