import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiMessageBubble } from "@/components/ai/AiMessageBubble";

describe("AiMessageBubble code copy", () => {
  let host: HTMLDivElement;
  let root: Root;
  let writeText: ReturnType<typeof vi.fn>;

  beforeEach(() => {
    vi.useFakeTimers();
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });

    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("copies the code block text from assistant messages", async () => {
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "assistant",
          content: "```bash\ncurl -fsSL https://example.test/install.sh\n```",
        }),
      );
    });

    const button = host.querySelector(
      "button[data-ai-code-copy]",
    ) as HTMLButtonElement | null;

    expect(button).not.toBeNull();

    await act(async () => {
      button!.dispatchEvent(new MouseEvent("click", { bubbles: true }));
    });

    expect(writeText).toHaveBeenCalledWith(
      "curl -fsSL https://example.test/install.sh",
    );
  });
});
