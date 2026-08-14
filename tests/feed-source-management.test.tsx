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
  feedSourceTrash,
  feedSourceTrashMatch,
  feedSourceTrashPreview,
  feedSyncSource,
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
  feedSourceTrash: vi.fn(),
  feedSourceTrashMatch: vi.fn(),
  feedSourceTrashPreview: vi.fn(),
  feedSyncSource: vi.fn(),
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
  feedSourceTrash,
  feedSourceTrashMatch,
  feedSourceTrashPreview,
  feedSyncSource,
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
    fulltextEnabled: true,
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
  feedSourceTrashMatch.mockResolvedValue(null);
  feedSourceTrashPreview.mockResolvedValue({
    itemCount: 5,
    starredCount: 2,
    purgeAfter: "2026-09-12T00:00:00Z",
  });
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
  it("keeps the discovery controls inside the dialog gutter", () => {
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
      />,
    );

    expect(screen.getByTestId("feed-source-dialog").className).toContain(
      "sm:max-w-sm",
    );
    expect(screen.getByTestId("feed-source-dialog-body").className).toContain(
      "px-5",
    );
    expect(screen.getByTestId("feed-discover-controls").className).toContain(
      "space-y-2",
    );
    expect(
      screen.getByTestId("feed-discover-url").parentElement?.className,
    ).toContain("w-full");
  });

  it("places discovery and next-step actions in one equal-width action bar", () => {
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
      />,
    );

    const discover = screen.getByTestId("feed-discover-run");
    const next = screen.getByTestId("feed-confirm-subscribe");
    expect(discover.parentElement).toBe(next.parentElement);
    expect(discover.parentElement?.className).toContain("grid-cols-2");
  });

  it("uses the configured default interval for a newly confirmed source", async () => {
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
        defaultFetchIntervalMinutes={180}
      />,
    );
    fireEvent.change(screen.getByTestId("feed-discover-url"), {
      target: { value: "https://example.com" },
    });
    fireEvent.click(screen.getByTestId("feed-discover-run"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
      ).toBeTruthy(),
    );
    fireEvent.click(
      screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
    );
    fireEvent.click(screen.getByTestId("feed-confirm-subscribe"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-add-interval")).toHaveTextContent(
        "3 小时",
      ),
    );
  });

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

  it("reports the bounded initial-history import without exposing implementation details", async () => {
    feedSyncSource.mockResolvedValueOnce({
      status: "succeeded",
      newItems: 50,
      skippedHistory: 1075,
      errorCode: null,
    });
    const onOpenChange = vi.fn();
    render(
      <FeedSourceDialog
        open
        mode="add"
        source={null}
        onOpenChange={onOpenChange}
        onSourcesChanged={() => undefined}
      />,
    );
    fireEvent.change(screen.getByTestId("feed-discover-url"), {
      target: { value: "https://example.com" },
    });
    fireEvent.click(screen.getByTestId("feed-discover-run"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
      ).toBeTruthy(),
    );
    fireEvent.click(
      screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
    );
    fireEvent.click(screen.getByTestId("feed-confirm-subscribe"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-add-submit")).toBeTruthy(),
    );
    fireEvent.click(screen.getByTestId("feed-add-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("feed-add-success")).toHaveTextContent(
        "已导入最新 50 篇，略过 1075 篇较早历史",
      ),
    );
    expect(onOpenChange).not.toHaveBeenCalledWith(false);
    fireEvent.click(screen.getByTestId("feed-add-success-close"));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("源已添加但首次同步失败时重试不会重复创建来源", async () => {
    feedSyncSource
      .mockResolvedValueOnce({
        status: "failed",
        newItems: 0,
        errorCode: "feed_fetch_failed",
      })
      .mockResolvedValueOnce({
        status: "succeeded",
        newItems: 1,
        errorCode: null,
      });
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
    await waitFor(() => screen.getByTestId("feed-candidate-list"));
    fireEvent.click(
      screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
    );
    fireEvent.click(screen.getByTestId("feed-confirm-subscribe"));
    await waitFor(() => screen.getByTestId("feed-add-submit"));
    fireEvent.click(screen.getByTestId("feed-add-submit"));

    await waitFor(() =>
      expect(screen.getByTestId("feed-dialog-error")).toHaveTextContent(
        "订阅已添加，但首次同步失败",
      ),
    );
    fireEvent.click(screen.getByTestId("feed-add-submit"));
    await waitFor(() => expect(onOpenChange).toHaveBeenCalledWith(false));
    expect(feedSourceAdd).toHaveBeenCalledTimes(1);
    expect(feedSyncSource).toHaveBeenCalledTimes(2);
    expect(onSourcesChanged).toHaveBeenCalledTimes(1);
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
        fulltextEnabled: true,
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
      expect(feedSourceUpdate).toHaveBeenCalledWith("src-1", {
        isEnabled: false,
      }),
    );
    expect(feedSourceTrashPreview).toHaveBeenCalledWith("src-1");
  });

  it("labels source management and the recoverable unsubscribe action explicitly", async () => {
    render(
      <FeedSourceDialog
        open
        mode="edit"
        source={source()}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
      />,
    );

    await waitFor(() =>
      expect(screen.getByTestId("feed-unsubscribe-delete")).toHaveTextContent(
        "5 篇",
      ),
    );

    expect(screen.getByRole("dialog")).toHaveTextContent("管理订阅");
    expect(screen.getByRole("dialog")).toHaveTextContent(
      "修改订阅设置；如不再需要，可退订并移入 RSS 回收站。",
    );
    expect(screen.getByTestId("feed-unsubscribe-delete")).toHaveTextContent(
      "退订并移入 RSS 回收站（5 篇）",
    );

    fireEvent.click(screen.getByTestId("feed-unsubscribe-delete"));
    await waitFor(() =>
      expect(
        screen.getByTestId("feed-delete-confirm-submit"),
      ).toHaveTextContent("退订并移入 RSS 回收站"),
    );
  });

  it("confirms deletion with article, favorite, and purge details", async () => {
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
    expect(screen.getByTestId("feed-delete-confirm").textContent).toContain(
      "2",
    );
    // 删除前不得调用 remove（二次确认后才执行）。
    expect(feedSourceTrash).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("feed-delete-confirm-submit"));
    await waitFor(() => expect(feedSourceTrash).toHaveBeenCalledWith("src-1"));
    expect(onOpenChange).toHaveBeenCalledWith(false);
  });

  it("requires confirmation before restoring an identical trashed URL", async () => {
    feedSourceTrashMatch.mockResolvedValueOnce({
      id: "src-old",
      title: "Old feed",
      itemCount: 8,
      starredCount: 1,
      deletedAt: "2026-08-13T00:00:00Z",
      purgeAfter: "2026-09-12T00:00:00Z",
    });
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
    await waitFor(() => screen.getByTestId("feed-candidate-list"));
    fireEvent.click(
      screen.getByTestId("feed-candidate-https://example.com/feed.xml"),
    );
    fireEvent.click(screen.getByTestId("feed-confirm-subscribe"));
    await waitFor(() => screen.getByTestId("feed-add-submit"));
    fireEvent.click(screen.getByTestId("feed-add-submit"));
    await waitFor(() => screen.getByTestId("feed-restore-match"));
    expect(feedSourceAdd).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("feed-restore-subscribe"));
    await waitFor(() =>
      expect(feedSourceAdd).toHaveBeenCalledWith(
        expect.objectContaining({ restoreDeleted: true }),
      ),
    );
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

  it("does not expose internal error codes when editing a source fails", async () => {
    feedSourceUpdate.mockRejectedValueOnce({
      code: "feed_source_validation_failed",
    });
    render(
      <FeedSourceDialog
        open
        mode="edit"
        source={source()}
        onOpenChange={() => undefined}
        onSourcesChanged={() => undefined}
      />,
    );

    fireEvent.click(screen.getByTestId("feed-edit-save"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-dialog-error")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-dialog-error")).toHaveTextContent(
      "保存订阅设置失败，请稍后重试。",
    );
    expect(screen.getByTestId("feed-dialog-error")).not.toHaveTextContent(
      "feed_source_validation_failed",
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
    expect(feedItemList).not.toHaveBeenCalledWith(
      expect.objectContaining({ search: "hel" }),
    );
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(feedItemList).toHaveBeenCalledWith(
      expect.objectContaining({ search: "hel", sourceId: null, limit: 51 }),
    );

    // Escape 清空并回到先前视图。
    fireEvent.keyDown(input, { key: "Escape" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    expect(feedItemList).toHaveBeenLastCalledWith(
      expect.objectContaining({ search: null }),
    );
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
    expect(feedItemList).not.toHaveBeenCalledWith(
      expect.objectContaining({ search: "中" }),
    );
    fireEvent.compositionEnd(input);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(300);
    });
    await act(async () => {
      await Promise.resolve();
    });
    expect(feedItemList).toHaveBeenCalledWith(
      expect.objectContaining({ search: "中", sourceId: null, limit: 51 }),
    );
    vi.useRealTimers();
  });

  it("surfaces search errors with a retry", async () => {
    feedItemList.mockImplementation((query: { search?: string | null }) =>
      query.search === "boom"
        ? Promise.reject({ code: "database", message: "Database error" })
        : Promise.resolve([]),
    );
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
    expect(screen.getByTestId("feed-list-error").textContent).not.toContain(
      "database",
    );
    fireEvent.click(screen.getByTestId("feed-list-retry"));
    await waitFor(() =>
      expect(
        feedItemList.mock.calls.filter(
          ([query]) => (query as { search?: string }).search === "boom",
        ).length,
      ).toBeGreaterThan(1),
    );
  });
});
