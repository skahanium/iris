import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { StableMarkdownHtml } from "@/components/ai/StableMarkdownHtml";

describe("StableMarkdownHtml content identity", () => {
  let container: HTMLDivElement;
  let root: Root;

  function render(html: string, contentIdentity: string): HTMLDivElement {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
    act(() => {
      root.render(
        <StableMarkdownHtml
          html={html}
          contentIdentity={contentIdentity}
          className="ai-message-body"
        />,
      );
    });
    return container;
  }

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("resets the DOM instead of morphing when message identity changes", () => {
    const target = render("<p id='body'>old message</p>", "message-a");
    const oldNode = target.querySelector("#body");
    expect(oldNode).not.toBeNull();

    act(() => {
      root.render(
        <StableMarkdownHtml
          html="<p id='body'>new message</p>"
          contentIdentity="message-b"
          className="ai-message-body"
        />,
      );
    });

    const newNode = target.querySelector("#body");
    expect(newNode).not.toBeNull();
    expect(newNode).not.toBe(oldNode);
    expect(newNode?.textContent).toBe("new message");
  });
});
