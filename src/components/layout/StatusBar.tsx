import { memo } from "react";
import { Moon, Network, Redo2, Settings, Sun, Undo2 } from "lucide-react";

import { ConnectivityIndicators } from "@/components/layout/ConnectivityIndicators";
import { EditorZoomControl } from "@/components/layout/EditorZoomControl";
import { StatusBarTokenUsage } from "@/components/layout/StatusBarTokenUsage";
import type { AssistantChromeSnapshot } from "@/types/assistant-chrome";
import type { FileLinkSummary } from "@/types/ipc";
import type { WebSearchAvailability } from "@/lib/web-search-provider-state";
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
  onEditorZoomChange?: (zoom: number) => void;
  onUndo?: () => void;
  onRedo?: () => void;
  canUndo?: boolean;
  canRedo?: boolean;
  webSearch?: boolean;
  webSearchAvailability?: WebSearchAvailability | null;
  onWebSearchChange?: (enabled: boolean) => void;
  theme: "dark" | "light";
  onThemeChange: (theme: "dark" | "light") => void;
  connectivity?: ConnectivityStatus | null;
  onOpenConnectivitySettings?: () => void;
  onOpenManagementCenter?: () => void;
  onOpenGraph?: () => void;
  /** AI 侧栏上报的 Token / 工具活动（见 UnifiedAssistantPanel） */
  assistantChrome?: AssistantChromeSnapshot | null;
  linkSummary?: FileLinkSummary | null;
  linkSummaryUnavailable?: boolean;
  onOpenKnowledgeRelations?: () => void;
}

