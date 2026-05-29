import type { NodeViewProps } from "@tiptap/react";
import { NodeViewContent, NodeViewWrapper } from "@tiptap/react";
import { Check, RotateCw, X } from "lucide-react";

import { Button } from "@/components/ui/button";

import { inlineAiActionLabel } from "@/lib/inline-ai-actions";

import type { AiStreamOptions } from "./extensions/AiStreamExtension";

export function AiNodeView({ editor, node }: NodeViewProps) {
  const status = node.attrs.status as string;
  const action = typeof node.attrs.action === "string" ? node.attrs.action : "";
  const isStreaming = status === "streaming";

  const extension = editor.extensionManager.extensions.find(
    (e) => e.name === "aiStream",
  );
  const onRetry = (extension?.options as AiStreamOptions | undefined)?.onRetry;

  return (
    <NodeViewWrapper className="my-3 rounded-lg border border-primary/40 bg-card/80 p-3">
      <div className="mb-2 flex items-center gap-2 text-xs text-primary">
        <span>
          {action ? inlineAiActionLabel(action) : "AI 建议"}
          {isStreaming ? "（生成中…）" : status === "error" ? "（失败）" : ""}
        </span>
        <div className="ml-auto flex gap-1">
          <Button
            type="button"
            size="sm"
            variant="ghost"
            title="接受：保留生成内容"
            disabled={isStreaming}
            onClick={() => editor.commands.acceptAiStream()}
          >
            <Check className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            title="重试：重新生成"
            disabled={isStreaming}
            onClick={() => onRetry?.(editor)}
          >
            <RotateCw className="h-4 w-4" />
          </Button>
          <Button
            type="button"
            size="sm"
            variant="ghost"
            title="回退：恢复原文"
            disabled={isStreaming}
            onClick={() => editor.commands.rollbackAiStream()}
          >
            <X className="h-4 w-4" />
          </Button>
        </div>
      </div>
      <NodeViewContent className="font-mono text-sm text-foreground" />
    </NodeViewWrapper>
  );
}
