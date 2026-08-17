import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantAnswerReveal } from "@/components/ai/hooks/useAssistantAnswerReveal";
import type { AssistantPresentationState } from "@/lib/assistant-presentation";

function Harness({
  presentation,
}: {
  presentation: AssistantPresentationState | null;
}) {
  const { answer, revealing } = useAssistantAnswerReveal(presentation);
  return createElement(
    "output",
    {
      "data-answer": answer,
      "data-revealing": String(revealing),
    },
    answer,
  );
}

function presentationFor(
  answer: string,
  runId = "run-1",
): AssistantPresentationState {
  return {
    runId,
    lastSeq: answer.length > 0 ? 1 : 0,
    resyncFromSeq: null,
    pendingEvents: [],
    processItems: [],
    answer,
    answerComplete: false,
  };
}

describe("useAssistantAnswerReveal", () => {
  let host: HTMLDivElement;
  let root: Root;
  let frameCallbacks: Map<number, FrameRequestCallback>;
  let nextFrame: number;
  let requestFrame: ReturnType<typeof vi.spyOn>;
  let cancelFrame: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    frameCallbacks = new Map();
    nextFrame = 1;
    requestFrame = vi
      .spyOn(window, "requestAnimationFrame")
      .mockImplementation((callback) => {
        const frame = nextFrame;
        nextFrame += 1;
        frameCallbacks.set(frame, callback);
        return frame;
      });
    cancelFrame = vi
      .spyOn(window, "cancelAnimationFrame")
      .mockImplementation((frame) => {
        frameCallbacks.delete(frame);
      });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    requestFrame.mockRestore();
    cancelFrame.mockRestore();
  });

  function drainFrames() {
    while (frameCallbacks.size > 0) {
      const callbacks = Array.from(frameCallbacks.values());
      frameCallbacks.clear();
      act(() => {
        callbacks.forEach((callback) => callback(16));
      });
    }
  }

  it("releases a large answer over multiple frames instead of one commit", () => {
    act(() => {
      root.render(createElement(Harness, { presentation: null }));
    });
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe("");

    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor("1234567890"),
        }),
      );
    });

    // The authoritative answer exists but is not yet visible in the same commit.
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe("");
    expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
      "true",
    );
    expect(frameCallbacks.size).toBe(1);

    act(() => {
      frameCallbacks.get(1)?.(16);
    });
    const afterFirstFrame = host
      .querySelector("output")
      ?.getAttribute("data-answer");
    expect(afterFirstFrame?.length).toBeGreaterThan(0);
    expect(afterFirstFrame?.length).toBeLessThan(10);

    drainFrames();
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
      "1234567890",
    );
    expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
      "false",
    );
  });

  it("never splits a surrogate pair while revealing", () => {
    const emoji = "😀";
    const target = `a${emoji}b`;

    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor(target),
        }),
      );
    });

    while (frameCallbacks.size > 0) {
      const callbacks = Array.from(frameCallbacks.values());
      frameCallbacks.clear();
      act(() => {
        callbacks.forEach((callback) => callback(16));
      });
      const current = host
        .querySelector("output")
        ?.getAttribute("data-answer") as string;
      const lastCode = current.charCodeAt(current.length - 1);
      expect(
        lastCode < 0xd800 || lastCode > 0xdbff,
        `intermediate reveal ended with a high surrogate: ${current}`,
      ).toBe(true);
    }

    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
      target,
    );
  });

  it("does not reveal private reasoning markup", () => {
    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor("可见<thinking>隐藏</thinking>结尾"),
        }),
      );
    });

    drainFrames();
    const visible = host
      .querySelector("output")
      ?.getAttribute("data-answer") as string;
    expect(visible).toBe("可见结尾");
    expect(visible).not.toContain("隐藏");
  });

  it("reveals immediately when reduced motion is preferred", () => {
    const matchMedia = vi
      .fn()
      .mockReturnValue({ matches: true } as MediaQueryList);
    vi.stubGlobal("matchMedia", matchMedia);

    try {
      act(() => {
        root.render(
          createElement(Harness, {
            presentation: presentationFor("1234567890"),
          }),
        );
      });

      expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
        "1234567890",
      );
      expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
        "false",
      );
      expect(frameCallbacks.size).toBe(0);
    } finally {
      vi.unstubAllGlobals();
    }
  });

  it("resets immediately when a new run starts", () => {
    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor("old answer", "run-old"),
        }),
      );
    });
    drainFrames();
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
      "old answer",
    );

    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor("new", "run-new"),
        }),
      );
    });

    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe("");
    expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
      "true",
    );
  });
});
