import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const {
  feedItemList,
  feedSourceList,
  feedItemSetState,
  feedItemsMarkRead,
  listenFeedChanged,
} = vi.hoisted(() => ({
  feedItemList: vi.fn(),
  feedSourceList: vi.fn(),
  feedItemSetState: vi.fn(),
  feedItemsMarkRead: vi.fn(),
  listenFeedChanged: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  feedItemList,
  feedSourceList,
  feedItemSetState,
  feedItemsMarkRead,
  listenFeedChanged,
}));

import { act, renderHook, waitFor } from "@testing-library/react";

import { useFeedLibrary } from "@/hooks/useFeedLibrary";
import type { FeedItemSummary, FeedSourceSummary } from "@/types/ipc";

function source(id: string): FeedSourceSummary {
  return {
    id,
    title: `Source ${id}`,
    feedUrl: `https://example.com/${id}.xml`,
    siteUrl: null,
    folderPath: "",
    isEnabled: true,
    fetchIntervalMinutes: 60,
    fulltextEnabled: false,
    unreadCount: 1,
    lastCheckedAt: null,
    lastSuccessAt: null,
    nextFetchAt: null,
    consecutiveFailures: 0,
    lastErrorCode: null,
  };
}

function item(
  id: string,
  receivedAt = "2026-08-01T08:00:00Z",
): FeedItemSummary {
  return {
    rowId: Number(id.slice(1)),
    id,
    sourceId: "src-1",
    sourceTitle: "Source src-1",
    title: `Item ${id}`,
    authorName: null,
    canonicalUrl: null,
    publishedAt: null,
    receivedAt,
    sortAt: receivedAt,
    excerpt: "excerpt",
    isRead: false,
    isStarred: false,
    isArchived: false,
    conversionStatus: "ok",
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  feedSourceList.mockResolvedValue([source("src-1")]);
  feedItemList.mockResolvedValue([item("i1")]);
  feedItemSetState.mockResolvedValue(undefined);
  listenFeedChanged.mockResolvedValue(() => undefined);
});

afterEach(() => {
  vi.useRealTimers();
});

