import {
  cleanup,
  fireEvent,
  render,
  screen,
  waitFor,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const {
  feedLibraryOptimize,
  feedLibrarySummary,
  feedSyncAll,
  feedTrashClear,
  feedTrashList,
  feedTrashRestore,
  setAutoReadEnabled,
  setBackgroundSyncEnabled,
  setDefaultFetchIntervalMinutes,
} = vi.hoisted(() => ({
  feedLibraryOptimize: vi.fn(),
  feedLibrarySummary: vi.fn(),
  feedSyncAll: vi.fn(),
  feedTrashClear: vi.fn(),
  feedTrashList: vi.fn(),
  feedTrashRestore: vi.fn(),
  setAutoReadEnabled: vi.fn(),
  setBackgroundSyncEnabled: vi.fn(),
  setDefaultFetchIntervalMinutes: vi.fn(),
}));

vi.mock("@/lib/ipc", () => ({
  feedLibraryOptimize,
  feedLibrarySummary,
  feedSyncAll,
  feedTrashClear,
  feedTrashList,
  feedTrashRestore,
}));
vi.mock("@/hooks/useFeedSettings", () => ({
  useFeedSettings: () => ({
    autoReadEnabled: true,
    backgroundSyncEnabled: true,
    defaultFetchIntervalMinutes: 60,
    setAutoReadEnabled,
    setBackgroundSyncEnabled,
    setDefaultFetchIntervalMinutes,
  }),
}));
vi.mock("@/components/feed/FeedOpmlDialog", () => ({
  FeedOpmlDialog: ({ open }: { open: boolean }) =>
    open ? <div data-testid="mock-feed-opml-dialog">OPML</div> : null,
}));

import { FeedManagementSection } from "@/components/settings/FeedManagementSection";

describe("FeedManagementSection", () => {
  beforeEach(() => {
    vi.clearAllMocks();
    feedLibrarySummary.mockResolvedValue({
      sourceCount: 3,
      enabledSourceCount: 2,
      failedSourceCount: 1,
      itemCount: 50,
      unreadCount: 12,
      lastSuccessAt: "2026-08-13T01:02:03Z",
    });
    feedSyncAll.mockResolvedValue({
      succeeded: 2,
      failed: 0,
      newItems: 4,
      skippedHistory: 20,
    });
    feedTrashList.mockResolvedValue([
      {
        item: { id: "item-1", title: "已删除文章" },
        deletedAt: "2026-08-13T00:00:00Z",
        purgeAfter: "2026-09-12T00:00:00Z",
      },
    ]);
    feedTrashRestore.mockResolvedValue(undefined);
    feedTrashClear.mockResolvedValue(1);
    feedLibraryOptimize.mockResolvedValue(undefined);
  });

  afterEach(cleanup);

  it("manages global RSS policy and maintenance without adding another proxy toggle", async () => {
    const openOverview = vi.fn();
    render(
      <FeedManagementSection
        proxyStatusLabel="已跟随系统代理"
        onOpenOverview={openOverview}
      />,
    );

    await waitFor(() => expect(feedLibrarySummary).toHaveBeenCalledTimes(1));
    expect(screen.getByText("50")).toBeTruthy();
    expect(screen.getByText("12")).toBeTruthy();
    expect(screen.getByText("当前状态：已跟随系统代理")).toBeTruthy();
    expect(screen.queryByText("使用 RSS 代理")).toBeNull();

    fireEvent.click(screen.getByTestId("feed-auto-read-switch"));
    fireEvent.click(screen.getByTestId("feed-background-sync-switch"));
    fireEvent.change(screen.getByTestId("feed-default-interval"), {
      target: { value: "180" },
    });
    expect(setAutoReadEnabled).toHaveBeenCalledWith(false);
    expect(setBackgroundSyncEnabled).toHaveBeenCalledWith(false);
    expect(setDefaultFetchIntervalMinutes).toHaveBeenCalledWith(180);

    fireEvent.click(screen.getByTestId("feed-management-sync-all"));
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "已略过 20 篇较早历史",
      ),
    );

    fireEvent.click(screen.getByTestId("feed-management-opml"));
    expect(screen.getByTestId("mock-feed-opml-dialog")).toBeTruthy();

    fireEvent.click(screen.getByTestId("feed-management-trash"));
    await waitFor(() => expect(screen.getByText("已删除文章")).toBeTruthy());
    fireEvent.click(screen.getByText("恢复"));
    await waitFor(() =>
      expect(feedTrashRestore).toHaveBeenCalledWith("item-1"),
    );
    fireEvent.click(screen.getByText("立即清空已删除文章"));
    await waitFor(() => expect(feedTrashClear).toHaveBeenCalledTimes(1));

    fireEvent.click(screen.getByTestId("feed-management-optimize"));
    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent("资料库空间已优化"),
    );
    fireEvent.click(screen.getByText("前往总览"));
    expect(openOverview).toHaveBeenCalledTimes(1);
  });

  it("keeps the recycle view usable when a maintenance operation fails", async () => {
    feedTrashRestore.mockRejectedValueOnce(new Error("sqlite detail"));
    render(
      <FeedManagementSection
        proxyStatusLabel="无代理"
        onOpenOverview={() => undefined}
      />,
    );
    await waitFor(() => expect(feedLibrarySummary).toHaveBeenCalled());
    fireEvent.click(screen.getByTestId("feed-management-trash"));
    await waitFor(() => expect(screen.getByText("已删除文章")).toBeTruthy());
    fireEvent.click(screen.getByText("恢复"));

    await waitFor(() =>
      expect(screen.getByRole("status")).toHaveTextContent(
        "恢复未完成，请稍后重试。",
      ),
    );
    expect(screen.getByText("已删除文章")).toBeTruthy();
  });
});
