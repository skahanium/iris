//! 订阅来源与视图导航（阶段 4）。
//!
//! 五个文章视图 + 同步失败源诊断区；未读同时用字重、圆点与 aria-label，
//! 不能只用 brand 色。

import {
  Inbox,
  Newspaper,
  Rss,
  Star,
  Archive,
  AlertTriangle,
  Radio,
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
}

export function FeedSidebar({
  sources,
  view,
  sourceId,
  onViewChange,
  onSourceSelect,
  onClearSource,
}: FeedSidebarProps) {
  const failedSources = sources.filter(
    (source) => source.lastErrorCode != null,
  );

  return (
    <nav
      data-testid="feed-sidebar"
      aria-label="订阅导航"
      className="flex h-full min-h-0 w-60 flex-col overflow-y-auto border-r border-border-subtle bg-panel p-2"
    >
      <div className="mb-1 px-2 text-caption font-medium text-muted-foreground">
        订阅
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
      <ul className="space-y-0.5">
        {sources.map((source) => {
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
        })}
      </ul>

      {failedSources.length > 0 ? (
        <div className="mt-4">
          <div className="mb-1 px-2 text-caption font-medium text-muted-foreground">
            同步失败
          </div>
          <ul className="space-y-0.5">
            {failedSources.map((source) => (
              <li key={source.id}>
                <button
                  type="button"
                  data-testid={`feed-failed-source-${source.id}`}
                  aria-label={`${source.title}：${source.lastErrorCode ?? "未知错误"}，点击重试`}
                  className="flex w-full items-center gap-2 rounded-md px-2 py-1.5 text-ui text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
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
              </li>
            ))}
          </ul>
        </div>
      ) : null}
    </nav>
  );
}
