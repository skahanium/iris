import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";

import { FeedSidebar } from "@/components/feed/FeedSidebar";
import type { FeedSourceSummary } from "@/types/ipc";

function source(
  id: string,
  title: string,
  folderPath: string,
): FeedSourceSummary {
  return {
    id,
    title,
    feedUrl: `https://example.com/${id}.xml`,
    siteUrl: null,
    folderPath,
    isEnabled: true,
    fetchIntervalMinutes: 60,
    fulltextEnabled: true,
    unreadCount: 0,
    lastCheckedAt: null,
    lastSuccessAt: null,
    nextFetchAt: null,
    consecutiveFailures: 0,
    lastErrorCode: null,
  };
}

afterEach(cleanup);

describe("FeedSidebar 来源分组", () => {
  it("按 folderPath 分区且保留未分组来源", () => {
    render(
      <FeedSidebar
        sources={[
          source("root", "根来源", ""),
          source("rust", "Rust 周刊", "技术/Rust"),
          source("web", "Web 周刊", "技术/Web"),
        ]}
        view="all"
        sourceId={null}
        onViewChange={vi.fn()}
        onSourceSelect={vi.fn()}
        onClearSource={vi.fn()}
        onAddSource={vi.fn()}
        onRetrySource={vi.fn()}
        onOpenOpml={vi.fn()}
      />,
    );

    expect(screen.getByTestId("feed-source-group-ungrouped")).toHaveTextContent(
      "根来源",
    );
    expect(screen.getByTestId("feed-source-group-技术-Rust")).toHaveTextContent(
      "Rust 周刊",
    );
    expect(screen.getByTestId("feed-source-group-技术-Web")).toHaveTextContent(
      "Web 周刊",
    );
  });
});
