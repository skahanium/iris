import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { AiMessageBubble } from "@/components/ai/AiMessageBubble";

describe("AiMessageBubble rendered HTML safety", () => {
  let host: HTMLDivElement;
  let root: Root;
  let writeText: ReturnType<typeof vi.fn>;
  let originalTrustedTypes: unknown;

  beforeEach(() => {
    vi.useFakeTimers();
    writeText = vi.fn().mockResolvedValue(undefined);
    Object.defineProperty(navigator, "clipboard", {
      configurable: true,
      value: { writeText },
    });
    originalTrustedTypes = (window as Window & { trustedTypes?: unknown })
      .trustedTypes;

    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    Object.defineProperty(window, "trustedTypes", {
      configurable: true,
      value: originalTrustedTypes,
    });
    vi.runOnlyPendingTimers();
    vi.useRealTimers();
  });

  it("renders code text without interactive controls inside assistant HTML", async () => {
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "assistant",
          content: "```bash\ncurl -fsSL https://example.test/install.sh\n```",
        }),
      );
    });

    expect(host.querySelector("button[data-ai-code-copy]")).toBeNull();
    expect(host.textContent).toContain(
      "curl -fsSL https://example.test/install.sh",
    );
    expect(writeText).not.toHaveBeenCalled();
  });

  it("renders final assistant markdown content when streaming is false", async () => {
    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "assistant",
          content: "**final answer**",
          streaming: false,
        }),
      );
    });

    expect(host.querySelector("strong")?.textContent).toBe("final answer");
  });

  it("uses a Trusted Types policy for assistant markdown HTML", async () => {
    const createHTML = vi.fn((html: string) => ({ __trustedHTML: html }));
    const createPolicy = vi.fn(() => ({ createHTML }));
    Object.defineProperty(window, "trustedTypes", {
      configurable: true,
      value: { createPolicy },
    });

    await act(async () => {
      root.render(
        createElement(AiMessageBubble, {
          role: "assistant",
          content: "**trusted**",
          streaming: false,
        }),
      );
    });

    expect(createPolicy).toHaveBeenCalledWith(
      "iris-sanitized-html",
      expect.objectContaining({ createHTML: expect.any(Function) }),
    );
    expect(createHTML).toHaveBeenCalledWith(expect.stringContaining("trusted"));
  });
});
