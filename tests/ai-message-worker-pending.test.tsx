import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

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
  });

  it("renders the first streaming frame as semantic Markdown without a worker", () => {
    renderBubble({ content: "**streaming**", streaming: true });

    expect(
      container.querySelector("[data-ai-streaming-markdown]"),
    ).not.toBeNull();
    expect(container.querySelector("strong")?.textContent).toBe("streaming");
    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "streaming",
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

  it("replaces a previous streaming frame instead of keeping stale HTML", () => {
    renderBubble({ content: "previous-frame", streaming: true });
    expect(container.textContent).toContain("previous-frame");

    act(() => {
      root.render(
        <AiMessageBubble role="assistant" content="**streaming**" streaming />,
      );
    });

    expect(container.textContent).not.toContain("previous-frame");
    expect(container.querySelector("strong")?.textContent).toBe("streaming");
    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "streaming",
    );
  });

  it("keeps a long streaming first frame in one tail node", () => {
    renderBubble({ content: "L".repeat(90_000), streaming: true });

    expect(container.querySelectorAll("[data-streaming-tail]")).toHaveLength(1);
  });

  it("renders finalized assistant history synchronously without a placeholder", () => {
    renderBubble({ content: "**final**", streaming: false });

    expect(container.querySelector("strong")?.textContent).toBe("final");
    expect(container.querySelector("[data-streaming-tail]")).toBeNull();
  });

  it("does not depend on a Markdown worker while streaming", () => {
    renderBubble({ content: "**fallback**", streaming: true });

    expect(container.querySelector("strong")?.textContent).toBe("fallback");
    expect(container.querySelector("[data-streaming-tail]")?.textContent).toBe(
      "fallback",
    );
  });
});
