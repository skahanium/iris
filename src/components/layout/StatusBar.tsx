import { memo, useMemo } from "react";

import { ConnectivityIndicators } from "@/components/layout/ConnectivityIndicators";
import { EditorZoomControl } from "@/components/layout/EditorZoomControl";
import { Kbd } from "@/components/ui/kbd";
import { splitFrontmatter } from "@/lib/frontmatter";
import { readingMinutes } from "@/lib/reading-time";
import { cn, formatCommandPaletteShortcut } from "@/lib/utils";
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
        />
        <span className="text-muted-foreground/60" aria-hidden>
          ·
        </span>
        <span className="max-w-[10rem] truncate" title={aiStatus}>
          {aiStatus}
        </span>
        {onWebSearchChange ? (
          <>
            <span className="text-muted-foreground/60" aria-hidden>
              ·
            </span>
            <button
              type="button"
              role="switch"
              aria-checked={webSearch}
              aria-label={webSearch ? "关闭联网搜索" : "开启联网搜索"}
              title="联网搜索"
              className="group inline-flex h-6 shrink-0 items-center gap-1.5 whitespace-nowrap rounded-md px-1.5 text-muted-foreground transition-[color,background-color,transform] duration-base ease-iris-out hover:bg-muted/60 focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel active:scale-[0.98]"
              onClick={() => onWebSearchChange(!webSearch)}
            >
              <span
                className="relative flex size-3.5 shrink-0 items-center justify-center"
                aria-hidden
              >
                <span
                  className={cn(
                    "absolute inset-0 rounded-full border transition-[border-color,transform,opacity] duration-base ease-iris-out",
                    webSearch
                      ? "scale-100 border-sky-600/50 opacity-100"
                      : "scale-[0.88] border-muted-foreground/25 opacity-90 group-hover:border-muted-foreground/40",
                  )}
                />
                <span
                  className={cn(
                    "relative size-2 rounded-full transition-[transform,box-shadow] duration-base ease-iris-out",
                    webSearch
                      ? "scale-110 bg-gradient-to-br from-sky-500 via-sky-600 to-sky-800 shadow-[0_0_0_1px_rgba(2,132,199,0.45),inset_0_1px_0_rgba(255,255,255,0.28)]"
                      : "scale-90 bg-gradient-to-br from-muted-foreground/50 to-muted-foreground/30 shadow-[inset_0_1px_0_rgba(255,255,255,0.08),inset_0_-1px_0_rgba(0,0,0,0.18)] group-hover:scale-95",
                  )}
                />
              </span>
              <span
                className={cn(
                  "transition-colors duration-base ease-iris-out",
                  webSearch ? "text-foreground/85" : "text-muted-foreground",
                )}
              >
                联网搜索
              </span>
            </button>
          </>
        ) : null}
      </div>
    </footer>
  );
});
