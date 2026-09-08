import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import {
  readingAnchorTarget,
  conversationFollowTarget,
  conversationFollowStreamKey,
  CONVERSATION_PARK_PADDING_PX,
  tailBottomInScrollContent,
  useConversationReadingAnchor,
} from "@/components/ai/hooks/useConversationReadingAnchor";

function ReadingAnchorHarness({ streamKey }: { streamKey: string }) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const { following } = useConversationReadingAnchor({
    viewportRef,
    active: true,
    revision: 1,
    streamKey,
  });

  return createElement(
    "div",
    { ref: viewportRef },
    createElement("div", { "data-streaming-tail": "" }),
    createElement("output", { "data-following": "" }, String(following)),
  );
}

describe("readingAnchorTarget", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
  });

  it("starts following again for a new assistant answer", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" }));
    });
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
    });

    act(() => {
      viewport.scrollTop = 150;
      viewport.dispatchEvent(new Event("scroll"));
      viewport.scrollTop = 120;
      viewport.dispatchEvent(new Event("scroll"));
    });
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-2" }));
    });
    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("keeps following when the user scrolls downward", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" }));
    });
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
    });

    act(() => {
      viewport.scrollTop = 150;
      viewport.dispatchEvent(new Event("scroll"));
    });

    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("keeps following on small upward jitter near the bottom", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" }));
    });
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
    });

    act(() => {
      viewport.scrollTop = 390;
      viewport.dispatchEvent(new Event("scroll"));
      viewport.scrollTop = 380;
      viewport.dispatchEvent(new Event("scroll"));
    });

    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("keeps following on small upward jitter while parked away from the canvas bottom", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" }));
    });
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
    });

    act(() => {
      viewport.scrollTop = 150;
      viewport.dispatchEvent(new Event("scroll"));
      viewport.scrollTop = 140;
      viewport.dispatchEvent(new Event("scroll"));
    });

    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("detaches only after scrolling clearly above the bottom", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" }));
    });
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
    });

    act(() => {
      viewport.scrollTop = 390;
      viewport.dispatchEvent(new Event("scroll"));
      viewport.scrollTop = 300;
      viewport.dispatchEvent(new Event("scroll"));
    });

    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
  });

  it("resumes following when the user returns near the bottom", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    act(() => {
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" }));
    });
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { configurable: true, value: 100 },
      scrollHeight: { configurable: true, value: 500 },
    });

    act(() => {
      viewport.scrollTop = 390;
      viewport.dispatchEvent(new Event("scroll"));
      viewport.scrollTop = 300;
      viewport.dispatchEvent(new Event("scroll"));
    });
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");

    act(() => {
      viewport.dispatchEvent(new WheelEvent("wheel", { deltaY: 90 }));
      viewport.scrollTop = 390;
      viewport.dispatchEvent(new Event("scroll"));
    });
    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("detaches on wheel intent before scroll geometry changes", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" })),
    );
    act(() =>
      host?.firstElementChild?.dispatchEvent(
        new WheelEvent("wheel", { deltaY: -1 }),
      ),
    );
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
  });

  it("accumulates repeated ten-pixel upward scrolls", () => {
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() =>
      root?.render(createElement(ReadingAnchorHarness, { streamKey: "run-1" })),
    );
    const viewport = host.firstElementChild as HTMLDivElement;
    Object.defineProperties(viewport, {
      clientHeight: { value: 100 },
      scrollHeight: { value: 1000 },
    });
    act(() => {
      for (const top of [500, 490, 480, 470]) {
        viewport.scrollTop = top;
        viewport.dispatchEvent(new Event("scroll"));
      }
    });
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
  });

  it.each(["End", "PageDown", "ArrowDown", " "])(
    "resumes at the bottom after %s keyboard intent",
    (key) => {
      host = document.createElement("div");
      document.body.append(host);
      root = createRoot(host);
      act(() =>
        root?.render(
          createElement(ReadingAnchorHarness, { streamKey: "run-1" }),
        ),
      );
      const viewport = host.firstElementChild as HTMLDivElement;
      Object.defineProperties(viewport, {
        clientHeight: { value: 100 },
        scrollHeight: { value: 500 },
      });
      act(() => {
        viewport.dispatchEvent(new WheelEvent("wheel", { deltaY: -10 }));
        viewport.scrollTop = 100;
        viewport.dispatchEvent(new Event("scroll"));
      });
      act(() => {
        viewport.dispatchEvent(new KeyboardEvent("keydown", { key }));
        viewport.scrollTop = 200;
        viewport.dispatchEvent(new Event("scroll"));
        viewport.scrollTop = 400;
        viewport.dispatchEvent(new Event("scroll"));
      });
      expect(host.querySelector("[data-following]")?.textContent).toBe("true");
    },
  );

  it("includes virtual-row transforms when locating the streaming tail", () => {
    expect(
      tailBottomInScrollContent({
        viewportTop: 120,
        viewportScrollTop: 900,
        tailBottom: 780,
      }),
    ).toBe(1_560);
  });

  it("keeps short content at the top when it cannot scroll", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 880,
        clientHeight: 1_000,
        tailBottom: 760,
      }),
    ).toBe(0);
  });

  it("follows the scroll-content bottom so the reserved spacer stays visible", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 3_000,
        clientHeight: 1_000,
        tailBottom: 2_100,
      }),
    ).toBe(2_000);
  });

  it("advances as the streaming answer grows", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 3_000,
        clientHeight: 1_000,
        tailBottom: 2_500,
      }),
    ).toBe(2_000);
  });

  it("clamps the reading anchor to the scrollable range", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 1_400,
        clientHeight: 1_000,
        tailBottom: 2_100,
      }),
    ).toBe(400);
  });
});

