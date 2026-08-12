//! 订阅来源与视图导航（阶段 4）。
//!
//! 五个文章视图 + 同步失败源诊断区；未读同时用字重、圆点与 aria-label，
//! 不能只用 brand 色。

import {
  AlertTriangle,
  Archive,
  Download,
  Inbox,
  Newspaper,
  Plus,
  Radio,
  RefreshCw,
  Rss,
  Star,
  Upload,
} from "lucide-react";

import { cn } from "@/lib/utils";
import type { FeedSourceSummary, FeedView } from "@/types/ipc";

const VIEWS: { view: FeedView; label: string; icon: typeof Inbox }[] = [
  { view: "inbox", label: "收件箱", icon: Inbox },
  { view: "today", label: "今日", icon: Newspaper },
  { view: "all", label: "全部", icon: Rss },
  { view: "starred", label: "收藏", icon: Star },
  { view: "archived", label: "归档", icon: Archive },
];

export interface FeedSidebarProps {
  sources: FeedSourceSummary[];
  view: FeedView;
  sourceId: string | null;
  onViewChange: (view: FeedView) => void;
  onSourceSelect: (sourceId: string) => void;
  onClearSource: () => void;
  onAddSource: () => void;
  /** 同步失败源的显式重试（稳定错误码文案由 aria-label 表达）。 */
  onRetrySource: (sourceId: string) => void;
  /** 打开 OPML 导入导出对话框。 */
  onOpenOpml: () => void;
}

export function FeedSidebar({
  sources,
  view,
  sourceId,
  onViewChange,
  onSourceSelect,
  onClearSource,
  onAddSource,
  onRetrySource,
  onOpenOpml,
}: FeedSidebarProps) {
  const failedSources = sources.filter(
    (source) => source.lastErrorCode != null,
  );
  const sourceGroups = new Map<string, FeedSourceSummary[]>();
  for (const source of sources) {
    const folder = source.folderPath.trim();
    const group = sourceGroups.get(folder) ?? [];
    group.push(source);
    sourceGroups.set(folder, group);
  }

  const renderSource = (source: FeedSourceSummary) => {
    const active = sourceId === source.id;
    const failed = source.lastErrorCode != null;
    return (
      <li key={source.id}>
        <button
          type="button"
          data-testid={`feed-source-${source.id}`}
          aria-pressed={active}
          aria-label={`${source.title}${failed ? "，同步失败" : ""}，${source.unreadCount} 条未读`}
          className={cn(
            "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground",
            active && "bg-muted/80 text-foreground",
          )}
          onClick={() => {
            onViewChange("all");
            onSourceSelect(source.id);
          }}
        >
          {failed ? (
            <AlertTriangle
              className="h-4 w-4 shrink-0 text-warning"
              aria-hidden="true"
            />
          ) : (
            <Rss className="h-4 w-4 shrink-0" aria-hidden="true" />
          )}
          <span className="min-w-0 flex-1 truncate text-left">
            {source.title}
          </span>
          {source.unreadCount > 0 ? (
            <span
              data-testid={`feed-source-unread-${source.id}`}
              className="shrink-0 rounded-full bg-muted px-1.5 text-micro tabular-nums text-muted-foreground"
            >
              {source.unreadCount}
            </span>
          ) : null}
        </button>
      </li>
    );
  };

  return (
    <nav
      data-testid="feed-sidebar"
      aria-label="订阅导航"
      className="flex h-full min-h-0 w-60 flex-col overflow-y-auto border-r border-border-subtle bg-panel p-2"
    >
      <div className="mb-1 flex items-center justify-between px-2">
        <span className="text-caption font-medium text-muted-foreground">
          订阅
        </span>
        <button
          type="button"
          data-testid="feed-add-source"
          aria-label="添加订阅"
          className="iris-focus-soft inline-flex h-6 w-6 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
          onClick={onAddSource}
        >
          <Plus className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>
      <ul className="space-y-0.5">
        {VIEWS.map(({ view: itemView, label, icon: Icon }) => {
          const active = view === itemView && sourceId === null;
          return (
            <li key={itemView}>
              <button
                type="button"
                data-testid={`feed-view-${itemView}`}
                aria-pressed={active}
                className={cn(
                  "flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground",
                  active && "bg-muted/80 text-foreground",
                )}
                onClick={() => {
                  onViewChange(itemView);
                  onClearSource();
                }}
              >
                <Icon className="h-4 w-4 shrink-0" aria-hidden="true" />
                <span className="truncate">{label}</span>
              </button>
            </li>
          );
        })}
      </ul>

      <div className="mb-1 mt-4 flex items-center gap-1 px-2 text-caption font-medium text-muted-foreground">
        <Radio className="h-3.5 w-3.5" aria-hidden="true" />
        订阅源
      </div>
      {[...sourceGroups.entries()].map(([folder, group]) => {
        const groupId = folder ? folder.replaceAll("/", "-") : "ungrouped";
        return (
          <div
            key={folder || "ungrouped"}
            data-testid={`feed-source-group-${groupId}`}
            className="mb-1"
          >
            {folder ? (
              <div className="truncate px-2 py-1 text-micro text-muted-foreground/70">
                {folder}
              </div>
            ) : null}
            <ul className="space-y-0.5">{group.map(renderSource)}</ul>
          </div>
        );
      })}

      {failedSources.length > 0 ? (
        <div className="mt-4">
          <div className="mb-1 px-2 text-caption font-medium text-muted-foreground">
            同步失败
          </div>
          <ul className="space-y-0.5">
            {failedSources.map((source) => (
              <li
                key={source.id}
                data-testid={`feed-failed-source-${source.id}`}
              >
                <div className="flex items-center gap-1 rounded-md px-2 py-1.5">
                  <button
                    type="button"
                    data-testid={`feed-failed-source-view-${source.id}`}
                    aria-label={`${source.title}：${source.lastErrorCode ?? "未知错误"}，点击查看`}
                    className="flex min-w-0 flex-1 items-center gap-2 rounded-md text-ui text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
                    onClick={() => {
                      onViewChange("all");
                      onSourceSelect(source.id);
                    }}
                  >
                    <AlertTriangle
                      className="h-4 w-4 shrink-0 text-warning"
                      aria-hidden="true"
                    />
                    <span className="min-w-0 flex-1 truncate text-left">
                      {source.title}
                    </span>
                  </button>
                  <button
                    type="button"
                    data-testid={`feed-retry-source-${source.id}`}
                    aria-label={`重试同步 ${source.title}`}
                    className="iris-focus-soft inline-flex h-6 w-6 shrink-0 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
                    onClick={() => onRetrySource(source.id)}
                  >
                    <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </div>
      ) : null}

      <div className="mt-auto flex items-center gap-1 border-t border-border-subtle pt-2">
        <button
          type="button"
          data-testid="feed-opml-import-entry"
          aria-label="导入 OPML"
          title="导入 OPML"
          className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
          onClick={onOpenOpml}
        >
          <Upload className="h-4 w-4" aria-hidden="true" />
        </button>
        <button
          type="button"
          data-testid="feed-opml-export-entry"
          aria-label="导出 OPML"
          title="导出 OPML"
          className="iris-focus-soft inline-flex h-7 w-7 items-center justify-center rounded-md text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
          onClick={onOpenOpml}
        >
          <Download className="h-4 w-4" aria-hidden="true" />
        </button>
      </div>
    </nav>
  );
}
