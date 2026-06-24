import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const windowControlsMocks = vi.hoisted(() => ({
  close: vi.fn(() => Promise.resolve()),
  isFullscreen: vi.fn(() => Promise.resolve(false)),
  isMaximized: vi.fn(() => Promise.resolve(false)),
  minimize: vi.fn(() => Promise.resolve()),
  onResized: vi.fn(() => Promise.resolve(() => undefined)),
  setFullscreen: vi.fn(() => Promise.resolve()),
  toggleMaximize: vi.fn(() => Promise.resolve()),
  isMacOSDesktopChrome: vi.fn(() => false),
  isWindowsDesktopChrome: vi.fn(() => false),
}));

vi.mock("@tauri-apps/api/window", () => ({
  getCurrentWindow: () => ({
    close: windowControlsMocks.close,
    isFullscreen: windowControlsMocks.isFullscreen,
    isMaximized: windowControlsMocks.isMaximized,
    minimize: windowControlsMocks.minimize,
    onResized: windowControlsMocks.onResized,
    setFullscreen: windowControlsMocks.setFullscreen,
    toggleMaximize: windowControlsMocks.toggleMaximize,
  }),
}));

vi.mock("@/lib/platform-chrome", () => ({
  isMacOSDesktopChrome: windowControlsMocks.isMacOSDesktopChrome,
  isWindowsDesktopChrome: windowControlsMocks.isWindowsDesktopChrome,
}));

import { WindowControls } from "@/components/layout/WindowControls";

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderControls() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);

  act(() => {
    root?.render(createElement(WindowControls));
  });
}

async function flushPromises() {
  await act(async () => {
    await Promise.resolve();
  });
}

beforeEach(() => {
  windowControlsMocks.close.mockClear();
  windowControlsMocks.isFullscreen.mockReset();
  windowControlsMocks.isFullscreen.mockResolvedValue(false);
  windowControlsMocks.isMaximized.mockReset();
  windowControlsMocks.isMaximized.mockResolvedValue(false);
  windowControlsMocks.minimize.mockClear();
  windowControlsMocks.onResized.mockClear();
  windowControlsMocks.setFullscreen.mockClear();
  windowControlsMocks.toggleMaximize.mockClear();
  windowControlsMocks.isMacOSDesktopChrome.mockReset();
  windowControlsMocks.isMacOSDesktopChrome.mockReturnValue(false);
  windowControlsMocks.isWindowsDesktopChrome.mockReset();
  windowControlsMocks.isWindowsDesktopChrome.mockReturnValue(false);
  delete document.documentElement.dataset.irisWindowFullscreen;
});

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
});

describe("WindowControls", () => {
  it("does not render custom controls on macOS native chrome", () => {
    windowControlsMocks.isMacOSDesktopChrome.mockReturnValue(true);

    renderControls();

    expect(document.querySelector(".iris-window-controls")).toBeNull();
    expect(document.querySelector(".iris-traffic-light")).toBeNull();
  });

  it("keeps non-Windows custom controls as fullscreen, minimize, close", () => {
    renderControls();

    const buttons = [
      ...document.querySelectorAll<HTMLButtonElement>(".iris-traffic-light"),
    ];

    expect(buttons).toHaveLength(3);
    expect(buttons.map((button) => button.className)).toEqual([
      expect.stringContaining("iris-traffic-light--maximize"),
      expect.stringContaining("iris-traffic-light--minimize"),
      expect.stringContaining("iris-traffic-light--close"),
    ]);
  });

  it("uses the green custom control for native fullscreen instead of maximize on non-macOS desktops", async () => {
    renderControls();
    await flushPromises();

    const fullscreenButton = document.querySelector<HTMLButtonElement>(
      ".iris-traffic-light--maximize",
    );

    expect(fullscreenButton).not.toBeNull();
    act(() => fullscreenButton?.click());
    await flushPromises();

    expect(windowControlsMocks.isFullscreen).toHaveBeenCalled();
    expect(windowControlsMocks.setFullscreen).toHaveBeenCalledWith(true);
    expect(windowControlsMocks.toggleMaximize).not.toHaveBeenCalled();
  });

  it("keeps Windows controls in minimize, maximize, close order", () => {
    windowControlsMocks.isWindowsDesktopChrome.mockReturnValue(true);

    renderControls();

    const buttons = [
      ...document.querySelectorAll<HTMLButtonElement>(
        ".iris-window-control--windows",
      ),
    ];

    expect(buttons).toHaveLength(3);
    expect(buttons.map((button) => button.getAttribute("aria-label"))).toEqual([
      "最小化",
      "最大化",
      "关闭",
    ]);
  });
});
