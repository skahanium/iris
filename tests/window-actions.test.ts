import { describe, expect, it, vi } from "vitest";

import {
  toggleNativeFullscreen,
  toggleWindowMaximize,
} from "@/lib/window-actions";

describe("window actions", () => {
  it("toggles native fullscreen from the current fullscreen state", async () => {
    const win = {
      isFullscreen: vi.fn(() => Promise.resolve(false)),
      setFullscreen: vi.fn(() => Promise.resolve()),
    };

    await expect(toggleNativeFullscreen(win)).resolves.toBe(true);

    expect(win.isFullscreen).toHaveBeenCalledTimes(1);
    expect(win.setFullscreen).toHaveBeenCalledWith(true);
  });

  it("does not own macOS chrome transitions from frontend window actions", () => {
    expect(toggleNativeFullscreen.toString()).not.toContain("setDecorations");
    expect(toggleNativeFullscreen.toString()).not.toContain("setTitleBarStyle");
  });

  it("keeps maximize as a separate window action", async () => {
    const win = {
      toggleMaximize: vi.fn(() => Promise.resolve()),
    };

    await toggleWindowMaximize(win);

    expect(win.toggleMaximize).toHaveBeenCalledTimes(1);
  });
});