describe("conversationFollowTarget", () => {
  it("does not move the canvas while the live block still fits in the viewport", () => {
    expect(
      conversationFollowTarget({
        scrollTop: 200,
        scrollHeight: 2_000,
        clientHeight: 800,
        liveBottom: 900,
        parkTop: 220,
        shouldPark: false,
      }),
    ).toEqual({ scrollTop: 200, changed: false });
  });

  it("scrolls only by the overflow once the live block hits the bottom inset", () => {
    expect(
      conversationFollowTarget({
        scrollTop: 200,
        scrollHeight: 2_000,
        clientHeight: 800,
        liveBottom: 944,
        parkTop: 220,
        shouldPark: false,
      }),
    ).toEqual({ scrollTop: 240, changed: true });
  });

  it("parks the outgoing user message near the top of the viewport", () => {
    expect(
      conversationFollowTarget({
        scrollTop: 1_800,
        scrollHeight: 3_000,
        clientHeight: 800,
        liveBottom: 2_100,
        parkTop: 1_640,
        shouldPark: true,
      }),
    ).toEqual({
      scrollTop: 1_640 - CONVERSATION_PARK_PADDING_PX,
      changed: true,
    });
  });

  it("keeps short conversations at the top", () => {
    expect(
      conversationFollowTarget({
        scrollTop: 0,
        scrollHeight: 400,
        clientHeight: 800,
        liveBottom: 240,
        parkTop: 12,
        shouldPark: true,
      }),
    ).toEqual({ scrollTop: 0, changed: false });
  });
});

describe("conversationFollowStreamKey", () => {
  it("keeps the parked turn identity when the user row later gains a runId", () => {
    expect(
      conversationFollowStreamKey({
        live: true,
        userClientRequestId: "req-1",
        userRunId: undefined,
        activeStreamKey: "run:pending|assistant|",
      }),
    ).toBe("req-1");
    expect(
      conversationFollowStreamKey({
        live: true,
        userClientRequestId: "req-1",
        userRunId: "run-1",
        activeStreamKey: "run:run-1|assistant|turn-1",
      }),
    ).toBe("req-1");
  });

  it("falls back to the live assistant key when no user row exists", () => {
    expect(
      conversationFollowStreamKey({
        live: true,
        activeStreamKey: "run:run-1|assistant|",
      }),
    ).toBe("run:run-1|assistant|");
  });
});

