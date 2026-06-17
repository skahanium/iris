import { describe, expect, it, vi } from "vitest";

import { debounce, isModKey, matchesKeyChord } from "@/lib/utils";

describe("isModKey", () => {
  it("returns true for ctrlKey", () => {
    expect(isModKey({ ctrlKey: true, metaKey: false } as KeyboardEvent)).toBe(
      true,
    );
  });

  it("returns true for metaKey", () => {
    expect(isModKey({ ctrlKey: false, metaKey: true } as KeyboardEvent)).toBe(
      true,
    );
  });

  it("returns false when neither modifier is set", () => {
    expect(isModKey({ ctrlKey: false, metaKey: false } as KeyboardEvent)).toBe(
      false,
    );
  });
});

describe("matchesKeyChord", () => {
  it("matches Ctrl+Period by physical code when WebView reports a non-literal key", () => {
    expect(
      matchesKeyChord(
        {
          key: "Process",
          code: "Period",
          ctrlKey: true,
          metaKey: false,
          shiftKey: false,
        } as KeyboardEvent,
        { key: ".", mod: true },
      ),
    ).toBe(true);
  });
});

describe("debounce", () => {
  it("flush invokes pending callback immediately", () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounce(fn, 500);
    d("a");
    expect(fn).not.toHaveBeenCalled();
    d.flush();
    expect(fn).toHaveBeenCalledWith("a");
    vi.useRealTimers();
  });

  it("cancel drops pending callback", () => {
    vi.useFakeTimers();
    const fn = vi.fn();
    const d = debounce(fn, 500);
    d("b");
    d.cancel();
    vi.advanceTimersByTime(600);
    expect(fn).not.toHaveBeenCalled();
    vi.useRealTimers();
  });
});
