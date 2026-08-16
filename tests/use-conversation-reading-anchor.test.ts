import { act, createElement, useRef } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import {
  readingAnchorTarget,
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

  it("includes virtual-row transforms when locating the streaming tail", () => {
    expect(
      tailBottomInScrollContent({
        viewportTop: 120,
        viewportScrollTop: 900,
        tailBottom: 780,
      }),
    ).toBe(1_560);
  });

  it("keeps short content bottom-aligned", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 880,
        clientHeight: 1_000,
        tailBottom: 760,
      }),
    ).toBe(0);
  });

  it("places the last visible streaming line in the reading zone", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 3_000,
        clientHeight: 1_000,
        tailBottom: 2_100,
      }),
    ).toBe(1_500);
  });

  it("advances as a single growing streaming paragraph adds new lines", () => {
    expect(
      readingAnchorTarget({
        scrollHeight: 3_000,
        clientHeight: 1_000,
        tailBottom: 2_500,
      }),
    ).toBe(1_900);
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
