import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { StreamingMessageBody } from "@/components/ai/StreamingMessageBody";

describe("StreamingMessageBody", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
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
});
