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
  feedDiscover,
  feedSourceAdd,
  feedSourceUpdate,
  feedSourceRemove,
  feedSourceItemCount,
  feedSyncSource,
  feedSearch,
  feedItemList,
  feedSourceList,
  feedItemGet,
  feedItemSetState,
  feedItemsMarkRead,
  listenFeedChanged,
} = vi.hoisted(() => ({
  feedDiscover: vi.fn(),
  feedSourceAdd: vi.fn(),
  feedSourceUpdate: vi.fn(),
  feedSourceRemove: vi.fn(),
  feedSourceItemCount: vi.fn(),
  feedSyncSource: vi.fn(),
  feedSearch: vi.fn(),
  feedItemList: vi.fn(),
  feedSourceList: vi.fn(),
  feedItemGet: vi.fn(),
  feedItemSetState: vi.fn(),
  feedItemsMarkRead: vi.fn(),
  listenFeedChanged: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  feedDiscover,
  feedSourceAdd,
  feedSourceUpdate,
  feedSourceRemove,
  feedSourceItemCount,
  feedSyncSource,
  feedSearch,
  feedItemList,
  feedSourceList,
  feedItemGet,
  feedItemSetState,
  feedItemsMarkRead,
  listenFeedChanged,
  openExternalHttpsUrl: vi.fn(),
}));

import { FeedSourceDialog } from "@/components/feed/FeedSourceDialog";
import { FeedWorkspace } from "@/components/feed/FeedWorkspace";
import type { FeedSourceSummary } from "@/types/ipc";

function source(overrides: Partial<FeedSourceSummary> = {}): FeedSourceSummary {
  return {
    id: "src-1",
    title: "Example Feed",
    feedUrl: "https://example.com/feed.xml",
    siteUrl: null,
    folderPath: "tech",
    isEnabled: true,
    fetchIntervalMinutes: 60,
    unreadCount: 3,
    lastCheckedAt: null,
    lastSuccessAt: null,
    nextFetchAt: null,
    consecutiveFailures: 0,
    lastErrorCode: null,
    ...overrides,
  };
}

beforeEach(() => {
  vi.clearAllMocks();
  vi.useRealTimers();
  feedDiscover.mockResolvedValue([
    {
      url: "https://example.com/feed.xml",
      title: "Example Feed",
      format: "rss",
    },
    { url: "https://example.com/atom.xml", title: "Atom Feed", format: "atom" },
  ]);
  feedSourceAdd.mockResolvedValue(source({ id: "src-new" }));
  feedSyncSource.mockResolvedValue({
    status: "succeeded",
    newItems: 3,
    errorCode: null,
  });
  feedSourceUpdate.mockResolvedValue(undefined);
  feedSourceRemove.mockResolvedValue(5);
  feedSourceItemCount.mockResolvedValue(5);
  feedSearch.mockResolvedValue([]);
  feedItemList.mockResolvedValue([]);
  feedSourceList.mockResolvedValue([source()]);
  feedItemGet.mockResolvedValue(null);
  feedItemSetState.mockResolvedValue(undefined);
  feedItemsMarkRead.mockResolvedValue(0);
  listenFeedChanged.mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
});

