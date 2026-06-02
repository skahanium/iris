import { memo } from "react";

import { ConnectivityIndicators } from "@/components/layout/ConnectivityIndicators";
import { EditorZoomControl } from "@/components/layout/EditorZoomControl";
import { StatusBarTokenUsage } from "@/components/layout/StatusBarTokenUsage";
import { Kbd } from "@/components/ui/kbd";
import { formatCommandPaletteShortcut } from "@/lib/utils";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { ConnectivityStatus } from "@/types/llm";

interface StatusBarProps {
  path: string | null;
  /** User-facing document name (`files.title`). */
  documentTitle?: string | null;
  /** Current note has unsaved edits (shown as text, not on tabs). */
  unsaved?: boolean;
  characterCount: number;
  readingMinutes: number;
  aiStatus: string;
  editorZoom?: number;
  onEditorZoomIn?: () => void;
  onEditorZoomOut?: () => void;
  onEditorZoomReset?: () => void;
  webSearch?: boolean;
  onWebSearchChange?: (enabled: boolean) => void;
  connectivity?: ConnectivityStatus | null;
  onOpenConnectivitySettings?: () => void;
  /** ⌘K 和弦等待第二键 */
  keyboardLeaderPending?: boolean;
  /** AI 侧栏上报的 Token / 工具活动（见 UnifiedAssistantPanel） */
  assistantChrome?: AssistantChromeSnapshot | null;
}

export const StatusBar = memo(function StatusBar({
  path,
  documentTitle,
  unsaved = false,
  characterCount,
  readingMinutes,
  aiStatus,
  editorZoom = 1,
  onEditorZoomIn,
  onEditorZoomOut,
  onEditorZoomReset,
  webSearch = false,
  onWebSearchChange,
  connectivity = null,
  onOpenConnectivitySettings,
  keyboardLeaderPending = false,
  assistantChrome = null,
}: StatusBarProps) {
  const trimmedTitle = documentTitle?.trim();
  const label = trimmedTitle || (path ? "无标题" : "未打开文件");

  const toolLabel = assistantChrome?.toolActivityLabel?.trim() ?? null;
  const statusLine = keyboardLeaderPending
    ? "⌘K 等待第二键…"
    : toolLabel || aiStatus;
  const statusTitle = keyboardLeaderPending
    ? "⌘K 等待第二键，Esc 取消"
    : [toolLabel, !toolLabel ? aiStatus : null].filter(Boolean).join(" · ") ||
      aiStatus;

  return (
    <footer
      data-testid="status-bar"
      className="flex h-8 shrink-0 items-center gap-3 border-t border-border/60 bg-surface-chrome px-3 font-sans text-[11px] tracking-wide text-muted-foreground"
    >
      <span className="min-w-0 truncate" title={path ?? undefined}>
        {label}
      </span>
      <span className="shrink-0 text-muted-foreground/60" aria-hidden>
        ·
      </span>
      <span className="shrink-0 tabular-nums">
        {characterCount.toLocaleString()} 字
      </span>
      <span className="shrink-0 text-muted-foreground/60" aria-hidden>
        ·
      </span>
      <span className="shrink-0 tabular-nums">约 {readingMinutes} 分钟</span>
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
        <span
          className="hidden shrink-0 text-muted-foreground/60 sm:inline"
          aria-hidden
        >
          ·
        </span>
        <ConnectivityIndicators
          status={connectivity}
          onOpenSettings={onOpenConnectivitySettings}
          webSearch={webSearch}
          onWebSearchChange={onWebSearchChange}
        />
        {assistantChrome?.sessionTokenUsage ? (
          <>
            <span className="text-muted-foreground/60" aria-hidden>
              ·
            </span>
            <StatusBarTokenUsage
              sessionUsage={assistantChrome.sessionTokenUsage}
            />
          </>
        ) : null}
        <span className="text-muted-foreground/60" aria-hidden>
          ·
        </span>
        <span
          className="max-w-[14rem] truncate"
          title={statusTitle}
          role="status"
          aria-live="polite"
        >
          {statusLine}
        </span>
      </div>
    </footer>
  );
});
