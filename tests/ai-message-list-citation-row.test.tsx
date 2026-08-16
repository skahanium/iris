import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { AiMessageList } from "@/components/ai/AiMessageList";

vi.mock("@tanstack/react-virtual", () => ({
  useVirtualizer: ({ count }: { count: number }) => ({
    getTotalSize: () => count * 96,
    getVirtualItems: () =>
      Array.from({ length: count }, (_, index) => ({
        index,
        start: index * 96,
      })),
    measureElement: vi.fn(),
  }),
}));

describe("AiMessageList citation rows", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
  });

  it("renders finalized citations in a separate virtual row", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(
        <AiMessageList
          streaming={false}
          messages={[
            {
              role: "assistant",
              content: "带来源的回答。",
              sourceSummary: [{ category: "web", count: 1 }],
            },
          ]}
        />,
      );
    });

    const citationRow = host.querySelector("[data-row-kind='citations']");
    expect(citationRow).not.toBeNull();
    expect(
      citationRow?.querySelector(".assistant-citation-footer"),
    ).not.toBeNull();
    expect(citationRow?.closest(".ai-message-bubble")).toBeNull();
  });
});
