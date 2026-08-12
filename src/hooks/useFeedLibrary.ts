//! 订阅资料库前端状态层（阶段 4）。
//!
//! 保存 `view/sourceId/search/selectedItemId/page/status`；每次筛选变化
//! 递增 request epoch，迟到响应不得覆盖新视图；`feed:changed` 事件只触发
//! 重新查询；状态操作失败回滚乐观更新。文章正文永不写入 localStorage。

import { useCallback, useEffect, useRef, useState } from "react";

import {
  feedItemList,
  feedItemsMarkRead,
  feedItemSetState,
  feedSearch,
  feedSourceList,
  listenFeedChanged,
} from "@/lib/ipc";
import type {
  FeedItemQuery,
  FeedItemStatePatch,
  FeedItemSummary,
  FeedSourceSummary,
  FeedView,
} from "@/types/ipc";

export type FeedLibraryStatus = "idle" | "loading" | "ready" | "error";

export const FEED_PAGE_LIMIT = 50;

export interface FeedLibraryApi {
  view: FeedView;
  sourceId: string | null;
  search: string;
  selectedItemId: string | null;
  page: number;
  status: FeedLibraryStatus;
  errorCode: string | null;
  items: FeedItemSummary[];
  sources: FeedSourceSummary[];
  setView: (view: FeedView) => void;
  setSourceId: (sourceId: string | null) => void;
  setSearch: (search: string) => void;
  selectItem: (itemId: string | null) => void;
  setItemState: (itemId: string, patch: FeedItemStatePatch) => void;
  markAllRead: () => Promise<number>;
  loadMore: () => void;
  refresh: () => void;
}

function errorCodeOf(error: unknown): string {
  return (error as { code?: string })?.code ?? "feed_unknown_error";
}

export function useFeedLibrary(): FeedLibraryApi {
  const [view, setViewState] = useState<FeedView>("inbox");
  const [sourceId, setSourceIdState] = useState<string | null>(null);
  const [search, setSearchState] = useState("");
  const [selectedItemId, setSelectedItemId] = useState<string | null>(null);
  const [page, setPage] = useState(1);
  const [status, setStatus] = useState<FeedLibraryStatus>("idle");
  const [errorCode, setErrorCode] = useState<string | null>(null);
  const [items, setItems] = useState<FeedItemSummary[]>([]);
  const [sources, setSources] = useState<FeedSourceSummary[]>([]);
  const requestSequenceRef = useRef(0);
  const filtersRef = useRef({ view, sourceId, search });
  filtersRef.current = { view, sourceId, search };

  const fetchRows = useCallback(
    (
      sequence: number,
      nextView: FeedView,
      nextSourceId: string | null,
      nextSearch: string,
    ) => {
      setStatus("loading");
      setErrorCode(null);
      const trimmed = nextSearch.trim();
      const run = trimmed
        ? feedSearch(trimmed, nextSourceId, FEED_PAGE_LIMIT)
        : feedItemList({
            view: nextView,
            sourceId: nextSourceId,
            receivedAfter: null,
            cursor: null,
            limit: FEED_PAGE_LIMIT,
          });
      void run
        .then((rows) => {
          if (requestSequenceRef.current !== sequence) return;
          setItems(rows);
          setPage(1);
          setStatus("ready");
        })
        .catch((error: unknown) => {
          if (requestSequenceRef.current !== sequence) return;
          setStatus("error");
          setErrorCode(errorCodeOf(error));
        });
    },
    [],
  );

  const refresh = useCallback(() => {
    const {
      view: currentView,
      sourceId: currentSource,
      search: currentSearch,
    } = filtersRef.current;
    fetchRows(
      ++requestSequenceRef.current,
      currentView,
      currentSource,
      currentSearch,
    );
  }, [fetchRows]);

  const loadSources = useCallback(async () => {
    try {
      const rows = await feedSourceList();
      setSources(rows);
    } catch {
      // 源列表失败静默：下一次事件/刷新会重试。
    }
  }, []);

  // 每次筛选变化递增 epoch 并重新查询；迟到响应被序号守卫丢弃。
  useEffect(() => {
    fetchRows(++requestSequenceRef.current, view, sourceId, search);
  }, [fetchRows, view, sourceId, search]);

  useEffect(() => {
    void loadSources();
  }, [loadSources]);

  // 同步事件只提示重新查询：刷新源列表与当前视图。
  useEffect(() => {
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listenFeedChanged(() => {
      if (disposed) return;
      void loadSources();
      refresh();
    }).then((dispose) => {
      if (disposed) dispose();
      else unlisten = dispose;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [loadSources, refresh]);

  const setView = useCallback((nextView: FeedView) => {
    setViewState(nextView);
    setSelectedItemId(null);
  }, []);

  const setSourceId = useCallback((nextSourceId: string | null) => {
    setSourceIdState(nextSourceId);
    setSelectedItemId(null);
  }, []);

  const setSearch = useCallback((nextSearch: string) => {
    setSearchState(nextSearch);
  }, []);

  const setItemState = useCallback(
    (itemId: string, patch: FeedItemStatePatch) => {
      const previous = items;
      setItems((rows) =>
        rows.map((row) =>
          row.id === itemId
            ? {
                ...row,
                isRead: patch.isRead ?? row.isRead,
                isStarred: patch.isStarred ?? row.isStarred,
                isArchived: patch.isArchived ?? row.isArchived,
              }
            : row,
        ),
      );
      void feedItemSetState(itemId, patch).catch(() => {
        // 状态失败回滚：恢复操作前的行快照。
        setItems(previous);
      });
    },
    [items],
  );

  const markAllRead = useCallback(async (): Promise<number> => {
    const { view: currentView, sourceId: currentSource } = filtersRef.current;
    const query: FeedItemQuery = {
      view: currentView,
      sourceId: currentSource,
      receivedAfter: null,
      cursor: null,
      limit: FEED_PAGE_LIMIT,
    };
    const affected = await feedItemsMarkRead(query);
    refresh();
    return affected;
  }, [refresh]);

  const loadMore = useCallback(() => {
    if (status !== "ready") return;
    const last = items[items.length - 1];
    if (!last) return;
    const {
      view: currentView,
      sourceId: currentSource,
      search: currentSearch,
    } = filtersRef.current;
    if (currentSearch.trim()) return; // 搜索暂不分页
    const sequence = ++requestSequenceRef.current;
    void feedItemList({
      view: currentView,
      sourceId: currentSource,
      receivedAfter: null,
      cursor: { sortAt: last.receivedAt, rowId: last.rowId },
      limit: FEED_PAGE_LIMIT,
    })
      .then((rows) => {
        if (requestSequenceRef.current !== sequence) return;
        setItems((previous) => [...previous, ...rows]);
        setPage((previous) => previous + 1);
      })
      .catch(() => {
        // 分页失败静默：下次滚动重试。
      });
  }, [items, status]);

  return {
    view,
    sourceId,
    search,
    selectedItemId,
    page,
    status,
    errorCode,
    items,
    sources,
    setView,
    setSourceId,
    setSearch,
    selectItem: setSelectedItemId,
    setItemState,
    markAllRead,
    loadMore,
    refresh,
  };
}
