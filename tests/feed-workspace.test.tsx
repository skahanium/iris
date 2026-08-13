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
  feedSourceList,
  feedItemGet,
  feedItemSetState,
  feedItemsMarkRead,
  feedSyncSource,
  feedSyncAll,
  listenFeedChanged,
} = vi.hoisted(() => ({
  feedItemList: vi.fn(),
  feedSourceList: vi.fn(),
  feedItemGet: vi.fn(),
  feedItemSetState: vi.fn(),
  feedItemsMarkRead: vi.fn(),
  feedSyncSource: vi.fn(),
  feedSyncAll: vi.fn(),
  listenFeedChanged: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  feedItemList,
  feedSourceList,
  feedItemGet,
  feedItemSetState,
  feedItemsMarkRead,
  feedSyncSource,
  feedSyncAll,
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
    fulltextEnabled: false,
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
    sortAt: "2026-08-01T08:00:00Z",
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
    siteUrl: "https://example.com/site",
    contentOrigin: "feed",
    fulltextStatus: "not_requested",
  };
}

function setWidth(width: number) {
  Object.defineProperty(window, "innerWidth", {
    configurable: true,
    writable: true,
    value: width,
  });
}

class FeedResizeObserver {
  static instances: FeedResizeObserver[] = [];
  private readonly callback: ResizeObserverCallback;
  readonly targets = new Set<Element>();

  constructor(callback: ResizeObserverCallback) {
    this.callback = callback;
    FeedResizeObserver.instances.push(this);
  }

  observe(target: Element) {
    this.targets.add(target);
  }

  unobserve(target: Element) {
    this.targets.delete(target);
  }

  disconnect() {
    this.targets.clear();
  }

  fire(width: number) {
    this.callback(
      [...this.targets].map((target) => ({
        target,
        contentRect: { width },
      })) as ResizeObserverEntry[],
      this as unknown as ResizeObserver,
    );
  }
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
  feedSyncAll.mockResolvedValue({
    total: 2,
    succeeded: 2,
    failed: 0,
    skipped: 0,
    inFlight: 0,
    newItems: 3,
  });
  listenFeedChanged.mockResolvedValue(() => undefined);
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  delete (globalThis as { ResizeObserver?: unknown }).ResizeObserver;
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
  it("零订阅时显示整区引导，不请求详情也不误报文章失败", async () => {
    feedSourceList.mockResolvedValue([]);
    feedItemList.mockResolvedValue([]);

    await renderWorkspace();

    expect(screen.getByTestId("feed-library-onboarding")).toBeTruthy();
    expect(screen.getByText("还没有订阅")).toBeTruthy();
    expect(screen.queryByTestId("feed-list")).toBeNull();
    expect(screen.queryByText("文章加载失败")).toBeNull();
    expect(feedItemGet).not.toHaveBeenCalled();
  });

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
    expect(screen.getByTestId("feed-list-error").textContent).not.toContain(
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

  it("offers an explicit settings-backed auto-read toggle", async () => {
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await waitFor(() => screen.getByTestId("feed-toggle-auto-read"));
    const toggle = screen.getByTestId("feed-toggle-auto-read");
    expect(toggle.getAttribute("aria-pressed")).toBe("true");
    act(() => fireEvent.click(toggle));
    expect(toggle.getAttribute("aria-pressed")).toBe("false");
    // 浏览器测试环境没有 Tauri settings IPC；不得重新写入旧 localStorage 键。
    expect(localStorage.getItem("iris-feed-auto-read")).toBeNull();
    localStorage.removeItem("iris-feed-auto-read");
  });

  it("inactive 时不聚焦、不安装阅读监听且不会自动标记已读", async () => {
    const view = render(<FeedWorkspace active={false} />);
    await flush();
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );
    vi.useFakeTimers();
    await act(async () => {
      fireEvent.click(screen.getByTestId("feed-item-i1"));
      await Promise.resolve();
      await Promise.resolve();
    });
    const title = screen.getByTestId("feed-reader-title");
    expect(document.activeElement).not.toBe(title);

    act(() => fireEvent.keyDown(document, { key: "PageDown" }));
    await act(async () => vi.advanceTimersByTimeAsync(2000));
    expect(feedItemSetState).not.toHaveBeenCalled();

    view.rerender(<FeedWorkspace active />);
    await act(async () => vi.advanceTimersByTimeAsync(1000));
    expect(feedItemSetState).toHaveBeenCalledWith("i1", { isRead: true });
  });

  it("仅 Reader 内的有效阅读动作触发提前已读", async () => {
    await renderWorkspace();
    vi.useFakeTimers();
    await act(async () => {
      fireEvent.click(screen.getByTestId("feed-item-i1"));
      await Promise.resolve();
      await Promise.resolve();
    });
    const reader = screen.getByTestId("feed-reader");

    act(() => fireEvent.keyDown(reader, { key: "Shift" }));
    act(() => fireEvent.keyDown(reader, { key: "PageDown", ctrlKey: true }));
    expect(feedItemSetState).not.toHaveBeenCalled();
    act(() => fireEvent.keyDown(reader, { key: "PageDown" }));
    expect(feedItemSetState).toHaveBeenCalledWith("i1", { isRead: true });
  });

  it("详情失败后点击重试会为同一文章发起新请求", async () => {
    feedItemGet
      .mockRejectedValueOnce({ code: "feed_item_not_found" })
      .mockResolvedValueOnce(detailOf(item("i1")));
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await waitFor(() =>
      expect(screen.getByTestId("feed-reader-retry")).toBeTruthy(),
    );

    act(() => fireEvent.click(screen.getByTestId("feed-reader-retry")));
    await waitFor(() => expect(feedItemGet).toHaveBeenCalledTimes(2));
    expect(await screen.findByTestId("feed-reader-title")).toBeTruthy();
  });

  it("状态写入失败时重新加载详情以回滚详情投影", async () => {
    feedItemSetState.mockRejectedValueOnce(new Error("database"));
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await waitFor(() => screen.getByTestId("feed-toggle-read"));
    const detailCalls = feedItemGet.mock.calls.length;

    act(() => fireEvent.click(screen.getByTestId("feed-toggle-read")));
    await waitFor(() =>
      expect(feedItemGet.mock.calls.length).toBeGreaterThan(detailCalls),
    );
    expect(screen.getByTestId("feed-toggle-read").textContent).toContain(
      "标为已读",
    );
  });

  it("j/k 只移动 roving 焦点，Enter 打开；r 执行网络同步", async () => {
    await renderWorkspace();
    await waitFor(() =>
      expect(screen.getByTestId("feed-item-i1")).toBeTruthy(),
    );
    expect(screen.getByTestId("feed-item-i1").getAttribute("tabindex")).toBe(
      "0",
    );

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "j" }),
    );
    await flush();
    expect(feedItemGet).not.toHaveBeenCalled();
    expect(screen.getByTestId("feed-item-i2").getAttribute("tabindex")).toBe(
      "0",
    );

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "k" }),
    );
    await flush();
    expect(feedItemGet).not.toHaveBeenCalled();

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), {
        key: "Enter",
      }),
    );
    await waitFor(() => expect(feedItemGet).toHaveBeenCalledWith("i1"));

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

    act(() =>
      fireEvent.keyDown(screen.getByTestId("feed-workspace"), { key: "r" }),
    );
    await waitFor(() => expect(feedSyncAll).toHaveBeenCalledTimes(1));
    expect(screen.getByRole("status").textContent).toContain("同步完成");

    // 输入框聚焦时不触发。
    const input = document.createElement("input");
    document.body.appendChild(input);
    const getCallsBefore = feedItemGet.mock.calls.length;
    act(() => fireEvent.keyDown(input, { key: "j" }));
    expect(feedItemGet.mock.calls.length).toBe(getCallsBefore);
    input.remove();
  });

  it("窄屏仍提供来源、搜索、添加、OPML 与同步入口", async () => {
    setWidth(900);
    await renderWorkspace();
    expect(screen.getByTestId("feed-open-drawer")).toBeTruthy();
    expect(screen.getByTestId("feed-search-input")).toBeTruthy();
    expect(screen.getByTestId("feed-add-source")).toBeTruthy();
    expect(screen.getByTestId("feed-open-opml")).toBeTruthy();
    expect(screen.getByTestId("feed-sync-now")).toBeTruthy();
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

  it("依据订阅容器实测宽度而不是 window 宽度选择断点", async () => {
    setWidth(1600);
    FeedResizeObserver.instances = [];
    globalThis.ResizeObserver =
      FeedResizeObserver as unknown as typeof ResizeObserver;
    await renderWorkspace();

    const workspaceObserver = FeedResizeObserver.instances.find((observer) =>
      [...observer.targets].some(
        (target) => target.getAttribute("data-testid") === "feed-workspace",
      ),
    );
    expect(workspaceObserver).toBeTruthy();
    act(() => workspaceObserver?.fire(900));
    await waitFor(() => expect(screen.getByText("订阅")).toBeTruthy());
    expect(screen.queryByTestId("feed-toggle-sidebar")).toBeNull();

    act(() => workspaceObserver?.fire(1400));
    await waitFor(() =>
      expect(screen.getByTestId("feed-toggle-sidebar")).toBeTruthy(),
    );
  });

  it("opens the source drawer at 1024-1365 and collapses at wide", async () => {
    setWidth(1200);
    await renderWorkspace();
    expect(screen.getByTestId("feed-open-drawer")).toBeTruthy();
    act(() => fireEvent.click(screen.getByTestId("feed-open-drawer")));
    await flush();
    const drawer = screen.getByTestId("feed-drawer");
    expect(drawer.className).toContain("top-[var(--titlebar-height)]");
    expect(drawer.className).toContain("border-r");
    expect(screen.getByTestId("feed-sidebar").className).toContain(
      "border-r-0",
    );
    const drawerHeader = screen.getByTestId("feed-sidebar-header");
    expect(drawerHeader).toContainElement(
      screen.getByTestId("feed-drawer-close"),
    );
    expect(
      drawerHeader.querySelector('[data-testid="feed-add-source"]'),
    ).not.toBeNull();
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

  it("uses a full-width borderless source drawer below 1024px", async () => {
    setWidth(900);
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-open-drawer")));
    await flush();

    const drawer = screen.getByTestId("feed-drawer");
    expect(drawer.className).toContain("w-full");
    expect(drawer.className).toContain("border-r-0");
    const sidebar = screen.getByTestId("feed-sidebar");
    expect(sidebar.className).toContain("w-full");
    expect(sidebar.className).toContain("border-r-0");
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
describe("FeedWorkspace 保存为笔记", () => {
  const saveAsNote = vi.fn();

  async function renderWorkspaceWithSave() {
    saveAsNote.mockReset();
    saveAsNote.mockResolvedValue("技术/文章标题.md");
    const utils = render(<FeedWorkspace onSaveAsNote={saveAsNote} />);
    await flush();
    await waitFor(() => expect(feedSourceList).toHaveBeenCalled());
    await flush();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await flush();
    await waitFor(() =>
      expect(screen.getByTestId("feed-reader-title")).toBeTruthy(),
    );
    return utils;
  }

  it("未提供回调时不显示保存入口", async () => {
    await renderWorkspace();
    act(() => fireEvent.click(screen.getByTestId("feed-item-i1")));
    await flush();
    await waitFor(() =>
      expect(screen.getByTestId("feed-reader-title")).toBeTruthy(),
    );
    expect(screen.queryByTestId("feed-save-as-note")).toBeNull();
  });

  it("确认保存后回调收到模板 Markdown、标题与目录", async () => {
    await renderWorkspaceWithSave();
    act(() => fireEvent.click(screen.getByTestId("feed-save-as-note")));
    await flush();
    expect(screen.getByTestId("feed-save-note-dialog")).toBeTruthy();
    // 文件名预填文章标题（清理非法字符）。
    expect(
      (screen.getByTestId("feed-save-note-title") as HTMLInputElement).value,
    ).toBe("Item i1");

    fireEvent.change(screen.getByTestId("feed-save-note-folder"), {
      target: { value: "技术/Rust" },
    });
    fireEvent.click(screen.getByTestId("feed-save-note-confirm"));
    await waitFor(() => expect(saveAsNote).toHaveBeenCalledTimes(1));
    const [markdown, titleHint, folder] = saveAsNote.mock.calls[0] as [
      string,
      string,
      string,
    ];
    expect(markdown).toContain("# Item i1");
    expect(markdown).toContain(
      "> 来源：[Example Feed](https://example.com/site)  ",
    );
    expect(markdown).toContain("> 保存：");
    expect(titleHint).toBe("Item i1");
    expect(folder).toBe("技术/Rust");
    // 成功后对话框关闭。
    await waitFor(() =>
      expect(screen.queryByTestId("feed-save-note-dialog")).toBeNull(),
    );
  });

  it("保存失败停留在文章并显示可重试错误", async () => {
    await renderWorkspaceWithSave();
    saveAsNote.mockRejectedValue(new Error("笔记已锁定，无法保存"));
    act(() => fireEvent.click(screen.getByTestId("feed-save-as-note")));
    await flush();
    fireEvent.click(screen.getByTestId("feed-save-note-confirm"));
    await waitFor(() =>
      expect(screen.getByTestId("feed-save-note-error").textContent).toContain(
        "笔记已锁定，无法保存",
      ),
    );
    expect(screen.getByTestId("feed-save-note-dialog")).toBeTruthy();
    // 可重试：再次确认成功。
    saveAsNote.mockResolvedValueOnce("技术/文章标题.md");
    fireEvent.click(screen.getByTestId("feed-save-note-confirm"));
    await waitFor(() =>
      expect(screen.queryByTestId("feed-save-note-dialog")).toBeNull(),
    );
  });

  it("非法目录或空文件名不发起保存", async () => {
    await renderWorkspaceWithSave();
    act(() => fireEvent.click(screen.getByTestId("feed-save-as-note")));
    await flush();
    fireEvent.change(screen.getByTestId("feed-save-note-folder"), {
      target: { value: "技:术" },
    });
    fireEvent.click(screen.getByTestId("feed-save-note-confirm"));
    await flush();
    expect(saveAsNote).not.toHaveBeenCalled();
    expect(screen.getByTestId("feed-save-note-error").textContent).toContain(
      "目录",
    );
  });
});
