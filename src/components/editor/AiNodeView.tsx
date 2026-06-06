import type { NodeViewProps } from "@tiptap/react";
import { NodeViewContent, NodeViewWrapper } from "@tiptap/react";
import { Check, RotateCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";
import { cn } from "@/lib/utils";
import { inlineAiActionLabel } from "@/lib/inline-ai-actions";

import type { AiStreamOptions } from "./extensions/AiStreamExtension";

function stopEditorCapture(event: React.MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
}

export function AiNodeView({ editor, node }: NodeViewProps) {
  const status = node.attrs.status as string;
  const action = typeof node.attrs.action === "string" ? node.attrs.action : "";
  const isStreaming = status === "streaming";
  const isReady = status === "ready";
  const isError = status === "error";
  const hasContent = node.textContent.length > 0;

  const extension = editor.extensionManager.extensions.find(
    (e) => e.name === "aiStream",
  );
  const options = extension?.options as AiStreamOptions | undefined;
  const onRetry = options?.onRetry;

  const actionLabel = action ? inlineAiActionLabel(action) : "AI 建议";
  const statusLabel = isStreaming
    ? "生成中…"
    : isError
      ? "失败"
      : isReady
        ? "待确认"
        : "";

  return (
    <NodeViewWrapper
      className="my-2"
      data-testid="ai-stream-node"
      contentEditable={false}
    >
      <div
        className={cn(
          "overflow-hidden rounded-lg border border-border/60 bg-surface-elevated/90 shadow-sm",
          "border-l-2 border-l-primary/50",
        )}
      >
        <div
          className="flex flex-wrap items-center gap-2 border-b border-border/50 px-3 py-2"
          contentEditable={false}
          onMouseDown={stopEditorCapture}
        >
          <div className="min-w-0 flex-1">
            <p className="text-xs font-medium text-foreground">{actionLabel}</p>
            {statusLabel ? (
              <p className="text-[10px] text-muted-foreground">{statusLabel}</p>
            ) : null}
          </div>
          <div className="flex shrink-0 items-center gap-1">
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-8 gap-1 px-2 text-xs"
              title="放弃：移除候选并保留原文"
              data-testid="ai-stream-dismiss"
              onMouseDown={stopEditorCapture}
              onClick={() => editor.commands.dismissAiStream()}
            >
              <X className="h-3.5 w-3.5" />
              放弃
            </Button>
            <Button
              type="button"
              size="sm"
              variant="ghost"
              className="h-8 gap-1 px-2 text-xs"
              title="重试：重新生成"
              disabled={isStreaming}
              data-testid="ai-stream-retry"
              onMouseDown={stopEditorCapture}
              onClick={() => onRetry?.(editor)}
            >
              <RotateCw className="h-3.5 w-3.5" />
              重试
            </Button>
            <Button
              type="button"
              size="sm"
              variant="default"
              className="h-8 gap-1 px-2 text-xs"
              title="接受：用候选替换原文"
              disabled={!isReady || !hasContent}
              data-testid="ai-stream-accept"
              onMouseDown={stopEditorCapture}
              onClick={() => editor.commands.acceptAiStream()}
            >
              <Check className="h-3.5 w-3.5" />
              接受
            </Button>
          </div>
        </div>

        <div className="relative px-3 py-2.5">
          <NodeViewContent className="font-prose text-sm leading-relaxed text-foreground outline-none" />
          {isStreaming ? (
            <div
              className="mt-2 flex items-center gap-2 text-[10px] text-muted-foreground"
              aria-live="polite"
            >
              <span className="ai-stream-pulse" aria-hidden>
                <span />
                <span />
                <span />
              </span>
              正在生成…
            </div>
          ) : null}
        </div>
      </div>
    </NodeViewWrapper>
  );
}
