import { memo } from "react";
import { Moon, Redo2, Sun, Undo2 } from "lucide-react";

import { ConnectivityIndicators } from "@/components/layout/ConnectivityIndicators";
import { EditorZoomControl } from "@/components/layout/EditorZoomControl";
import { StatusBarTokenUsage } from "@/components/layout/StatusBarTokenUsage";
import { Kbd } from "@/components/ui/kbd";
import { dispatchOpenAuditTrail } from "@/lib/audit-trail-events";
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
  onUndo?: () => void;
  onRedo?: () => void;
  canUndo?: boolean;
  canRedo?: boolean;
  webSearch?: boolean;
  onWebSearchChange?: (enabled: boolean) => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
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
  onUndo,
  onRedo,
  canUndo = false,
  canRedo = false,
  webSearch = false,
  onWebSearchChange,
  theme,
  onThemeChange,
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
      {onUndo && onRedo ? (
        <>
          <span className="shrink-0 text-muted-foreground/60" aria-hidden>
            ·
          </span>
          <div className="flex items-center gap-0.5">
            <button
              type="button"
              title="撤销 (⌘Z)"
              className="flex h-5 w-5 items-center justify-center rounded text-muted-foreground/60 hover:bg-muted hover:text-foreground disabled:opacity-30"
              onClick={onUndo}
              disabled={!canUndo}
            >
              <Undo2 className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              title="重做 (⌘⇧Z)"
              className="flex h-5 w-5 items-center justify-center rounded text-muted-foreground/60 hover:bg-muted hover:text-foreground disabled:opacity-30"
              onClick={onRedo}
              disabled={!canRedo}
            >
              <Redo2 className="h-3.5 w-3.5" />
            </button>
          </div>
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
        <button
          type="button"
          role="switch"
          aria-checked={theme === "dark"}
          aria-label={theme === "dark" ? "切换到亮色模式" : "切换到暗色模式"}
          title={theme === "dark" ? "切换到亮色模式" : "切换到暗色模式"}
          data-testid="status-bar-theme-switch"
          className="inline-flex h-6 shrink-0 items-center gap-1 rounded-sm border border-border/40 bg-surface-inset/30 px-1.5 text-muted-foreground transition-[background-color,color,transform] duration-base ease-iris-out hover:bg-muted/50 hover:text-foreground focus:outline-none focus-visible:ring-2 focus-visible:ring-primary focus-visible:ring-offset-1 focus-visible:ring-offset-panel active:scale-[0.98]"
          onClick={() => onThemeChange(theme === "dark" ? "light" : "dark")}
        >
          {theme === "dark" ? (
            <Moon className="h-3.5 w-3.5" />
          ) : (
            <Sun className="h-3.5 w-3.5" />
          )}
        </button>
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
        {assistantChrome?.harnessRequestId ? (
          <>
            <span className="text-muted-foreground/60" aria-hidden>
              ·
            </span>
            <button
              type="button"
              className="shrink-0 text-primary hover:underline"
              data-testid="status-bar-audit-link"
              onClick={dispatchOpenAuditTrail}
            >
              工具审计
            </button>
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
