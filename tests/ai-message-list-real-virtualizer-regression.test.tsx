import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiMessageList } from "@/components/ai/AiMessageList";

describe("AiMessageList real virtualizer regression", () => {
  let host: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    host = document.createElement("div");
    host.style.height = "640px";
    host.style.width = "420px";
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  it("does not enter a nested update loop while streaming a non-empty assistant message", async () => {
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);

    await act(async () => {
      root.render(
        <AiMessageList
          messages={[{ role: "assistant", content: "第一段" }]}
          streaming={true}
        />,
      );
    });

    for (const content of [
      "第一段\n\n第二段",
      "第一段\n\n第二段\n\n```ts\nconst x = 1;\n```",
      "第一段\n\n第二段\n\n```ts\nconst x = 1;\n```\n\n| A | B |\n| - | - |\n| 1 | 2 |",
    ]) {
      await act(async () => {
        root.render(
          <AiMessageList
            messages={[{ role: "assistant", content }]}
            streaming={true}
          />,
        );
      });
    }

    expect(
      consoleError.mock.calls
        .flat()
        .some((entry) =>
          String(entry).includes("Maximum update depth exceeded"),
        ),
    ).toBe(false);
  });
});
