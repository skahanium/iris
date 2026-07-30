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

describe("AiMessageBubble markdown worker pending behavior", () => {
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

  it("renders first streaming frame synchronously while worker output is pending", () => {
    workerState.value = {
      failed: false,
      html: null,
      pending: true,
    };

    renderBubble({ content: "**streaming**", streaming: true });

    expect(renderMarkdownWithProfileMock).toHaveBeenCalledWith(
      "**streaming**",
      "chat_assistant",
      { streaming: true },
    );
    expect(container.innerHTML).toContain("sync-rendered");
  });

  it("keeps previous worker html while a later streaming render is pending", () => {
    workerState.value = {
      failed: false,
      html: "<p>previous-worker-render</p>",
      pending: true,
    };

    renderBubble({ content: "**streaming**", streaming: true });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
    expect(container.innerHTML).toContain("previous-worker-render");
  });

  it("does not synchronously render a long streaming first frame while worker output is pending", () => {
    workerState.value = {
      failed: false,
      html: null,
      pending: true,
    };

    renderBubble({ content: "L".repeat(90_000), streaming: true });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
  });

  it("keeps final assistant history off the main-thread renderer while worker output is pending", () => {
    renderBubble({ content: "**final**", streaming: false });

    expect(renderMarkdownWithProfileMock).not.toHaveBeenCalled();
    expect(container.textContent).toContain("正在渲染回答…");
  });

  it("falls back to synchronous rendering when the streaming worker failed", () => {
    workerState.value = {
      failed: true,
      html: null,
      pending: false,
    };

    renderBubble({ content: "**fallback**", streaming: true });

    expect(renderMarkdownWithProfileMock).toHaveBeenCalledWith(
      "**fallback**",
      "chat_assistant",
      { streaming: true },
    );
    expect(container.innerHTML).toContain("sync-rendered");
  });
});
