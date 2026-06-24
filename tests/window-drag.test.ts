import { describe, expect, it, vi } from "vitest";

import { createWindowDragMouseDown } from "@/lib/window-drag";

function makeWindow() {
  return {
    startDragging: vi.fn(() => Promise.resolve()),
    toggleMaximize: vi.fn(() => Promise.resolve()),
  };
}

function makeMouseDownEvent({
  detail,
  target,
}: {
  detail: number;
  target: HTMLElement;
}) {
  return {
    button: 0,
    detail,
    preventDefault: vi.fn(),
    target,
  } as unknown as Parameters<ReturnType<typeof createWindowDragMouseDown>>[0];
}

describe("window drag mouse handling", () => {
  it("uses titlebar double-click for maximize and restore", () => {
    const win = makeWindow();
    const titlebar = document.createElement("header");
    const onMouseDown = createWindowDragMouseDown(
      win as unknown as Parameters<typeof createWindowDragMouseDown>[0],
    );

    onMouseDown(makeMouseDownEvent({ detail: 2, target: titlebar }));

    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
    expect(win.startDragging).not.toHaveBeenCalled();
  });

  it("does not maximize when double-clicking an excluded titlebar control", () => {
    const win = makeWindow();
    const button = document.createElement("button");
    button.dataset.tauriDragRegionExclude = "";
    const onMouseDown = createWindowDragMouseDown(
      win as unknown as Parameters<typeof createWindowDragMouseDown>[0],
    );

    onMouseDown(makeMouseDownEvent({ detail: 2, target: button }));

    expect(win.toggleMaximize).not.toHaveBeenCalled();
    expect(win.startDragging).not.toHaveBeenCalled();
  });
});
