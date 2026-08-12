//! 订阅工作区（阶段 4）：导航/列表/阅读三区 + 响应式状态机 + 快捷键。
//!
//! 布局（规范 §10.2）：`>=1366` 来源导航可折叠；`1024–1365` 抽屉；
//! `800–1023` 列表/阅读单平面切换。快捷键 j/k/o/Enter/m/s/e/r，
//! 输入框聚焦时不触发。

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Menu, Pencil, Plus, RefreshCw, Search, Upload, X } from "lucide-react";

import { FeedItemList } from "@/components/feed/FeedItemList";
import { FeedOpmlDialog } from "@/components/feed/FeedOpmlDialog";
import { FeedReader } from "@/components/feed/FeedReader";
import { FeedSidebar } from "@/components/feed/FeedSidebar";
import {
  FeedSourceDialog,
  type FeedSourceDialogMode,
} from "@/components/feed/FeedSourceDialog";
import { useFeedLibrary } from "@/hooks/useFeedLibrary";
import { feedItemGet, feedSyncAll, feedSyncSource } from "@/lib/ipc";
import { cn } from "@/lib/utils";
import type { FeedItemDetail, FeedSourceSummary, FeedView } from "@/types/ipc";

export type FeedBreakpoint = "wide" | "mid" | "narrow";

export interface FeedWorkspaceProps {
  /** 工作区当前是否可见；不可见时暂停焦点与阅读副作用。 */
  active?: boolean;
  /** 保存为笔记（App 层执行 fileCreate + 打开）；缺省时不显示入口。 */
  onSaveAsNote?: (
    markdown: string,
    titleHint: string,
    folderPath: string,
  ) => Promise<string>;
}

function useFeedBreakpoints(container: HTMLDivElement | null): FeedBreakpoint {
  const [width, setWidth] = useState(() => window.innerWidth);
  useEffect(() => {
    if (!container) return;
    const refresh = () => {
      const measured = container.getBoundingClientRect().width;
      setWidth(measured > 0 ? Math.round(measured) : window.innerWidth);
    };
    refresh();
    if (typeof ResizeObserver === "undefined") {
      window.addEventListener("resize", refresh);
      return () => window.removeEventListener("resize", refresh);
    }
    const observer = new ResizeObserver((entries) => {
      const entry = entries.find((candidate) => candidate.target === container);
      if (entry) setWidth(Math.round(entry.contentRect.width));
    });
    observer.observe(container);
    return () => observer.disconnect();
  }, [container]);
  if (width >= 1366) return "wide";
  if (width >= 1024) return "mid";
  return "narrow";
}

/** 搜索框：200ms debounce、输入法 composition 中不发请求、Escape 清空。 */
function FeedSearchBox({
  value,
  onChange,
}: {
  value: string;
  onChange: (value: string) => void;
}) {
  const [draft, setDraft] = useState(value);
  const composingRef = useRef(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    setDraft(value);
  }, [value]);

  useEffect(
    () => () => {
      if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    },
    [],
  );

  const schedule = (next: string) => {
    if (timerRef.current !== null) window.clearTimeout(timerRef.current);
    timerRef.current = window.setTimeout(() => {
      if (!composingRef.current) onChange(next);
    }, 200);
  };

  return (
    <div className="relative px-2 py-1.5">
      <Search
        className="pointer-events-none absolute left-4 top-1/2 h-3.5 w-3.5 -translate-y-1/2 text-muted-foreground"
        aria-hidden="true"
      />
      <input
        data-testid="feed-search-input"
        type="search"
        className="iris-focus-soft w-full rounded-md border border-border-subtle bg-background py-1 pl-7 pr-2 text-ui outline-none placeholder:text-muted-foreground"
        placeholder="搜索订阅文章"
        value={draft}
        aria-label="搜索订阅文章"
        onChange={(event) => {
          const next = event.target.value;
          setDraft(next); // 受控显示立即跟随输入
          schedule(next); // 父级查询走 200ms debounce
        }}
        onCompositionStart={() => {
          composingRef.current = true;
        }}
        onCompositionEnd={(event) => {
          composingRef.current = false;
          const next = (event.target as HTMLInputElement).value;
          setDraft(next);
          schedule(next);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            if (timerRef.current !== null)
              window.clearTimeout(timerRef.current);
            setDraft("");
            onChange("");
          }
        }}
      />
    </div>
  );
}

