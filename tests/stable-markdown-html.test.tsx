import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { StableMarkdownHtml } from "@/components/ai/StableMarkdownHtml";

describe("StableMarkdownHtml", () => {
  let container: HTMLDivElement;
  let root: Root;

  function render(html: string): HTMLDivElement {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(<StableMarkdownHtml html={html} className="ai-message-body" />);
    });
    return container;
  }

  function rerender(html: string): void {
    act(() => {
      root.render(<StableMarkdownHtml html={html} className="ai-message-body" />);
    });
  }

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("preserves node identity for unchanged blocks across streaming flushes", () => {
    const target = render("<p id='a'>stable</p><p id='b'>old tail</p>");
    const first = target.querySelector("#a");
    const tail = target.querySelector("#b");
    expect(first?.textContent).toBe("stable");

    rerender("<p id='a'>stable</p><p id='b'>new tail</p>");

    expect(target.querySelector("#a")).toBe(first);
    expect(target.querySelector("#b")).toBe(tail);
    expect(tail?.textContent).toBe("new tail");
  });

  it("appends new stable blocks without recreating earlier blocks", () => {
    const target = render("<p id='a'>first</p>");
    const first = target.querySelector("#a");

    rerender("<p id='a'>first</p><pre id='b'>second</pre>");

    expect(target.querySelector("#a")).toBe(first);
    expect(target.querySelector("#b")?.textContent).toBe("second");
  });

  it("handles the initial streaming frame", () => {
    const target = render("<p id='first'>hello</p>");

    expect(target.querySelector("#first")?.textContent).toBe("hello");
  });
});
