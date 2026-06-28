import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const startupSplashMocks = vi.hoisted(() => ({
  isTauriRuntime: vi.fn(() => false),
  showMainWindowWhenReady: vi.fn(() => Promise.resolve()),
}));

vi.mock("@/lib/tauri-runtime", () => ({
  isTauriRuntime: startupSplashMocks.isTauriRuntime,
}));

vi.mock("@/lib/ipc", () => ({
  showMainWindowWhenReady: startupSplashMocks.showMainWindowWhenReady,
}));

import { StartupSplash } from "@/components/layout/StartupSplash";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let reducedMotion = false;

function renderSplash(
  props: Partial<Parameters<typeof StartupSplash>[0]> = {},
) {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);
  act(() => {
    root?.render(
      createElement(StartupSplash, {
        ready: false,
        minDurationMs: 1600,
        fadeDurationMs: 220,
        ...props,
      }),
    );
  });
}

function rerenderSplash(
  props: Partial<Parameters<typeof StartupSplash>[0]> = {},
) {
  act(() => {
    root?.render(
      createElement(StartupSplash, {
        ready: false,
        minDurationMs: 1600,
        fadeDurationMs: 220,
        ...props,
      }),
    );
  });
}

beforeEach(() => {
  vi.useFakeTimers();
  reducedMotion = false;
  startupSplashMocks.isTauriRuntime.mockReturnValue(false);
  startupSplashMocks.showMainWindowWhenReady.mockClear();
  Object.defineProperty(window, "matchMedia", {
    configurable: true,
    value: (query: string) => ({
      matches: reducedMotion && query === "(prefers-reduced-motion: reduce)",
      media: query,
      onchange: null,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
      addListener: vi.fn(),
      removeListener: vi.fn(),
      dispatchEvent: vi.fn(),
    }),
  });
  Object.defineProperty(window, "requestAnimationFrame", {
    configurable: true,
    value: (callback: FrameRequestCallback) =>
      window.setTimeout(() => callback(performance.now()), 16),
  });
  Object.defineProperty(window, "cancelAnimationFrame", {
    configurable: true,
    value: (id: number) => window.clearTimeout(id),
  });
});

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
  vi.useRealTimers();
});

describe("StartupSplash", () => {
  it("stays visible until startup is ready and the minimum duration has elapsed", () => {
    const onExited = vi.fn();
    renderSplash({ ready: false, onExited });

    expect(
      document.querySelector('[data-testid="startup-splash"]'),
    ).not.toBeNull();
    expect(document.body.textContent).toContain("唤醒知识网络");
    expect(document.body.textContent).toContain("准备笔记");

    act(() => vi.advanceTimersByTime(1700));
    expect(
      document.querySelector('[data-testid="startup-splash"]'),
    ).not.toBeNull();

    rerenderSplash({ ready: true, onExited });
    expect(document.body.textContent).toContain("打开工作区");
    expect(document.querySelector('[data-state="exiting"]')).not.toBeNull();

    act(() => vi.advanceTimersByTime(219));
    expect(
      document.querySelector('[data-testid="startup-splash"]'),
    ).not.toBeNull();
    expect(onExited).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(1));
    expect(document.querySelector('[data-testid="startup-splash"]')).toBeNull();
    expect(onExited).toHaveBeenCalledTimes(1);
  });

  it("waits out the remaining minimum duration after startup becomes ready", () => {
    renderSplash({ ready: false });

    act(() => vi.advanceTimersByTime(700));
    rerenderSplash({ ready: true });
    expect(document.querySelector('[data-state="visible"]')).not.toBeNull();

    act(() => vi.advanceTimersByTime(899));
    expect(document.querySelector('[data-state="visible"]')).not.toBeNull();

    act(() => vi.advanceTimersByTime(1));
    expect(document.querySelector('[data-state="exiting"]')).not.toBeNull();
  });

  it("marks the splash as reduced-motion when the user prefers less motion", () => {
    reducedMotion = true;
    renderSplash({ ready: false });

    const splash = document.querySelector('[data-testid="startup-splash"]');
    expect(splash?.className).toContain("iris-startup-splash--reduced-motion");
  });

  it("reveals the hidden Tauri window after the splash has painted", () => {
    startupSplashMocks.isTauriRuntime.mockReturnValue(true);

    renderSplash({ ready: false });

    expect(startupSplashMocks.showMainWindowWhenReady).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(16));
    expect(startupSplashMocks.showMainWindowWhenReady).not.toHaveBeenCalled();

    act(() => vi.advanceTimersByTime(16));
    expect(startupSplashMocks.showMainWindowWhenReady).toHaveBeenCalledTimes(1);
  });
});
