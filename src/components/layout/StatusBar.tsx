import { useMemo } from "react";

import { Button } from "@/components/ui/button";
import { formatEditorZoomPercent } from "@/lib/editor-zoom";
import { splitFrontmatter } from "@/lib/frontmatter";
import { readingMinutes } from "@/lib/reading-time";

interface StatusBarProps {
  path: string | null;
  /** User-facing document name (`files.title`). */
  documentTitle?: string | null;
  wordCount: number;
  markdown?: string;
  aiStatus: string;
  editorZoom?: number;
  onEditorZoomIn?: () => void;
  onEditorZoomOut?: () => void;
  onEditorZoomReset?: () => void;
}

export function StatusBar({
  path,
  documentTitle,
  wordCount,
  markdown = "",
  aiStatus,
  editorZoom = 1,
  onEditorZoomIn,
  onEditorZoomOut,
  onEditorZoomReset,
}: StatusBarProps) {
  const label = documentTitle ?? path ?? "未打开文件";

  const bodyText = useMemo(() => splitFrontmatter(markdown).body, [markdown]);
  const minutes = useMemo(() => readingMinutes(bodyText), [bodyText]);

  return (
    <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-border bg-panel px-3 font-sans text-[11px] tracking-wide text-muted-foreground">
      <span className="min-w-0 truncate" title={path ?? undefined}>
        {label}
      </span>
      <span className="shrink-0 text-muted-foreground/60" aria-hidden>
        ·
      </span>
      <span className="shrink-0 tabular-nums">
        {wordCount.toLocaleString()} 字
      </span>
      <span className="shrink-0 text-muted-foreground/60" aria-hidden>
        ·
      </span>
      <span className="shrink-0 tabular-nums">约 {minutes} 分钟</span>
      {onEditorZoomIn && onEditorZoomOut && onEditorZoomReset ? (
        <>
          <span className="shrink-0 text-muted-foreground/60" aria-hidden>
            ·
          </span>
          <span
            className="flex shrink-0 items-center gap-0.5"
            role="group"
            aria-label="编辑器缩放"
          >
            <Button
              type="button"
              variant="ghost"
              className="h-6 min-w-6 px-1.5 text-[11px] tabular-nums"
              aria-label="缩小"
              onClick={onEditorZoomOut}
            >
              −
            </Button>
            <Button
              type="button"
              variant="ghost"
              className="h-6 min-w-[2.75rem] px-1.5 text-[11px] tabular-nums"
              aria-label="重置缩放"
              onClick={onEditorZoomReset}
            >
              {formatEditorZoomPercent(editorZoom)}
            </Button>
            <Button
              type="button"
              variant="ghost"
              className="h-6 min-w-6 px-1.5 text-[11px] tabular-nums"
              aria-label="放大"
              onClick={onEditorZoomIn}
            >
              +
            </Button>
          </span>
        </>
      ) : null}
      <span className="shrink-0 text-muted-foreground/60" aria-hidden>
        ·
      </span>
      <span className="ml-auto shrink-0 truncate">{aiStatus}</span>
    </footer>
  );
}
