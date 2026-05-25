import { describe, expect, it, vi } from "vitest";

import { debounce, isModKey } from "@/lib/utils";

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