function isClassifiedStatusLine(value: string | null | undefined) {
  if (!value) return false;
  return /涉密|保险库|classified/i.test(value);
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
  onEditorZoomChange,
  onUndo,
  onRedo,
  canUndo = false,
  canRedo = false,
  webSearch = false,
  webSearchAvailability = null,
  onWebSearchChange,
  theme,
  onThemeChange,
  connectivity = null,
  onOpenConnectivitySettings,
  onOpenManagementCenter,
  onOpenGraph,
  assistantChrome = null,
  linkSummary = null,
  linkSummaryUnavailable = false,
  onOpenKnowledgeRelations,
}: StatusBarProps) {
  const trimmedTitle = documentTitle?.trim();
  const label = trimmedTitle || (path ? "无标题" : "未打开文件");

  const toolLabel = assistantChrome?.toolActivityLabel?.trim() ?? null;
  const rawStatusLine = toolLabel || aiStatus;
  const safeStatusLine = isClassifiedStatusLine(rawStatusLine)
    ? ""
    : rawStatusLine;
  const statusTitle = [safeStatusLine].filter(Boolean).join(" · ") || undefined;

  return (
    <footer
      data-testid="status-bar"
      className="flex h-8 shrink-0 items-center gap-3 border-t border-border/60 bg-surface-chrome px-3 font-sans text-[11px] tracking-wide text-muted-foreground"
    >
      <span
        data-testid="status-bar-document-title"
        className="min-w-0 max-w-[min(18rem,32vw)] truncate"
        title={trimmedTitle || path || undefined}
      >
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
      {onEditorZoomIn &&
      onEditorZoomOut &&
      onEditorZoomReset &&
      onEditorZoomChange ? (
        <>
          <span className="shrink-0 text-muted-foreground/60" aria-hidden>
            ·
          </span>
          <EditorZoomControl
            editorZoom={editorZoom}
            onZoomIn={onEditorZoomIn}
            onZoomOut={onEditorZoomOut}
            onZoomReset={onEditorZoomReset}
            onZoomChange={onEditorZoomChange}
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
              className="iris-focus-soft flex h-5 w-5 items-center justify-center rounded text-muted-foreground/60 hover:bg-muted hover:text-foreground disabled:opacity-30"
              onClick={onUndo}
              disabled={!canUndo}
            >
              <Undo2 className="h-3.5 w-3.5" />
            </button>
            <button
              type="button"
              title="重做 (⌘⇧Z)"
              className="iris-focus-soft flex h-5 w-5 items-center justify-center rounded text-muted-foreground/60 hover:bg-muted hover:text-foreground disabled:opacity-30"
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
      {path &&
      onOpenKnowledgeRelations &&
      (linkSummary || linkSummaryUnavailable) ? (
        <>
          <span className="shrink-0 text-muted-foreground/60" aria-hidden>
            ·
          </span>
          <button
            type="button"
            data-testid="status-bar-link-summary"
            className="iris-focus-soft inline-flex h-5 shrink-0 items-center rounded-sm px-1.5 tabular-nums text-muted-foreground transition-[background-color,color,transform] duration-base ease-iris-out hover:bg-muted/50 hover:text-foreground focus:outline-none active:scale-[0.98]"
            title="打开知识关联"
            onClick={onOpenKnowledgeRelations}
          >
            {linkSummaryUnavailable
              ? "双链暂不可用"
              : `入链 ${linkSummary?.inboundCount ?? 0} · 出链 ${
                  linkSummary?.outboundCount ?? 0
                }`}
          </button>
        </>
      ) : null}
      <div className="ml-auto flex min-w-0 shrink-0 items-center gap-3">
        {onOpenManagementCenter ? (
          <>
            <button
              type="button"
              title="打开管理中心"
              aria-label="打开管理中心"
              data-testid="status-bar-management-button"
              className="iris-focus-soft inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm border border-border/40 bg-surface-inset/30 text-muted-foreground transition-[background-color,color,transform] duration-base ease-iris-out hover:bg-muted/50 hover:text-foreground focus:outline-none active:scale-[0.98]"
              onClick={onOpenManagementCenter}
            >
              <Settings className="h-3.5 w-3.5" />
            </button>
            <span
              className="hidden shrink-0 text-muted-foreground/60 sm:inline"
              aria-hidden
            >
              ·
            </span>
          </>
        ) : null}
        {onOpenGraph ? (
          <>
            <button
              type="button"
              title="打开知识图谱"
              aria-label="打开知识图谱"
              data-testid="status-bar-graph-button"
              className="iris-focus-soft inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-sm border border-border/40 bg-surface-inset/30 text-muted-foreground transition-[background-color,color,transform] duration-base ease-iris-out hover:bg-muted/50 hover:text-foreground focus:outline-none active:scale-[0.98]"
              onClick={onOpenGraph}
            >
              <Network className="h-3.5 w-3.5" />
            </button>
            <span
              className="hidden shrink-0 text-muted-foreground/60 sm:inline"
              aria-hidden
            >
              ·
            </span>
          </>
        ) : null}
        <ConnectivityIndicators
          status={connectivity}
          onOpenSettings={onOpenConnectivitySettings}
          webSearch={webSearch}
          webSearchAvailability={webSearchAvailability}
          onWebSearchChange={onWebSearchChange}
        />
        <button
          type="button"
          role="switch"
          aria-checked={theme === "dark"}
          aria-label={theme === "dark" ? "切换到亮色模式" : "切换到暗色模式"}
          title={theme === "dark" ? "切换到亮色模式" : "切换到暗色模式"}
          data-testid="status-bar-theme-switch"
          className="iris-focus-soft inline-flex h-6 shrink-0 items-center gap-1 rounded-sm border border-border/40 bg-surface-inset/30 px-1.5 text-muted-foreground transition-[background-color,color,transform] duration-base ease-iris-out hover:bg-muted/50 hover:text-foreground focus:outline-none active:scale-[0.98]"
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
        {safeStatusLine ? (
          <>
            <span className="text-muted-foreground/60" aria-hidden>
              ·
            </span>
            <span
              className="max-w-[14rem] truncate"
              title={statusTitle}
              role="status"
              aria-live="polite"
            >
              {safeStatusLine}
            </span>
          </>
        ) : null}
      </div>
    </footer>
  );
});
