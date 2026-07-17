import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiComposer } from "@/components/ui/ai-composer";

describe("AiComposer display mention overlay", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
  });

  it("keeps one accessible textarea over an aria-hidden, length-identical highlight layer", async () => {
    const value = "ask Guide\nnext";
    await act(async () => {
      root.render(
        <AiComposer
          value={value}
          displayMentions={[
            {
              kind: "file",
              value: "Policies/Guide.md",
              label: "Guide",
              range: { from: 4, to: 9 },
            },
          ]}
          onChange={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });

    const textarea = host.querySelector("textarea");
    const layer = host.querySelector<HTMLElement>(
      '[data-testid="ai-mention-highlight-layer"]',
    );
    const mention = layer?.querySelector(".ai-composer-display-mention");
    expect(host.querySelectorAll("textarea")).toHaveLength(1);
    expect(textarea?.getAttribute("aria-label")).toBe("AI 输入");
    expect(layer?.getAttribute("aria-hidden")).toBe("true");
    expect(layer?.textContent).toBe(value);
    expect(mention?.textContent).toBe("Guide");
    expect(mention?.getAttribute("title")).toBe("文档：Policies/Guide.md");
    expect(textarea?.className).toContain("ai-composer-textarea-with-mentions");

    textarea?.setSelectionRange(0, 0);
    act(() =>
      mention?.dispatchEvent(new MouseEvent("mousedown", { bubbles: true })),
    );
    expect(textarea?.selectionStart).toBe(9);
  });

  it("synchronizes textarea scrolling with the highlight layer", async () => {
    await act(async () => {
      root.render(
        <AiComposer
          value={`ask Guide\n${"line\n".repeat(20)}`}
          displayMentions={[
            {
              kind: "file",
              value: "Policies/Guide.md",
              label: "Guide",
              range: { from: 4, to: 9 },
            },
          ]}
          onChange={vi.fn()}
          onSubmit={vi.fn()}
        />,
      );
    });

    const textarea = host.querySelector("textarea")!;
    const layer = host.querySelector<HTMLElement>(
      '[data-testid="ai-mention-highlight-layer"]',
    )!;
    textarea.scrollTop = 24;
    textarea.scrollLeft = 3;
    act(() => textarea.dispatchEvent(new Event("scroll", { bubbles: true })));

    expect(layer.scrollTop).toBe(24);
    expect(layer.scrollLeft).toBe(3);
  });
});
