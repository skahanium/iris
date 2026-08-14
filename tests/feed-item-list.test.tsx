import { render, screen } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";

const { useVirtualizer } = vi.hoisted(() => ({
  useVirtualizer: vi.fn(),
}));

vi.mock("@tanstack/react-virtual", () => ({ useVirtualizer }));

import { FeedItemList } from "@/components/feed/FeedItemList";
import type { FeedItemSummary } from "@/types/ipc";

function item(overrides: Partial<FeedItemSummary> = {}): FeedItemSummary {
  return {
    rowId: 1,
    id: "item-1",
    sourceId: "source-1",
    sourceTitle: "很长的订阅来源名称很长的订阅来源名称",
    title: "很长的中文文章标题很长的中文文章标题很长的中文文章标题",
    authorName: "很长的作者名称很长的作者名称",
    canonicalUrl: "https://example.com/article",
    publishedAt: "2026-08-13T00:00:00Z",
    receivedAt: "2026-08-13T00:00:00Z",
    sortAt: "2026-08-13T00:00:00Z",
    excerpt: "很长的摘要。".repeat(80),
    isRead: false,
    isStarred: false,
    isArchived: false,
    conversionStatus: "ok",
    ...overrides,
  };
}

beforeEach(() => {
  useVirtualizer.mockReturnValue({
    getVirtualItems: () => [{ index: 0, key: "item-1", start: 0 }],
    getTotalSize: () => 96,
    measureElement: vi.fn(),
  });
});

describe("FeedItemList", () => {
  it("clips long article metadata and reserves a safe virtual row height", () => {
    render(
      <FeedItemList
        items={[item()]}
        status="ready"
        errorCode={null}
        hasMore={false}
        selectedItemId={null}
        focusedItemId={null}
        onSelect={() => undefined}
        onFocusItem={() => undefined}
        onMarkAllRead={() => undefined}
        onRetry={() => undefined}
        onLoadMore={() => undefined}
      />,
    );

    const row = screen.getByTestId("feed-item-item-1");
    const excerpt = screen.getByTestId("feed-item-excerpt-item-1");
    expect(row).toHaveClass("overflow-hidden");
    expect(excerpt).toHaveClass("line-clamp-2");
    expect(excerpt).not.toHaveClass("block");

    const options = useVirtualizer.mock.calls[0]?.[0] as {
      estimateSize: () => number;
    };
    expect(options.estimateSize()).toBeGreaterThanOrEqual(96);
  });
});
