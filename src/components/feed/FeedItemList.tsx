//! 订阅文章列表（阶段 4）。
//!
//! 复用 `@tanstack/react-virtual`，稳定 key 为 item ID；未读同时用字重、
//! 圆点与 aria-label；空态/loading/error 均有明确可读状态。

import { useRef } from "react";
import { useVirtualizer } from "@tanstack/react-virtual";
import { CheckCheck, Loader2, RefreshCw, TriangleAlert } from "lucide-react";

import { cn } from "@/lib/utils";
import type { FeedItemSummary } from "@/types/ipc";

export interface FeedItemListProps {
  items: FeedItemSummary[];
  status: "idle" | "loading" | "ready" | "error";
  errorCode: string | null;
  hasMore: boolean;
  selectedItemId: string | null;
  focusedItemId: string | null;
  onSelect: (itemId: string) => void;
  onFocusItem: (itemId: string) => void;
  onMarkAllRead: () => void;
  onRetry: () => void;
  onLoadMore: () => void;
  /** 供外部（键盘滚动/阅读动作）触发延迟已读的列表容器。 */
  listContainerRef?: React.RefObject<HTMLDivElement | null>;
}

function ItemRow({
  item,
  selected,
  focused,
  onSelect,
  onFocusItem,
}: {
  item: FeedItemSummary;
  selected: boolean;
  focused: boolean;
  onSelect: (itemId: string) => void;
  onFocusItem: (itemId: string) => void;
}) {
  return (
    <button
      type="button"
      data-testid={`feed-item-${item.id}`}
      data-selected={selected || undefined}
      aria-label={`${item.isRead ? "已读" : "未读"}：${item.title}`}
      aria-current={selected ? "true" : undefined}
      tabIndex={focused ? 0 : -1}
      className={cn(
        "block w-full border-b border-border-subtle/60 px-3 py-2 text-left transition-colors duration-fast hover:bg-muted/40 focus:outline-none",
        selected && "bg-muted/70",
      )}
      onClick={() => onSelect(item.id)}
      onFocus={() => onFocusItem(item.id)}
    >
      <div className="flex items-start gap-2">
        <span
          aria-hidden="true"
          className={cn(
            "mt-1.5 h-1.5 w-1.5 shrink-0 rounded-full",
            item.isRead ? "bg-transparent" : "bg-brand",
          )}
        />
        <span className="min-w-0 flex-1">
          <span
            className={cn(
              "block truncate text-ui",
              !item.isRead && "font-medium text-foreground",
            )}
          >
            {item.title}
          </span>
          <span className="mt-0.5 block truncate text-caption text-muted-foreground">
            {item.sourceTitle}
            {item.authorName ? ` · ${item.authorName}` : ""}
          </span>
          <span
            data-testid={`feed-item-excerpt-${item.id}`}
            className="mt-1 line-clamp-2 block text-caption text-muted-foreground/80"
          >
            {item.excerpt}
          </span>
        </span>
      </div>
    </button>
  );
}

