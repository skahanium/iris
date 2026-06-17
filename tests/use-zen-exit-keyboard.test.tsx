import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useZenExitKeyboard } from "@/hooks/useZenExitKeyboard";

function Harness({
  zen,
  onZenChange,
}: {
  zen: boolean;
  onZenChange: (updater: (zen: boolean) => boolean) => void;
}) {
  useZenExitKeyboard({ zen, setZen: onZenChange });
  return null;
}

describe("useZenExitKeyboard", () => {
  let container: HTMLDivElement;
  let root: Root;

  beforeEach(() => {
    container = document.createElement("div");
    document.body.appendChild(container);
    root = createRoot(container);
  });

  afterEach(() => {
    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("exits zen mode on Escape", () => {
    const setZen = vi.fn();

    act(() => {
      root.render(createElement(Harness, { zen: true, onZenChange: setZen }));
    });

    const event = new KeyboardEvent("keydown", {
      key: "Escape",
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(event);

    expect(setZen).toHaveBeenCalledOnce();
    expect(setZen.mock.calls[0]?.[0](true)).toBe(false);
    expect(event.defaultPrevented).toBe(true);
  });

  it("does not handle Escape when zen mode is inactive", () => {
    const setZen = vi.fn();

    act(() => {
      root.render(createElement(Harness, { zen: false, onZenChange: setZen }));
    });

    const event = new KeyboardEvent("keydown", {
      key: "Escape",
      bubbles: true,
      cancelable: true,
    });
    window.dispatchEvent(event);

    expect(setZen).not.toHaveBeenCalled();
    expect(event.defaultPrevented).toBe(false);
  });

  it("toggles zen mode on Ctrl+Period keydown", () => {
    const setZen = vi.fn();

    act(() => {
      root.render(createElement(Harness, { zen: false, onZenChange: setZen }));
    });
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: ".",
        code: "Period",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(setZen).toHaveBeenCalledOnce();
    expect(setZen.mock.calls[0]?.[0](false)).toBe(true);
  });

  it("toggles zen mode on Ctrl+Period keyup when keydown was swallowed", () => {
    const setZen = vi.fn();

    act(() => {
      root.render(createElement(Harness, { zen: false, onZenChange: setZen }));
    });
    document.dispatchEvent(
      new KeyboardEvent("keyup", {
        key: "Process",
        code: "Period",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(setZen).toHaveBeenCalledOnce();
    expect(setZen.mock.calls[0]?.[0](false)).toBe(true);
  });

  it("does not toggle twice for one Ctrl+Period press", () => {
    const setZen = vi.fn();

    act(() => {
      root.render(createElement(Harness, { zen: false, onZenChange: setZen }));
    });
    window.dispatchEvent(
      new KeyboardEvent("keydown", {
        key: ".",
        code: "Period",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );
    document.dispatchEvent(
      new KeyboardEvent("keyup", {
        key: ".",
        code: "Period",
        ctrlKey: true,
        bubbles: true,
        cancelable: true,
      }),
    );

    expect(setZen).toHaveBeenCalledOnce();
  });
});
