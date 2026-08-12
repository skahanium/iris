import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const {
  feedItemList,
  feedSearch,
  feedSourceList,
  feedItemGet,
  feedItemSetState,
  feedItemsMarkRead,
  feedSyncSource,
  listenFeedChanged,
} = vi.hoisted(() => ({
  feedItemList: vi.fn(),
  feedSearch: vi.fn(),
  feedSourceList: vi.fn(),
  feedItemGet: vi.fn(),
  feedItemSetState: vi.fn(),
  feedItemsMarkRead: vi.fn(),
  feedSyncSource: vi.fn(),
  listenFeedChanged: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  feedItemList,
  feedSearch,
  feedSourceList,
  feedItemGet,
  feedItemSetState,
  feedItemsMarkRead,
  feedSyncSource,
  listenFeedChanged,
  openExternalHttpsUrl: vi.fn(),
}));

import { FeedWorkspace } from "@/components/feed/FeedWorkspace";
import type {
  FeedItemDetail,
  FeedItemSummary,
  FeedSourceSummary,
} from "@/types/ipc";

function source(overrides: Partial<FeedSourceSummary> = {}): FeedSourceSummary {
  return {
    id: "src-1",
    title: "Example Feed",
    feedUrl: "https://example.com/feed.xml",
    siteUrl: null,
    folderPath: "",
    isEnabled: true,
    fetchIntervalMinutes: 60,
    unreadCount: 2,
    lastCheckedAt: null,
    lastSuccessAt: null,
    nextFetchAt: null,
    consecutiveFailures: 0,
    lastErrorCode: null,
    ...overrides,
  };
}

function item(
  id: string,
  overrides: Partial<FeedItemSummary> = {},
): FeedItemSummary {
  return {
    rowId: Number(id.slice(1)),
    id,
    sourceId: "src-1",
    sourceTitle: "Example Feed",
    title: `Item ${id}`,
    authorName: null,
    canonicalUrl: "https://example.com/article",
    publishedAt: "2026-08-01T08:00:00Z",
    receivedAt: "2026-08-01T08:00:00Z",
    excerpt: "excerpt",
    isRead: false,
    isStarred: false,
    isArchived: false,
    conversionStatus: "ok",
    ...overrides,
  };
}

function detailOf(summary: FeedItemSummary): FeedItemDetail {
  return {
    summary,
    contentMarkdown: `# ${summary.title}\n\n正文内容。`,
    summaryMarkdown: summary.excerpt,
  };
}

function setWidth(width: number) {
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    writable: true,
    value: width,
  });
}

