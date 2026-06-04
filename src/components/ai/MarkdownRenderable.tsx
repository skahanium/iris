import { useMemo } from "react";

import { renderMarkdownWithProfile } from "@/lib/markdown-contract";
import { MarkdownErrorBoundary } from "@/components/ui/markdown-error-boundary";
import { cn } from "@/lib/utils";
import type { MarkdownProfile } from "@/lib/markdown-contract/types";

interface MarkdownRenderableProps {
  content: string;
  profile: MarkdownProfile;
  streaming?: boolean;
  className?: string;
}

/**
 * 统一 Markdown 渲染壳 — 所有 AI 表面共享。
 *
 * 包装 `renderMarkdownWithProfile`，内置：
 * - Error boundary（防止渲染异常崩溃）
 * - Sanitization（已在 contract 内部完成）
 * - 统一样式类（iris-prose 排版）
 * - 流式模式支持
 */
export function MarkdownRenderable({
  content,
  profile,
  streaming = false,
  className,
}: MarkdownRenderableProps) {
  const html = useMemo(() => {
    try {
      const result = renderMarkdownWithProfile(content || "", profile, {
        streaming,
      });
      return result.output;
    } catch {
      return (content || "")
        .replace(/&/g, "&amp;")
        .replace(/</g, "&lt;")
        .replace(/>/g, "&gt;")
        .replace(/\n/g, "<br>");
    }
  }, [content, profile, streaming]);

  return (
    <MarkdownErrorBoundary>
      <div
        className={cn(
          "ai-message-body iris-markdown-content select-text",
          streaming && "contain-layout",
          "[&_a.ai-citation]:font-medium [&_a.ai-citation]:text-ai-citation [&_a.ai-citation]:underline [&_a.ai-citation]:decoration-ai-citation/40 [&_a.ai-citation]:underline-offset-2 hover:[&_a.ai-citation]:text-ai-citation-hover",
          className,
        )}
        data-prose-surface="conversation"
        dangerouslySetInnerHTML={{ __html: html }}
      />
    </MarkdownErrorBoundary>
  );
}
