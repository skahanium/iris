import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

const workerState = vi.hoisted(() => ({
  value: {
    failed: false,
    html: null as string | null,
    pending: true,
  },
}));

const renderMarkdownWithProfileMock = vi.hoisted(() =>
  vi.fn(() => ({ output: "<p>sync-rendered</p>" })),
);

vi.mock("@/hooks/useMarkdownRenderWorker", () => ({
  useMarkdownRenderWorker: () => workerState.value,
}));

vi.mock("@/lib/markdown-contract", () => ({
  renderMarkdownWithProfile: renderMarkdownWithProfileMock,
}));

import { AiMessageBubble } from "@/components/ai/AiMessageBubble";

describe("AiMessageBubble Markdown rendering", () => {
  let container: HTMLDivElement;
  let root: Root;

  function renderBubble(props: { content: string; streaming: boolean }): void {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);

    act(() => {
      root.render(
        <AiMessageBubble
          role="assistant"
          content={props.content}
          streaming={props.streaming}
        />,
      );
    });
  }

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
    renderMarkdownWithProfileMock.mockClear();
    workerState.value = {
      failed: false,
      html: null,
      pending: true,
    };
  });

  it("renders the first streaming frame in the isolated tail without waiting for a worker", () => {
    workerState.value = {
      failed: false,
      html: null,
      pending: true,
    };

    renderBubble({ content: "**streaming**", streaming: true });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "**streaming**",
    );
  });

  it("shows the next streaming delta without an additional content throttle", () => {
    const now = vi.spyOn(performance, "now");
    now.mockReturnValueOnce(100).mockReturnValueOnce(101);
    renderBubble({ content: "第", streaming: true });

    act(() => {
      root.render(
        <AiMessageBubble role="assistant" content="第一" streaming />,
      );
    });

    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "第一",
    );
    now.mockRestore();
  });

  it("does not reuse stale worker HTML for a different streaming frame", () => {
    workerState.value = {
      failed: false,
      html: "<p>previous-worker-render</p>",
      pending: true,
    };

    renderBubble({ content: "**streaming**", streaming: true });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
    expect(container.innerHTML).not.toContain("previous-worker-render");
    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "**streaming**",
    );
  });

  it("keeps a long streaming first frame in one tail node", () => {
    workerState.value = {
      failed: false,
      html: null,
      pending: true,
    };

    renderBubble({ content: "L".repeat(90_000), streaming: true });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
    expect(container.querySelectorAll("[data-streaming-tail]")).toHaveLength(1);
  });

  it("renders finalized assistant history synchronously without a placeholder", () => {
    workerState.value = { failed: false, html: null, pending: false };

    renderBubble({ content: "**final**", streaming: false });

    expect(renderMarkdownWithProfileMock).toHaveBeenCalled();
    expect(container.textContent).toContain("sync-rendered");
  });

  it("does not depend on worker failure state while streaming", () => {
    workerState.value = {
      failed: true,
      html: null,
      pending: false,
    };

    renderBubble({ content: "**fallback**", streaming: true });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "**fallback**",
    );
  });
});
