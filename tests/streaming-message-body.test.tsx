import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { StreamingMessageBody } from "@/components/ai/StreamingMessageBody";
import { StreamingLineBudgetRefContext } from "@/lib/streaming-line-budget-ref";
import type { StreamingLineBudget } from "@/lib/streaming-line-fit";
import * as streamingMarkdownSplitter from "@/lib/streaming-markdown-splitter";

describe("StreamingMessageBody", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
    vi.restoreAllMocks();
  });

  it("appends newly stable blocks without replacing earlier rendered blocks", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(
        <StreamingMessageBody
          content={"第一段。\n\n第二段。\n\n仍在输出"}
          contentIdentity="run-1"
        />,
      );
    });
    const first = host?.querySelector("p");
    expect(first?.textContent).toBe("第一段。");

    act(() => {
      root?.render(
        <StreamingMessageBody
          content={"第一段。\n\n第二段。\n\n第三段。\n\n仍在输出"}
          contentIdentity="run-1"
        />,
      );
    });

    const paragraphs = host?.querySelectorAll("p");
    expect(paragraphs).toHaveLength(3);
    expect(paragraphs?.[0]).toBe(first);
    expect(paragraphs?.[1]?.textContent).toBe("第二段。");
    expect(paragraphs?.[2]?.textContent).toBe("第三段。");
    expect(host?.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "仍在输出",
    );
  });

  it("does not rebuild the first stable block when lexer token counts jitter", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    const splitter = vi
      .spyOn(streamingMarkdownSplitter, "splitStreamingMarkdown")
      .mockImplementation((content: string) =>
        content.includes("更多")
          ? {
              stableMarkdown: "第一段。\n\n",
              tailMarkdown: "仍在输出更多",
              stableBlockCount: 1,
            }
          : {
              stableMarkdown: "第一段。\n\n",
              tailMarkdown: "仍在输出",
              stableBlockCount: 2,
            },
      );

    act(() => {
      root?.render(
        <StreamingMessageBody
          content={"第一段。\n\n仍在输出"}
          contentIdentity="run-jitter"
        />,
      );
    });
    const first = host?.querySelector("p");
    expect(first?.textContent).toBe("第一段。");

    act(() => {
      root?.render(
        <StreamingMessageBody
          content={"第一段。\n\n仍在输出更多"}
          contentIdentity="run-jitter"
        />,
      );
    });

    expect(host?.querySelector("p")).toBe(first);
    expect(host?.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "仍在输出更多",
    );
    splitter.mockRestore();
  });

  it("publishes the tail line budget after updating visible text", () => {
    const budgetRef: { current: StreamingLineBudget | null } = {
      current: null,
    };
    const descriptor = Object.getOwnPropertyDescriptor(
      HTMLElement.prototype,
      "clientWidth",
    );
    Object.defineProperty(HTMLElement.prototype, "clientWidth", {
      configurable: true,
      get() {
        return 180;
      },
    });
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    try {
      act(() => {
        root?.render(
          <StreamingLineBudgetRefContext.Provider value={budgetRef}>
            <StreamingMessageBody
              content={"仍在输出"}
              contentIdentity="run-budget"
            />
          </StreamingLineBudgetRefContext.Provider>,
        );
      });
      expect(budgetRef.current?.lineWidthPx).toBe(180);
      expect(budgetRef.current?.remainingPx).toBeLessThanOrEqual(180);
    } finally {
      if (descriptor) {
        Object.defineProperty(HTMLElement.prototype, "clientWidth", descriptor);
      } else {
        Reflect.deleteProperty(HTMLElement.prototype, "clientWidth");
      }
    }
  });
});
