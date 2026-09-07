import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

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
      viewport.scrollTop = 390;
      viewport.dispatchEvent(new Event("scroll"));
    });
    expect(host.querySelector("[data-following]")?.textContent).toBe("true");
  });

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