/** 冲刷 promise 驱动的状态更新（真实 timers 下 act 包裹微任务）。 */
async function flush() {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
  setWidth(1440); // wide
  feedSourceList.mockResolvedValue([source()]);
  feedItemList.mockResolvedValue([item("i1"), item("i2")]);
  feedSearch.mockResolvedValue([]);
  feedItemGet.mockImplementation((id: string) =>
    Promise.resolve(detailOf(item(id))),
  );
  feedItemSetState.mockResolvedValue(undefined);
  feedItemsMarkRead.mockResolvedValue(2);
  feedSyncSource.mockResolvedValue({
    status: "succeeded",
    newItems: 0,
    errorCode: null,
  });
  listenFeedChanged.mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

async function renderWorkspace() {
  const utils = render(<FeedWorkspace />);
  await flush();
  await waitFor(() => expect(feedSourceList).toHaveBeenCalled());
  await flush();
  return utils;
}

async function openSidebar() {
  await waitFor(() =>
    expect(screen.getByTestId("feed-toggle-sidebar")).toBeTruthy(),
  );
  act(() => fireEvent.click(screen.getByTestId("feed-toggle-sidebar")));
  await flush();
}

describe("FeedWorkspace", () => {
  it("renders the five article views and switches queries", async () => {
    await renderWorkspace();
    await openSidebar();
    for (const view of ["inbox", "today", "all", "starred", "archived"]) {
      const button = screen.getByTestId(`feed-view-${view}`);
      act(() => fireEvent.click(button));
      await flush();
      await waitFor(() =>
        expect(feedItemList).toHaveBeenLastCalledWith(
          expect.objectContaining({ view }),
        ),
      );
    }
  });

  it("shows sync-failed sources with stable error text and retry", async () => {
    feedSourceList.mockResolvedValue([
      source(),
      source({
        id: "src-broken",
        title: "Broken Feed",
        lastErrorCode: "feed_http_error_500",
      }),
    ]);
    await renderWorkspace();
    await openSidebar();

    const failed = screen.getByTestId("feed-failed-source-src-broken");
    expect(failed).toBeTruthy();
    // 显式重试：点击重试按钮触发 feedSyncSource 并刷新。
    act(() =>
      fireEvent.click(screen.getByTestId("feed-retry-source-src-broken")),
    );
    await flush();
    await waitFor(() =>
      expect(feedSyncSource).toHaveBeenCalledWith("src-broken", true),
    );
    act(() =>
      fireEvent.click(screen.getByTestId("feed-failed-source-view-src-broken")),
    );
    await flush();
    await waitFor(() =>
      expect(feedItemList).toHaveBeenLastCalledWith(
        expect.objectContaining({ view: "all", sourceId: "src-broken" }),
      ),
    );
  });

  it("shows unread counts in the sidebar", async () => {
    feedSourceList.mockResolvedValue([
      source({ id: "src-1", title: "One", unreadCount: 5 }),
      source({ id: "src-2", title: "Two", unreadCount: 0 }),
    ]);
    await renderWorkspace();
    await openSidebar();
    expect(screen.getByTestId("feed-source-unread-src-1").textContent).toBe(
      "5",
    );
    expect(screen.queryByTestId("feed-source-unread-src-2")).toBeNull();
  });

  it("renders the empty state", async () => {
    feedItemList.mockResolvedValue([]);
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-list-empty")).toBeTruthy(),
    );
  });

  it("renders loading and error states with retry", async () => {
    feedItemList.mockReturnValue(new Promise(() => undefined));
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByText("正在加载订阅内容…")).toBeTruthy(),
    );
    cleanup();

    feedItemList.mockRejectedValue({
      code: "database",
      message: "Database error",
    });
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-list-error")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-list-error").textContent).toContain(
      "database",
    );
    act(() => fireEvent.click(screen.getByTestId("feed-list-retry")));
    await flush();
    await waitFor(() =>
      expect(feedItemList.mock.calls.length).toBeGreaterThan(1),
    );
  });

  it("marks an opened article read after one visible second", async () => {
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );
    // 挂载前启用 fake timers：阅读器内 1 秒延迟已读受控。
    vi.useFakeTimers();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await act(async () => {
      await Promise.resolve();
    });
    expect(screen.getByTestId("feed-reader-title")).toBeTruthy();
    expect(feedItemSetState).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(1000);
    });
    expect(feedItemSetState).toHaveBeenCalledWith("i1", { isRead: true });
    vi.useRealTimers();
  });

  it("does not auto-mark read when the setting is disabled", async () => {
    localStorage.setItem("iris-feed-auto-read", "false");
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );
    vi.useFakeTimers();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await act(async () => {
      await Promise.resolve();
    });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(2000);
    });
    expect(feedItemSetState).not.toHaveBeenCalled();
    vi.useRealTimers();
    localStorage.removeItem("iris-feed-auto-read");
  });

  it("supports j/k/m/s/e/r shortcuts and ignores editable targets", async () => {
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "j" }),
    );
    await flush();
    await waitFor(() => expect(feedItemGet).toHaveBeenCalledWith("i1"));

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "j" }),
    );
    await flush();
    await waitFor(() => expect(feedItemGet).toHaveBeenLastCalledWith("i2"));

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "k" }),
    );
    await flush();
    await waitFor(() => expect(feedItemGet).toHaveBeenLastCalledWith("i1"));

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "m" }),
    );
    expect(feedItemSetState).toHaveBeenCalledWith("i1", { isRead: true });

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "s" }),
    );
    expect(feedItemSetState).toHaveBeenCalledWith("i1", { isStarred: true });

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "e" }),
    );
    expect(feedItemSetState).toHaveBeenCalledWith("i1", { isArchived: true });

    const before = feedItemList.mock.calls.length;
    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "r" }),
    );
    await flush();
    await waitFor(() =>
      expect(feedItemList.mock.calls.length).toBeGreaterThan(before),
    );

    // 输入框聚焦时不触发。
    const input = document.createElement("input");
    document.body.appendChild(input);
    const getCallsBefore = feedItemGet.mock.calls.length;
    act(() => fireEvent.keyDown(input, { key: "j" }));
    expect(feedItemGet.mock.calls.length).toBe(getCallsBefore);
    input.remove();
  });

  it("batch marks the frozen view read", async () => {
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-mark-all-read")).toBeTruthy(),
    );
    act(() => fireEvent.click(screen.getByTestId("feed-mark-all-read")));
    await flush();
    await waitFor(() =>
      expect(feedItemsMarkRead).toHaveBeenCalledWith(
        expect.objectContaining({ view: "inbox" }),
      ),
    );
  });

  it("blocks remote images by default and loads them on demand", async () => {
    feedItemGet.mockResolvedValue({
      summary: item("i1"),
      contentMarkdown: "![photo](https://cdn.example.com/a.png)",
      summaryMarkdown: "",
    });
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await flush();
    await waitFor(() =>
      expect(screen.getByTestId("feed-reader-body")).toBeTruthy(),
    );
    const body = screen.getByTestId("feed-reader-body");
    expect(body.querySelectorAll("img").length).toBe(0);
    expect(body.textContent).toContain("图片");
    expect(screen.getByTestId("feed-load-remote-images")).toBeTruthy();

    act(() => fireEvent.click(screen.getByTestId("feed-load-remote-images")));
    await flush();
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-reader-body").querySelectorAll("img").length,
      ).toBe(1),
    );
  });

  it("uses single-plane list/reader state machine below 1024px", async () => {
    setWidth(900);
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await flush();
    await waitFor(() =>
      expect(screen.getByTestId("feed-reader-title")).toBeTruthy(),
    );
    expect(screen.queryByTestId("feed-item-i2")).toBeNull();
    act(() => fireEvent.click(screen.getByTestId("feed-back-to-list")));
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );
  });

  it("opens the source drawer at 1024-1365 and collapses at wide", async () => {
    setWidth(1200);
    await renderWorkspace();
    expect(screen.getByTestId("feed-open-drawer")).toBeTruthy();
    act(() => fireEvent.click(screen.getByTestId("feed-open-drawer")));
    await flush();
    expect(screen.getByTestId("feed-drawer")).toBeTruthy();
    act(() => fireEvent.click(screen.getByTestId("feed-drawer-close")));
    await flush();
    expect(screen.queryByTestId("feed-drawer")).toBeNull();

    setWidth(1440);
    act(() => {
      window.dispatchEvent(new Event("resize"));
    });
    await flush();
    expect(screen.getByTestId("feed-toggle-sidebar")).toBeTruthy();
    expect(screen.queryByTestId("feed-drawer")).toBeNull();
  });

  it("shows degraded conversion notice and external open action", async () => {
    feedItemGet.mockResolvedValue({
      summary: item("i1", { conversionStatus: "degraded" }),
      contentMarkdown: "# T\n\nbody",
      summaryMarkdown: "",
    });
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await flush();
    await waitFor(() =>
      expect(screen.getByTestId("feed-degraded-notice")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-open-external")).toBeTruthy();
    expect(
      screen.getByTestId("feed-reader-permalink").getAttribute("href"),
    ).toBe("https://example.com/article");
  });
});