describe("useFeedLibrary", () => {
  it("loads sources and the inbox on mount", async () => {
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(feedSourceList).toHaveBeenCalledTimes(1);
    expect(feedItemList).toHaveBeenCalledWith({
      view: "inbox",
      sourceId: null,
      search: null,
      receivedAfter: null,
      cursor: null,
      limit: 51,
    });
    expect(result.current.view).toBe("inbox");
    expect(result.current.items).toHaveLength(1);
    expect(result.current.sources).toHaveLength(1);
  });

  it("re-queries on source and view changes", async () => {
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => result.current.setSourceId("src-1"));
    await waitFor(() =>
      expect(feedItemList).toHaveBeenLastCalledWith(
        expect.objectContaining({ sourceId: "src-1" }),
      ),
    );

    act(() => result.current.setView("starred"));
    await waitFor(() =>
      expect(feedItemList).toHaveBeenLastCalledWith(
        expect.objectContaining({ view: "starred", sourceId: "src-1" }),
      ),
    );
  });

  it("discards late responses after a filter change", async () => {
    let resolveFirst: (rows: FeedItemSummary[]) => void = () => undefined;
    const first = new Promise<FeedItemSummary[]>((resolve) => {
      resolveFirst = resolve;
    });
    feedItemList.mockImplementationOnce(() => first);
    feedItemList.mockResolvedValue([item("i2")]);

    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(feedItemList).toHaveBeenCalledTimes(1));

    // 视图切换触发第二次请求（立即返回 i2）。
    act(() => result.current.setView("all"));
    await waitFor(() =>
      expect(result.current.items.map((row) => row.id)).toEqual(["i2"]),
    );

    // 迟到的第一次响应（i1）不得覆盖当前视图。
    await act(async () => {
      resolveFirst([item("i1")]);
    });
    expect(result.current.items.map((row) => row.id)).toEqual(["i2"]);
  });

  it("refreshes lists when a feed:changed event arrives", async () => {
    let notify: ((event: unknown) => void) | undefined;
    listenFeedChanged.mockImplementation(
      (callback: (event: unknown) => void) => {
        notify = callback;
        return Promise.resolve(() => undefined);
      },
    );
    feedItemList.mockResolvedValue([item("i3")]);

    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    const callsBefore = feedItemList.mock.calls.length;
    await act(async () => {
      notify?.({
        sourceId: "src-1",
        kind: "items_changed",
        newItems: 1,
        errorCode: null,
      });
    });
    await waitFor(() =>
      expect(feedItemList.mock.calls.length).toBeGreaterThan(callsBefore),
    );
    await waitFor(() =>
      expect(result.current.items.map((row) => row.id)).toEqual(["i3"]),
    );
    // 事件后订阅源（未读数）也刷新。
    expect(feedSourceList.mock.calls.length).toBeGreaterThan(1);
  });

  it("manual refresh reloads both sources and current rows", async () => {
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));
    const sourceCalls = feedSourceList.mock.calls.length;
    const itemCalls = feedItemList.mock.calls.length;

    act(() => result.current.refresh());

    await waitFor(() =>
      expect(feedSourceList.mock.calls.length).toBeGreaterThan(sourceCalls),
    );
    await waitFor(() =>
      expect(feedItemList.mock.calls.length).toBeGreaterThan(itemCalls),
    );
  });

  it("rolls back item state when the backend rejects", async () => {
    feedItemSetState.mockRejectedValueOnce(new Error("feed_item_not_found"));
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => {
      result.current.setItemState("i1", { isRead: true });
    });
    // 乐观更新先生效。
    expect(result.current.items.find((row) => row.id === "i1")?.isRead).toBe(
      true,
    );
    // 后端拒绝后回滚。
    await waitFor(() =>
      expect(result.current.items.find((row) => row.id === "i1")?.isRead).toBe(
        false,
      ),
    );
  });

  it("failed optimistic update only rolls back its target item", async () => {
    let rejectFirst: (reason?: unknown) => void = () => undefined;
    feedItemList.mockResolvedValue([item("i1"), item("i2")]);
    feedItemSetState
      .mockImplementationOnce(
        () =>
          new Promise<void>((_resolve, reject) => {
            rejectFirst = reject;
          }),
      )
      .mockResolvedValueOnce(undefined);
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.items).toHaveLength(2));

    act(() => {
      result.current.setItemState("i1", { isRead: true });
      result.current.setItemState("i2", { isStarred: true });
    });
    await waitFor(() =>
      expect(
        result.current.items.find((row) => row.id === "i2")?.isStarred,
      ).toBe(true),
    );
    await act(async () => rejectFirst(new Error("failed")));

    expect(result.current.items.find((row) => row.id === "i1")?.isRead).toBe(
      false,
    );
    expect(result.current.items.find((row) => row.id === "i2")?.isStarred).toBe(
      true,
    );
  });

  it("today view keeps the server-side midnight boundary", async () => {
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => result.current.setView("today"));
    await waitFor(() =>
      expect(feedItemList).toHaveBeenLastCalledWith({
        view: "today",
        sourceId: null,
        search: null,
        receivedAfter: null,
        cursor: null,
        limit: 51,
      }),
    );
  });

  it("searches with the frozen query and clears back to the view", async () => {
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    act(() => result.current.setSearch("hello"));
    await waitFor(() =>
      expect(feedItemList).toHaveBeenLastCalledWith(
        expect.objectContaining({ search: "hello", view: "inbox" }),
      ),
    );

    act(() => result.current.setSearch(""));
    await waitFor(() =>
      expect(feedItemList).toHaveBeenLastCalledWith(
        expect.objectContaining({ view: "inbox" }),
      ),
    );
  });

  it("tracks the loaded page and loads more with a keyset cursor", async () => {
    const firstPage = Array.from({ length: 51 }, (_, index) =>
      item(
        `i${index + 1}`,
        `2026-08-02T08:${String(59 - index).padStart(2, "0")}:00Z`,
      ),
    );
    feedItemList
      .mockResolvedValueOnce(firstPage)
      .mockResolvedValue([item("i52", "2026-08-01T08:00:00Z")]);
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.hasMore).toBe(true);

    act(() => {
      void result.current.loadMore();
    });
    await waitFor(() => expect(result.current.page).toBe(2));
    expect(result.current.items).toHaveLength(51);
    expect(result.current.items.at(-1)?.id).toBe("i52");
    expect(feedItemList).toHaveBeenLastCalledWith({
      view: "inbox",
      sourceId: null,
      search: null,
      receivedAfter: null,
      cursor: { sortAt: "2026-08-02T08:10:00Z", rowId: 50 },
      limit: 51,
    });
  });

  it("only exposes load-more when a full page was returned", async () => {
    feedItemList.mockResolvedValue(
      Array.from({ length: 51 }, (_, index) => item(`i${index + 1}`)),
    );
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));
    expect(result.current.hasMore).toBe(true);
  });

  it("exposes batch mark-read and error surfaces", async () => {
    feedItemsMarkRead.mockResolvedValue(2);
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("ready"));

    let affected = 0;
    act(() => {
      void result.current.markAllRead().then((n) => {
        affected = n;
      });
    });
    await waitFor(() => expect(affected).toBe(2));
    expect(feedItemsMarkRead).toHaveBeenCalledWith({
      view: "inbox",
      sourceId: null,
      search: null,
      receivedAfter: null,
      cursor: null,
      limit: 50,
    });
  });

  it("surfaces stable error codes without crashing", async () => {
    feedItemList.mockRejectedValueOnce({
      code: "database",
      message: "Database error",
    });
    const { result } = renderHook(() => useFeedLibrary());
    await waitFor(() => expect(result.current.status).toBe("error"));
    expect(result.current.errorCode).toBe("database");
  });

  it("does not persist article content to localStorage", () => {
    expect(Object.keys(localStorage)).not.toContain(
      expect.stringContaining("feed"),
    );
    const { result } = renderHook(() => useFeedLibrary());
    expect(result.current.items).toEqual([]);
    expect(Object.keys(localStorage)).not.toContain(
      expect.stringContaining("feed"),
    );
  });
});
