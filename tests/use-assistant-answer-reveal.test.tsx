import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAssistantAnswerReveal } from "@/components/ai/hooks/useAssistantAnswerReveal";
import type { AssistantPresentationState } from "@/lib/assistant-presentation";
import type { StreamingLineBudget } from "@/lib/streaming-line-fit";

function Harness({
  presentation,
  getLineBudget,
}: {
  presentation: AssistantPresentationState | null;
  getLineBudget?: () => StreamingLineBudget | null;
}) {
  const { runId, answer, revealing } = useAssistantAnswerReveal(
    presentation,
    getLineBudget,
  );
  return createElement(
    "output",
    {
      "data-run-id": runId ?? "",
      "data-answer": answer,
      "data-revealing": String(revealing),
    },
    answer,
  );
}

function presentationFor(
  answer: string,
  runId = "run-1",
  resetEpoch = 0,
): AssistantPresentationState {
  return {
    runId,
    lastSeq: answer.length > 0 ? 1 : 0,
    resyncFromSeq: null,
    pendingEvents: [],
    processItems: [],
    answer,
    answerComplete: false,
    resetEpoch,
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

  it("aligns a short increment immediately without a typewriter", () => {
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
  });

  it("releases a large answer over a few frames instead of one commit", () => {
    const large = "x".repeat(200);
    act(() => {
      root.render(createElement(Harness, { presentation: null }));
    });
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe("");

    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor(large),
        }),
      );
    });

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
    expect(afterFirstFrame?.length).toBeLessThan(large.length);

    drainFrames();
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
      large,
    );
    expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
      "false",
    );
  });

  it("releases only as much text as the current line budget allows", () => {
    const large = "x".repeat(40);
    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor(large),
          getLineBudget: () => ({
            remainingPx: 8,
            lineWidthPx: 240,
            font: "14px sans-serif",
          }),
        }),
      );
    });

    expect(frameCallbacks.size).toBe(1);
    act(() => {
      frameCallbacks.get(1)?.(16);
    });
    const afterFirstFrame = host
      .querySelector("output")
      ?.getAttribute("data-answer");
    expect(afterFirstFrame?.length).toBeGreaterThan(0);
    expect(afterFirstFrame?.length).toBeLessThanOrEqual(2);
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
    expect(host.querySelector("output")?.getAttribute("data-run-id")).toBe(
      "run-old",
    );
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

    expect(host.querySelector("output")?.getAttribute("data-run-id")).toBe(
      "run-new",
    );
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
      "new",
    );
    expect(host.querySelector("output")?.getAttribute("data-answer")).not.toBe(
      "old answer",
    );
    expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
      "false",
    );
  });

  it("clears immediately when resetEpoch changes even if the target grows", () => {
    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor("old", "run-same", 0),
        }),
      );
    });
    drainFrames();
    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe(
      "old",
    );

    act(() => {
      root.render(
        createElement(Harness, {
          presentation: presentationFor("x".repeat(200), "run-same", 1),
        }),
      );
    });

    expect(host.querySelector("output")?.getAttribute("data-answer")).toBe("");
    expect(host.querySelector("output")?.getAttribute("data-revealing")).toBe(
      "true",
    );
  });
});
