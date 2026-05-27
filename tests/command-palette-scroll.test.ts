import { describe, expect, it } from "vitest";

import { ensureOptionVisible } from "@/lib/command-palette-scroll";

function mockViewport(scrollTop = 0, clientHeight = 200) {
  const el = document.createElement("div");
  let top = scrollTop;
  Object.defineProperty(el, "scrollTop", {
    get: () => top,
    set: (v: number) => {
      top = v;
    },
    configurable: true,
  });
  Object.defineProperty(el, "clientHeight", { value: clientHeight });
  el.getBoundingClientRect = () =>
    ({
      top: 0,
      bottom: clientHeight,
      left: 0,
      right: 300,
      width: 300,
      height: clientHeight,
      x: 0,
      y: 0,
      toJSON: () => ({}),
    }) as DOMRect;
  return el;
}

describe("ensureOptionVisible", () => {
  it("scrolls down only by the clipped amount below the viewport", () => {
    const viewport = mockViewport(0, 100);
    const item = document.createElement("button");
    item.getBoundingClientRect = () =>
      ({
        top: 110,
        bottom: 140,
        left: 0,
        right: 300,
        width: 300,
        height: 30,
        x: 0,
        y: 110,
        toJSON: () => ({}),
      }) as DOMRect;

    ensureOptionVisible(viewport, item, 1);
    expect(viewport.scrollTop).toBe(48);
  });

  it("does not scroll when the item is already fully visible", () => {
    const viewport = mockViewport(0, 100);
    const item = document.createElement("button");
    item.getBoundingClientRect = () =>
      ({
        top: 60,
        bottom: 90,
        left: 0,
        right: 300,
        width: 300,
        height: 30,
        x: 0,
        y: 60,
        toJSON: () => ({}),
      }) as DOMRect;

    ensureOptionVisible(viewport, item, 1);
    expect(viewport.scrollTop).toBe(0);
  });
});