export function FeedItemList({
  items,
  status,
  hasMore,
  selectedItemId,
  focusedItemId,
  onSelect,
  onFocusItem,
  onMarkAllRead,
  onRetry,
  onLoadMore,
  listContainerRef,
}: FeedItemListProps) {
  const fallbackRef = useRef<HTMLDivElement>(null);
  const scrollRef = listContainerRef ?? fallbackRef;
  const virtualizer = useVirtualizer({
    count: items.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 72,
    overscan: 12,
  });
  const virtualRows = virtualizer.getVirtualItems();

  const header = (
    <div className="flex h-10 shrink-0 items-center justify-between border-b border-border-subtle px-3">
      <span className="text-caption font-medium text-muted-foreground">
        {status === "loading" ? "加载中…" : `已显示 ${items.length} 篇`}
      </span>
      <button
        type="button"
        data-testid="feed-mark-all-read"
        className="iris-focus-soft inline-flex items-center gap-1 rounded-md px-2 py-1 text-caption text-muted-foreground transition-colors duration-fast hover:bg-muted/60 hover:text-foreground"
        onClick={onMarkAllRead}
        disabled={items.length === 0 || status !== "ready"}
      >
        <CheckCheck className="h-3.5 w-3.5" aria-hidden="true" />
        全部已读
      </button>
    </div>
  );

  if (status === "loading" && items.length === 0) {
    return (
      <div
        data-testid="feed-list"
        className="flex h-full min-h-0 flex-1 flex-col bg-background"
      >
        {header}
        <div className="flex flex-1 items-center justify-center gap-2 text-caption text-muted-foreground">
          <Loader2 className="h-4 w-4 animate-spin" aria-hidden="true" />
          正在加载订阅内容…
        </div>
      </div>
    );
  }

  if (status === "error" && items.length === 0) {
    return (
      <div
        data-testid="feed-list"
        className="flex h-full min-h-0 flex-1 flex-col bg-background"
      >
        {header}
        <div
          data-testid="feed-list-error"
          className="flex flex-1 flex-col items-center justify-center gap-2 px-6 text-center"
        >
          <TriangleAlert className="h-5 w-5 text-warning" aria-hidden="true" />
          <p className="text-ui text-muted-foreground">订阅内容加载失败</p>
          <p className="text-caption text-muted-foreground/70">
            请检查网络或稍后重试。
          </p>
          <button
            type="button"
            data-testid="feed-list-retry"
            className="iris-focus-soft inline-flex items-center gap-1 rounded-md border border-border-subtle px-3 py-1 text-caption transition-colors duration-fast hover:bg-muted/60"
            onClick={onRetry}
          >
            <RefreshCw className="h-3.5 w-3.5" aria-hidden="true" />
            重试
          </button>
        </div>
      </div>
    );
  }

  if (items.length === 0) {
    return (
      <div
        data-testid="feed-list"
        className="flex h-full min-h-0 flex-1 flex-col bg-background"
      >
        {header}
        <div
          data-testid="feed-list-empty"
          className="flex flex-1 items-center justify-center px-6 text-center text-caption text-muted-foreground"
        >
          没有订阅内容。添加订阅源或稍后同步。
        </div>
      </div>
    );
  }

  return (
    <div
      data-testid="feed-list"
      className="flex h-full min-h-0 flex-1 flex-col bg-background"
    >
      {header}
      <div ref={scrollRef} className="min-h-0 flex-1 overflow-y-auto">
        {virtualRows.length > 0 ? (
          <div
            style={{ height: virtualizer.getTotalSize(), position: "relative" }}
          >
            {virtualRows.map((virtualRow) => {
              const item = items[virtualRow.index];
              if (!item) return null;
              return (
                <div
                  key={item.id}
                  data-index={virtualRow.index}
                  ref={virtualizer.measureElement}
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    transform: `translateY(${virtualRow.start}px)`,
                  }}
                >
                  <ItemRow
                    item={item}
                    selected={selectedItemId === item.id}
                    focused={focusedItemId === item.id}
                    onSelect={onSelect}
                    onFocusItem={onFocusItem}
                  />
                </div>
              );
            })}
          </div>
        ) : (
          // 零高度容器（如 jsdom/窄视口）回退为普通行，保证内容可读。
          <div>
            {items.map((item) => (
              <ItemRow
                key={item.id}
                item={item}
                selected={selectedItemId === item.id}
                focused={focusedItemId === item.id}
                onSelect={onSelect}
                onFocusItem={onFocusItem}
              />
            ))}
          </div>
        )}
        {hasMore ? (
          <button
            type="button"
            data-testid="feed-load-more"
            className="block w-full px-3 py-2 text-center text-caption text-muted-foreground transition-colors duration-fast hover:bg-muted/40"
            onClick={onLoadMore}
          >
            继续显示已保存文章
          </button>
        ) : null}
      </div>
    </div>
  );
}