describe("FeedSourceDialog 管理交互", () => {
  it("discovers candidates and requires a single explicit selection", async () => {
    const onOpenChange = vi.fn();
    const onSourcesChanged = vi.fn();
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={onOpenChange}
        onSourcesChanged={onSourcesChanged}
      />,
    );

    fireEvent.change(screen.getByTestId("feed-discover-url"), {
      target: { value: "https://example.com" },
    });
    fireEvent.click(screen.getByTestId("feed-discover-run"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-candidate-list")).toBeTruthy(),
    );
    const candidates = screen.getAllByRole("radio");
    expect(candidates.length).toBe(2);

    // 多候选不可自动全选/自动下一步：未选择时下一步禁用。
    expect(
      (screen.getByTestId("feed-confirm-subscribe") as HTMLButtonElement)
        .disabled,
    ).toBe(true);

    fireEvent.click(
      screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
    );
    expect(
      (screen.getByTestId("feed-confirm-subscribe") as HTMLButtonElement)
        .disabled,
    ).toBe(false);
    fireEvent.click(screen.getByTestId("feed-confirm-subscribe"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-add-title")).toBeTruthy(),
    );
  });

  it("adds the chosen candidate and syncs with the history choice", async () => {
    const onOpenChange = vi.fn();
    const onSourcesChanged = vi.fn();
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={onOpenChange}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.change(screen.getByTestId("feed-discover-url"), {
      target: { value: "https://example.com" },
    });
    fireEvent.click(screen.getByTestId("feed-discover-run"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-candidate-https://example.com/atom.xml"),
      ).toBeTruthy(),
    );
    fireEvent.click(
      screen.getByTestId("feed-candidate-https://example.com/atom.xml"),
    );
    fireEvent.click(screen.getByTestId("feed-confirm-subscribe"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-add-title")).toBeTruthy(),
    );

    // 默认历史已读（markHistoryRead=true）；勾选后为未读。
    fireEvent.change(screen.getByTestId("feed-add-folder"), {
      target: { value: "技术/Rust" },
    });
    fireEvent.click(screen.getByTestId("feed-add-history-unread"));
    fireEvent.click(screen.getByTestId("feed-add-submit"));

    await waitFor(() =>
      expect(feedSourceAdd).toHaveBeenCalledWith({
        url: "https://example.com/atom.xml",
        title: "Atom Feed",
        folderPath: "技术/Rust",
        fetchIntervalMinutes: 60,
      }),
    );
    await waitFor(() =>
      expect(feedSyncSource).toHaveBeenCalledWith("src-new", false),
    );
    expect(onSourcesChanged).toHaveBeenCalled();
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("edits title/folder/interval and pauses the source", async () => {
    const onSourcesChanged = vi.fn();
    render(
      <FeedSourceDialog
        open
        mode="edit"
        source={source()}
        onOpenChange={() => undefined}
        onSourcesChanged={onSourcesChanged}
      />,
    );
    fireEvent.change(screen.getByTestId("feed-edit-title"), {
      target: { value: "Renamed" },
    });
    fireEvent.change(screen.getByTestId("feed-edit-folder"), {
      target: { value: "新闻" },
    });
    // 暂停：关闭启用。
    fireEvent.click(screen.getByTestId("feed-edit-enabled"));
    fireEvent.click(screen.getByTestId("feed-edit-save"));
    await waitFor(() =>
      expect(feedSourceUpdate).toHaveBeenCalledWith("src-1", {
        titleOverride: "Renamed",
        folderPath: "新闻",
        fetchIntervalMinutes: 60,
        isEnabled: false,
      }),
    );
    expect(onSourcesChanged).toHaveBeenCalled();
  });

  it("unsubscribes keeping articles (pause only)", async () => {
    render(
      <FeedSourceDialog
        open
        mode="edit"
        source={source()}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
      />,
    );
    fireEvent.click(screen.getByTestId("feed-unsubscribe-keep"));
    await waitFor(() =>
      expect(feedSourceRemove).toHaveBeenCalledWith("src-1", true),
    );
    expect(feedSourceItemCount).toHaveBeenCalledWith("src-1");
  });

  it("confirms deletion with the article count before removing", async () => {
    const onOpenChange = vi.fn();
    render(
      <FeedSourceDialog
        open
        mode="edit"
        source={source()}
        onOpenChange={onOpenChange}
        onSourcesChanged={() => undefined}
      />,
    );
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-unsubscribe-delete").textContent,
      ).toContain("5 篇"),
    );
    fireEvent.click(screen.getByTestId("feed-unsubscribe-delete"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-delete-confirm")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-delete-confirm").textContent).toContain(
      "5",
    );
    // 删除前不得调用 remove（二次确认后才执行）。
    expect(feedSourceRemove).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("feed-delete-confirm-submit"));
    await waitFor(() =>
      expect(feedSourceRemove).toHaveBeenCalledWith("src-1", false),
    );
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("shows stable error text without raw payload", async () => {
    feedDiscover.mockRejectedValueOnce(new Error("raw html <script>"));
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
      />,
    );
    fireEvent.change(screen.getByTestId("feed-discover-url"), {
      target: { value: "https://example.com" },
    });
    fireEvent.click(screen.getByTestId("feed-discover-run"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-dialog-error")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-dialog-error").textContent).not.toContain(
      "<script>",
    );
  });
});

describe("订阅搜索交互", () => {
  it("debounces search input by 200ms and clears on Escape", async () => {
    vi.useFakeTimers();
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      writable: true,
      value: 1440,
    });
    feedItemList.mockResolvedValue([]);
    render(<FeedWorkspace />);
    await act(async () => {
      await Promise.resolve();
    });

    const input = screen.getByTestId("feed-search-input");
    fireEvent.change(input, { target: { value: "h" } });
    fireEvent.change(input, { target: { value: "he" } });
    fireEvent.change(input, { target: { value: "hel" } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(150);
    });
    expect(feedSearch).not.toHaveBeenCalled();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(feedSearch).toHaveBeenCalledWith("hel", null, 50);

    // Escape 清空并回到先前视图。
    fireEvent.keyDown(input, { key: "Escape" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(feedSearch.mock.calls.length).toBe(1);
    vi.useRealTimers();
  });

  it("does not fire search during IME composition", async () => {
    vi.useFakeTimers();
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      writable: true,
      value: 1440,
    });
    render(<FeedWorkspace />);
    await act(async () => {
      await Promise.resolve();
    });
    const input = screen.getByTestId("feed-search-input");
    fireEvent.compositionStart(input);
    fireEvent.change(input, { target: { value: "中" } });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(500);
    });
    expect(feedSearch).not.toHaveBeenCalled();
    fireEvent.compositionEnd(input);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(feedSearch).toHaveBeenCalledWith("中", null, 50);
    vi.useRealTimers();
  });

  it("surfaces search errors with a retry", async () => {
    feedSearch.mockRejectedValueOnce({
      code: "database",
      message: "Database error",
    });
    Object.defineProperty(window, "innerWidth", {
      configurable: true,
      writable: true,
      value: 1440,
    });
    render(<FeedWorkspace />);
    await act(async () => {
      await Promise.resolve();
    });
    const input = screen.getByTestId("feed-search-input");
    fireEvent.change(input, { target: { value: "boom" } });
    await waitFor(() =>
      expect(screen.getByTestId("feed-list-error")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-list-error").textContent).toContain(
      "database",
    );
    fireEvent.click(screen.getByTestId("feed-list-retry"));
    await waitFor(() =>
      expect(feedSearch.mock.calls.length).toBeGreaterThan(1),
    );
  });
});