function isEditableTarget(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.tagName === "INPUT" ||
    target.tagName === "TEXTAREA" ||
    target.isContentEditable
  );
}

export function FeedWorkspace({
  active = true,
  onSaveAsNote,
}: FeedWorkspaceProps) {
  const library = useFeedLibrary();
  const [workspaceElement, setWorkspaceElement] =
    useState<HTMLDivElement | null>(null);
  const breakpoint = useFeedBreakpoints(workspaceElement);
  const [sidebarOpen, setSidebarOpen] = useState(false);
  const [narrowPlane, setNarrowPlane] = useState<"list" | "reader">("list");
  const [detail, setDetail] = useState<FeedItemDetail | null>(null);
  const [detailStatus, setDetailStatus] = useState<
    "idle" | "loading" | "ready" | "error"
  >("idle");
  const [detailError, setDetailError] = useState<string | null>(null);
  const listContainerRef = useRef<HTMLDivElement>(null);
  const detailSequenceRef = useRef(0);
  const [dialogOpen, setDialogOpen] = useState(false);
  const [dialogMode, setDialogMode] = useState<FeedSourceDialogMode>("add");
  const [dialogSource, setDialogSource] = useState<FeedSourceSummary | null>(
    null,
  );
  const [opmlOpen, setOpmlOpen] = useState(false);
  const [detailRequestEpoch, setDetailRequestEpoch] = useState(0);
  const [focusedItemId, setFocusedItemId] = useState<string | null>(null);
  const [syncing, setSyncing] = useState(false);
  const [syncAnnouncement, setSyncAnnouncement] = useState("");

  useEffect(() => {
    if (library.items.length === 0) {
      setFocusedItemId(null);
      return;
    }
    if (!library.items.some((item) => item.id === focusedItemId)) {
      setFocusedItemId(library.items[0]?.id ?? null);
    }
  }, [focusedItemId, library.items]);

  // 选中文章 → 加载详情（迟到响应丢弃）。
  useEffect(() => {
    const itemId = library.selectedItemId;
    if (!itemId) {
      detailSequenceRef.current += 1;
      setDetail(null);
      setDetailStatus("idle");
      return;
    }
    const sequence = ++detailSequenceRef.current;
    setDetailStatus("loading");
    setDetailError(null);
    void feedItemGet(itemId)
      .then((loaded) => {
        if (detailSequenceRef.current !== sequence) return;
        setDetail(loaded);
        setDetailStatus("ready");
      })
      .catch((error: unknown) => {
        if (detailSequenceRef.current !== sequence) return;
        setDetailStatus("error");
        setDetailError(
          (error as { code?: string })?.code ?? "feed_item_not_found",
        );
      });
  }, [detailRequestEpoch, library.selectedItemId]);

  const handleSelectItem = useCallback(
    (itemId: string) => {
      library.selectItem(itemId);
      if (breakpoint === "narrow") setNarrowPlane("reader");
    },
    [breakpoint, library],
  );

  const handleBackToList = useCallback(() => {
    setNarrowPlane("list");
    window.requestAnimationFrame(() => {
      const target = focusedItemId
        ? listContainerRef.current?.querySelector<HTMLElement>(
            `[data-testid="feed-item-${focusedItemId}"]`,
          )
        : null;
      target?.focus();
    });
  }, [focusedItemId]);

  const selectedIndex = useMemo(
    () => library.items.findIndex((item) => item.id === focusedItemId),
    [focusedItemId, library.items],
  );

  const runSync = useCallback(async () => {
    if (syncing) return;
    setSyncing(true);
    setSyncAnnouncement("正在同步订阅…");
    try {
      if (library.sourceId) {
        const outcome = await feedSyncSource(library.sourceId, true);
        setSyncAnnouncement(
          outcome.status === "failed"
            ? `同步失败：${outcome.errorCode ?? "feed_sync_failed"}`
            : `同步完成，新增 ${outcome.newItems} 篇文章。`,
        );
      } else {
        const outcome = await feedSyncAll();
        setSyncAnnouncement(
          `同步完成：成功 ${outcome.succeeded}，失败 ${outcome.failed}，新增 ${outcome.newItems} 篇。`,
        );
      }
      library.refresh();
    } catch {
      setSyncAnnouncement("同步失败，请检查网络后重试。");
    } finally {
      setSyncing(false);
    }
  }, [library, syncing]);

  // 快捷键：j/k 移动、o/Enter 打开、m 已读、s 收藏、e 归档、r 刷新。
  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent) => {
      if (!active) return;
      if (isEditableTarget(event.target)) return;
      const items = library.items;
      if (event.key === "r") {
        event.preventDefault();
        void runSync();
        return;
      }
      if (items.length === 0) return;
      const move = (delta: number) => {
        event.preventDefault();
        const nextIndex = Math.min(
          Math.max(selectedIndex + delta, 0),
          items.length - 1,
        );
        const nextItem = items[nextIndex];
        if (nextItem) {
          setFocusedItemId(nextItem.id);
          window.requestAnimationFrame(() => {
            listContainerRef.current
              ?.querySelector<HTMLElement>(
                `[data-testid="feed-item-${nextItem.id}"]`,
              )
              ?.focus();
          });
        }
      };
      switch (event.key) {
        case "j":
          move(1);
          break;
        case "k":
          move(-1);
          break;
        case "o":
        case "Enter":
          if (selectedIndex >= 0) {
            event.preventDefault();
            const item = items[selectedIndex];
            if (item) handleSelectItem(item.id);
          }
          break;
        case "m": {
          const item = items[selectedIndex];
          if (!item) break;
          event.preventDefault();
          library.setItemState(item.id, { isRead: !item.isRead });
          break;
        }
        case "s": {
          const item = items[selectedIndex];
          if (!item) break;
          event.preventDefault();
          library.setItemState(item.id, { isStarred: !item.isStarred });
          break;
        }
        case "e": {
          const item = items[selectedIndex];
          if (!item) break;
          event.preventDefault();
          library.setItemState(item.id, { isArchived: !item.isArchived });
          break;
        }
        default:
          break;
      }
    },
    [active, handleSelectItem, library, runSync, selectedIndex],
  );

  const openAddDialog = useCallback(() => {
    setDialogMode("add");
    setDialogSource(null);
    setDialogOpen(true);
  }, []);

  const openEditDialog = useCallback((source: FeedSourceSummary) => {
    setDialogMode("edit");
    setDialogSource(source);
    setDialogOpen(true);
  }, []);

  const sidebarNode = (
    <FeedSidebar
      sources={library.sources}
      view={library.view}
      sourceId={library.sourceId}
      onViewChange={(view: FeedView) => library.setView(view)}
      onSourceSelect={library.setSourceId}
      onClearSource={() => library.setSourceId(null)}
      onAddSource={openAddDialog}
      onRetrySource={(sourceId) => {
        void feedSyncSource(sourceId, true).then(() => library.refresh());
      }}
      onOpenOpml={() => setOpmlOpen(true)}
    />
  );

  const listNode = (
    <FeedItemList
      items={library.items}
      status={library.status}
      errorCode={library.errorCode}
      hasMore={library.hasMore}
      selectedItemId={library.selectedItemId}
      focusedItemId={focusedItemId}
      onSelect={handleSelectItem}
      onFocusItem={setFocusedItemId}
      onMarkAllRead={() => void library.markAllRead()}
      onRetry={library.refresh}
      onLoadMore={library.loadMore}
      listContainerRef={listContainerRef}
    />
  );

  const selectedSource = library.sources.find(
    (source) => source.id === library.sourceId,
  );

  const syncButton = (
    <button
      type="button"
      data-testid="feed-sync-now"
      aria-label={library.sourceId ? "刷新当前订阅源" : "刷新全部订阅源"}
      className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
      disabled={syncing}
      onClick={() => void runSync()}
    >
      <RefreshCw
        className={cn("h-4 w-4", syncing && "animate-spin")}
        aria-hidden="true"
      />
    </button>
  );

  const readerNode = (
    <FeedReader
      detail={detail}
      status={detailStatus}
      errorCode={detailError}
      active={active}
      onRetry={() => setDetailRequestEpoch((epoch) => epoch + 1)}
      setItemState={(itemId, patch) => {
        setDetail((current) =>
          current?.summary.id === itemId
            ? {
                ...current,
                summary: {
                  ...current.summary,
                  isRead: patch.isRead ?? current.summary.isRead,
                  isStarred: patch.isStarred ?? current.summary.isStarred,
                  isArchived: patch.isArchived ?? current.summary.isArchived,
                },
              }
            : current,
        );
        void library.setItemState(itemId, patch).then((succeeded) => {
          if (!succeeded) {
            setDetailRequestEpoch((epoch) => epoch + 1);
          }
        });
      }}
      onSaveAsNote={onSaveAsNote}
    />
  );

  if (breakpoint === "narrow") {
    return (
      <div
        ref={setWorkspaceElement}
        data-testid="feed-workspace"
        className="relative flex h-full min-h-0 flex-1 flex-col bg-background"
        onKeyDown={handleKeyDown}
      >
        <div className="flex h-10 shrink-0 items-center gap-1 border-b border-border-subtle px-2">
          {narrowPlane === "reader" ? (
            <button
              type="button"
              data-testid="feed-back-to-list"
              aria-label="返回列表"
              className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
              onClick={handleBackToList}
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </button>
          ) : null}
          {narrowPlane === "list" ? (
            <button
              type="button"
              data-testid="feed-open-drawer"
              aria-label="打开来源与视图"
              className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
              onClick={() => setSidebarOpen(true)}
            >
              <Menu className="h-4 w-4" aria-hidden="true" />
            </button>
          ) : null}
          <span className="text-caption font-medium text-muted-foreground">
            {narrowPlane === "reader" ? "文章" : "订阅"}
          </span>
          {narrowPlane === "list" ? (
            <div className="ml-auto flex items-center gap-1">
              {syncButton}
              {selectedSource ? (
                <button
                  type="button"
                  data-testid="feed-edit-source"
                  aria-label="编辑当前订阅源"
                  className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
                  onClick={() => openEditDialog(selectedSource)}
                >
                  <Pencil className="h-4 w-4" aria-hidden="true" />
                </button>
              ) : null}
              <button
                type="button"
                data-testid="feed-add-source"
                aria-label="添加订阅源"
                className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
                onClick={openAddDialog}
              >
                <Plus className="h-4 w-4" aria-hidden="true" />
              </button>
              <button
                type="button"
                data-testid="feed-open-opml"
                aria-label="导入或导出 OPML"
                className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
                onClick={() => setOpmlOpen(true)}
              >
                <Upload className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>
          ) : null}
        </div>
        {narrowPlane === "list" ? (
          <FeedSearchBox value={library.search} onChange={library.setSearch} />
        ) : null}
        <div className="flex min-h-0 flex-1">
          {narrowPlane === "reader" ? readerNode : listNode}
        </div>
        {sidebarOpen ? (
          <div
            data-testid="feed-drawer"
            className="absolute inset-y-0 left-0 z-20 flex shadow-overlay"
          >
            {sidebarNode}
            <button
              type="button"
              data-testid="feed-drawer-close"
              aria-label="关闭来源抽屉"
              className="flex w-8 items-center justify-center border-r border-border-subtle bg-panel text-muted-foreground"
              onClick={() => setSidebarOpen(false)}
            >
              <X className="h-4 w-4" aria-hidden="true" />
            </button>
          </div>
        ) : null}
        <div role="status" aria-live="polite" className="sr-only">
          {syncAnnouncement}
        </div>
        <FeedSourceDialog
          open={dialogOpen}
          mode={dialogMode}
          source={dialogSource}
          onOpenChange={setDialogOpen}
          onSourcesChanged={library.refresh}
        />
        <FeedOpmlDialog
          open={opmlOpen}
          onOpenChange={setOpmlOpen}
          onSourcesChanged={library.refresh}
        />
      </div>
    );
  }

  return (
    <div
      ref={setWorkspaceElement}
      data-testid="feed-workspace"
      className="relative flex h-full min-h-0 flex-1 bg-background"
      onKeyDown={handleKeyDown}
    >
      {breakpoint === "wide" ? (
        <>
          {sidebarOpen ? (
            <div className="relative z-10 h-full shrink-0">{sidebarNode}</div>
          ) : null}
          <div className="flex min-w-0 flex-1">
            <div className="flex w-80 shrink-0 flex-col border-r border-border-subtle">
              <div className="flex h-10 shrink-0 items-center justify-between border-b border-border-subtle px-2">
                <button
                  type="button"
                  data-testid="feed-toggle-sidebar"
                  aria-pressed={sidebarOpen}
                  aria-label={sidebarOpen ? "收起来源导航" : "展开来源导航"}
                  className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
                  onClick={() => setSidebarOpen((open) => !open)}
                >
                  <Menu className="h-4 w-4" aria-hidden="true" />
                </button>
                {syncButton}
                {library.sourceId ? (
                  <button
                    type="button"
                    data-testid="feed-edit-source"
                    aria-label="编辑当前订阅源"
                    className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground hover:bg-muted/60"
                    onClick={() => {
                      const selected = library.sources.find(
                        (item) => item.id === library.sourceId,
                      );
                      if (selected) openEditDialog(selected);
                    }}
                  >
                    <Pencil className="h-4 w-4" aria-hidden="true" />
                  </button>
                ) : null}
              </div>
              <FeedSearchBox
                value={library.search}
                onChange={library.setSearch}
              />
              {listNode}
            </div>
            <div className="min-w-0 flex-1 border-l border-border-subtle">
              {readerNode}
            </div>
          </div>
        </>
      ) : (
        <>
          <div className="flex min-w-0 flex-1">
            <div className="flex w-80 shrink-0 flex-col border-r border-border-subtle">
              <FeedSearchBox
                value={library.search}
                onChange={library.setSearch}
              />
              {listNode}
            </div>
            <div className="min-w-0 flex-1 border-l border-border-subtle">
              {readerNode}
            </div>
          </div>
          {sidebarOpen ? (
            <div
              data-testid="feed-drawer"
              className="absolute inset-y-0 left-0 z-20 flex shadow-overlay"
            >
              {sidebarNode}
              <button
                type="button"
                data-testid="feed-drawer-close"
                aria-label="关闭来源抽屉"
                className="flex w-8 items-center justify-center border-r border-border-subtle bg-panel text-muted-foreground hover:text-foreground"
                onClick={() => setSidebarOpen(false)}
              >
                <X className="h-4 w-4" aria-hidden="true" />
              </button>
            </div>
          ) : null}
          <button
            type="button"
            data-testid="feed-open-drawer"
            aria-label="打开来源抽屉"
            className={cn(
              "absolute left-2 top-12 z-10 inline-flex h-7 w-7 items-center justify-center rounded-md bg-panel text-muted-foreground shadow-overlay hover:text-foreground",
              sidebarOpen && "hidden",
            )}
            onClick={() => setSidebarOpen(true)}
          >
            <Menu className="h-4 w-4" aria-hidden="true" />
          </button>
          {selectedSource ? (
            <button
              type="button"
              data-testid="feed-edit-source"
              aria-label="编辑当前订阅源"
              className="absolute left-11 top-12 z-10 inline-flex h-7 w-7 items-center justify-center rounded-md bg-panel text-muted-foreground shadow-overlay hover:text-foreground"
              onClick={() => openEditDialog(selectedSource)}
            >
              <Pencil className="h-4 w-4" aria-hidden="true" />
            </button>
          ) : null}
        </>
      )}
      <FeedSourceDialog
        open={dialogOpen}
        mode={dialogMode}
        source={dialogSource}
        onOpenChange={setDialogOpen}
        onSourcesChanged={library.refresh}
      />
      <FeedOpmlDialog
        open={opmlOpen}
        onOpenChange={setOpmlOpen}
        onSourcesChanged={library.refresh}
      />
      <div role="status" aria-live="polite" className="sr-only">
        {syncAnnouncement}
      </div>
    </div>
  );
}
