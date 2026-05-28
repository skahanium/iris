import { memo, useMemo } from "react";

import { ConnectivityIndicators } from "@/components/layout/ConnectivityIndicators";
import { EditorZoomControl } from "@/components/layout/EditorZoomControl";
import { Kbd } from "@/components/ui/kbd";
import { splitFrontmatter } from "@/lib/frontmatter";
import { readingMinutes } from "@/lib/reading-time";
import { formatCommandPaletteShortcut } from "@/lib/utils";
import type { ConnectivityStatus } from "@/types/llm";

interface StatusBarProps {
  path: string | null;
  /** User-facing document name (`files.title`). */
  documentTitle?: string | null;
  /** Current note has unsaved edits (shown as text, not on tabs). */
  unsaved?: boolean;
  wordCount: number;
  markdown?: string;
  aiStatus: string;
  editorZoom?: number;
  onEditorZoomIn?: () => void;
  onEditorZoomOut?: () => void;
  onEditorZoomReset?: () => void;
  webSearch?: boolean;
  onWebSearchChange?: (enabled: boolean) => void;
  connectivity?: ConnectivityStatus | null;
  onOpenConnectivitySettings?: () => void;
}

export const StatusBar = memo(function StatusBar({
  path,
  documentTitle,
  unsaved = false,
  wordCount,
  markdown = "",
  aiStatus,
  editorZoom = 1,
  onEditorZoomIn,
  onEditorZoomOut,
  onEditorZoomReset,
  webSearch = false,
  onWebSearchChange,
  connectivity = null,
  onOpenConnectivitySettings,
}: StatusBarProps) {
  const trimmedTitle = documentTitle?.trim();
  const label = trimmedTitle || (path ? "无标题" : "未打开文件");

  const bodyText = useMemo(() => splitFrontmatter(markdown).body, [markdown]);
  const minutes = useMemo(() => readingMinutes(bodyText), [bodyText]);

  return (
    <footer className="flex h-8 shrink-0 items-center gap-3 border-t border-border/60 bg-surface-chrome px-3 font-sans text-[11px] tracking-wide text-muted-foreground">
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
          <EditorZoomControl
            editorZoom={editorZoom}
            onZoomIn={onEditorZoomIn}
            onZoomOut={onEditorZoomOut}
            onZoomReset={onEditorZoomReset}
          />
        </>
      ) : null}
      {path && unsaved ? (
        <>
          <span className="shrink-0 text-muted-foreground/60" aria-hidden>
            ·
          </span>
          <span className="shrink-0 text-muted-foreground">未保存</span>
        </>
      ) : null}
      <div className="ml-auto flex min-w-0 shrink-0 items-center gap-3">
        <span
          className="hidden shrink-0 items-center gap-1.5 text-muted-foreground sm:inline-flex"
          title="打开命令面板"
        >
          <span>命令</span>
          <Kbd>{formatCommandPaletteShortcut()}</Kbd>
        </span>
        <span className="hidden shrink-0 text-muted-foreground/60 sm:inline" aria-hidden>
          ·
        </span>
        <ConnectivityIndicators
          status={connectivity}
          onOpenSettings={onOpenConnectivitySettings}
          webSearch={webSearch}
          onWebSearchChange={onWebSearchChange}
        />
        <span className="text-muted-foreground/60" aria-hidden>
          ·
        </span>
        <span className="max-w-[10rem] truncate" title={aiStatus}>
          {aiStatus}
        </span>
      </div>
    </footer>
  );
});