function GeometryHarness({ active = true }: { active?: boolean }) {
  const viewportRef = useRef<HTMLDivElement>(null);
  const { following, returnToLatest } = useConversationReadingAnchor({
    viewportRef,
    active,
    revision: 1,
    streamKey: active ? "request-1" : null,
  });
  return createElement(
    "div",
    { ref: viewportRef, "data-viewport": "" },
    createElement(
      "div",
      { "data-conversation-content": "" },
      createElement("div", {
        "data-conversation-row": "0",
        "data-conversation-park": "",
      }),
      createElement(
        "div",
        { "data-conversation-row": "1", "data-live-stream": "" },
        createElement("p", { "data-markdown-block": "0" }, "阅读锚点"),
      ),
    ),
    createElement("div", {
      "data-conversation-spacer": "",
      style: { height: 96 },
    }),
    createElement("button", { onClick: returnToLatest }, "latest"),
    createElement("output", { "data-following": "" }, String(following)),
  );
}

describe("conversation geometry ownership", () => {
  let host: HTMLDivElement;
  let root: Root;
  let viewport: HTMLDivElement;
  let bottom: number;
  let blockTop: number;
  let time: number;
  let nextFrame: number;
  let frames: Map<number, FrameRequestCallback>;
  let resize: () => void;
  let disconnect: ReturnType<typeof vi.fn>;
  const rect = (top: number, height: number) => ({
    top,
    bottom: top + height,
    left: 0,
    right: 420,
    width: 420,
    height,
    x: 0,
    y: top,
    toJSON() {},
  });
  const flush = (count = 40) => {
    for (let index = 0; index < count; index += 1) {
      const pending = [...frames.values()];
      frames.clear();
      time += 16;
      act(() => pending.forEach((callback) => callback(time)));
    }
  };
  beforeEach(() => {
    bottom = 1120;
    blockTop = 1050;
    time = 0;
    nextFrame = 0;
    frames = new Map();
    disconnect = vi.fn();
    vi.stubGlobal("requestAnimationFrame", (callback: FrameRequestCallback) => {
      frames.set(++nextFrame, callback);
      return nextFrame;
    });
    vi.stubGlobal("cancelAnimationFrame", (id: number) => frames.delete(id));
    vi.stubGlobal(
      "ResizeObserver",
      class {
        constructor(callback: () => void) {
          resize = callback;
        }
        observe() {}
        disconnect = disconnect;
      },
    );
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => root.render(createElement(GeometryHarness)));
    viewport = host.querySelector("[data-viewport]")!;
    const spacer = host.querySelector<HTMLElement>(
      "[data-conversation-spacer]",
    )!;
    Object.defineProperties(viewport, {
      clientHeight: { value: 600 },
      scrollHeight: {
        get: () => bottom + Number.parseFloat(spacer.style.height),
      },
    });
    viewport.getBoundingClientRect = () => rect(0, 600);
    host.querySelector<HTMLElement>(
      "[data-conversation-content]",
    )!.getBoundingClientRect = () => rect(-viewport.scrollTop, bottom);
    host.querySelector<HTMLElement>(
      "[data-live-stream]",
    )!.getBoundingClientRect = () =>
      rect(blockTop - viewport.scrollTop, bottom - blockTop);
    host.querySelector<HTMLElement>(
      "[data-conversation-park]",
    )!.getBoundingClientRect = () => rect(1000 - viewport.scrollTop, 50);
    host.querySelector<HTMLElement>(
      "[data-markdown-block]",
    )!.getBoundingClientRect = () =>
      rect(blockTop - viewport.scrollTop, bottom - blockTop);
    spacer.getBoundingClientRect = () =>
      rect(bottom - viewport.scrollTop, Number.parseFloat(spacer.style.height));
  });
  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.unstubAllGlobals();
  });

  it("creates park space and consumes it as locally growing content fills the viewport", () => {
    flush();
    expect(viewport.scrollTop).toBeCloseTo(988, 0);
    expect(
      host.querySelector<HTMLElement>("[data-conversation-spacer]")?.style
        .height,
    ).toBe("468px");
    bottom = 1300;
    act(() => resize());
    flush();
    expect(viewport.scrollTop).toBeCloseTo(988, 0);
    expect(
      host.querySelector<HTMLElement>("[data-conversation-spacer]")?.style
        .height,
    ).toBe("288px");
    bottom = 1600;
    act(() => resize());
    flush();
    expect(viewport.scrollTop).toBeCloseTo(1096, 0);
  });

  it("settles final geometry after playback becomes inactive and follows later resource growth", () => {
    flush();
    expect(viewport.scrollTop).toBeCloseTo(988, 0);
    bottom = 2200;
    act(() => {
      root.render(createElement(GeometryHarness, { active: false }));
      resize();
    });
    flush();
    expect(viewport.scrollTop).toBeCloseTo(1696, 0);
    bottom = 2500;
    act(() => resize());
    flush();
    expect(viewport.scrollTop).toBeCloseTo(1996, 0);
    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("performs final overflow settlement after an in-flight park finishes", () => {
    flush(1);
    bottom = 2200;
    act(() => {
      root.render(createElement(GeometryHarness, { active: false }));
      resize();
    });
    flush(80);
    expect(viewport.scrollTop).toBeCloseTo(1696, 0);
  });

  it("preserves detached reading when final geometry arrives after playback becomes inactive", () => {
    flush();
    act(() => {
      viewport.dispatchEvent(new WheelEvent("wheel", { deltaY: -10 }));
      viewport.scrollTop = 500;
      viewport.dispatchEvent(new Event("scroll"));
    });
    bottom = 2200;
    blockTop += 100;
    act(() => {
      root.render(createElement(GeometryHarness, { active: false }));
      resize();
    });
    flush();
    expect(viewport.scrollTop).toBeCloseTo(600, 0);
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
  });

  it("compensates delayed layout changes while detached without reattaching", () => {
    flush();
    act(() => {
      viewport.dispatchEvent(new WheelEvent("wheel", { deltaY: -10 }));
      viewport.scrollTop = 500;
      viewport.dispatchEvent(new Event("scroll"));
    });
    blockTop += 150;
    bottom += 150;
    act(() => resize());
    flush();
    expect(viewport.scrollTop).toBeCloseTo(650, 0);
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
    act(() => viewport.dispatchEvent(new Event("scroll")));
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
  });

  it("recovers the same source anchor when a block window remounts", () => {
    flush();
    act(() => {
      viewport.dispatchEvent(new WheelEvent("wheel", { deltaY: -10 }));
      viewport.scrollTop = 500;
      viewport.dispatchEvent(new Event("scroll"));
    });
    const original = host.querySelector<HTMLElement>("[data-markdown-block]")!;
    const replacement = document.createElement("p");
    replacement.setAttribute("data-markdown-block", "0");
    replacement.textContent = "阅读锚点";
    original.replaceWith(replacement);
    blockTop += 150;
    bottom += 150;
    replacement.getBoundingClientRect = () =>
      rect(blockTop - viewport.scrollTop, bottom - blockTop);
    act(() => resize());
    flush();
    expect(viewport.scrollTop).toBeCloseTo(650, 0);
  });

  it("does not detach when a programmatic target is clamped by the browser", () => {
    let actualTop = 0;
    Object.defineProperty(viewport, "scrollTop", {
      configurable: true,
      get: () => actualTop,
      set: (target: number) => {
        actualTop = Math.min(100, target);
      },
    });
    flush(1);
    expect(actualTop).toBe(100);
    act(() => viewport.dispatchEvent(new Event("scroll")));
    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
    act(() => viewport.dispatchEvent(new WheelEvent("wheel", { deltaY: -1 })));
    expect(host.querySelector("[data-following]")?.textContent).toBe("false");
  });

  it("does not mistake a geometry shrink clamp for upward user scrolling", () => {
    flush();
    bottom = 2200;
    act(() => resize());
    flush();
    act(() => viewport.dispatchEvent(new Event("scroll")));
    expect(viewport.scrollTop).toBeCloseTo(1696, 0);
    bottom = 1400;
    act(() => {
      viewport.scrollTop = viewport.scrollHeight - viewport.clientHeight;
      viewport.dispatchEvent(new Event("scroll"));
      resize();
    });
    flush();
    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

  it("coalesces geometry notifications and cancels its outstanding animation on cleanup", () => {
    const geometry = vi.fn();
    viewport.addEventListener("iris-conversation-geometry", geometry);
    act(() => {
      resize();
      resize();
      resize();
    });
    expect(frames.size).toBe(1);
    flush(1);
    expect(geometry).toHaveBeenCalledTimes(1);
    act(() => root.unmount());
    expect(frames.size).toBe(0);
    expect(disconnect).toHaveBeenCalled();
  });
});
