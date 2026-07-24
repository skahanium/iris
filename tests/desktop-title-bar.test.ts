import { readFileSync } from "node:fs";

import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  DesktopTitleBar,
  type TabItem,
} from "@/components/layout/DesktopTitleBar";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

const brandRailHandlerProp = ["on", "Home"].join("");
const brandRailActiveProp = ["is", "Home", "Active"].join("");
const brandRailActiveClass = ["iris-brand-rail--", "active"].join("");
const brandRailClickBinding = ["onClick={", "on", "Home", "}"].join("");

const originalUserAgent = window.navigator.userAgent;
let root: Root | null = null;
let host: HTMLDivElement | null = null;

async function flushEffects(): Promise<void> {
  await act(async () => {
    await Promise.resolve();
  });
}

function setTauriRuntime(enabled: boolean): void {
  const runtimeWindow = window as typeof window & {
    __TAURI__?: unknown;
    __TAURI_EVENT_PLUGIN_INTERNALS__?: {
      unregisterListener: () => void;
    };
    __TAURI_INTERNALS__?: {
      invoke: () => Promise<boolean>;
      metadata: { currentWindow: { label: string } };
      transformCallback: () => number;
    };
  };
  if (enabled) {
    Object.defineProperty(window.navigator, "userAgent", {
      configurable: true,
      value: "Macintosh",
    });
    runtimeWindow.__TAURI__ = {};
    runtimeWindow.__TAURI_INTERNALS__ = {
      invoke: () => Promise.resolve(false),
      metadata: { currentWindow: { label: "main" } },
      transformCallback: () => 1,
    };
    runtimeWindow.__TAURI_EVENT_PLUGIN_INTERNALS__ = {
      unregisterListener: () => undefined,
    };
    return;
  }
  Reflect.deleteProperty(runtimeWindow, "__TAURI__");
  Reflect.deleteProperty(runtimeWindow, "__TAURI_INTERNALS__");
  Reflect.deleteProperty(runtimeWindow, "__TAURI_EVENT_PLUGIN_INTERNALS__");
  Object.defineProperty(window.navigator, "userAgent", {
    configurable: true,
    value: originalUserAgent,
  });
}

function renderTitleBar(
  tabs: TabItem[] = [
    {
      path: "/vault/sample-note.md",
      title: "Sample Note",
      dirty: true,
    },
  ],
) {
  setTauriRuntime(true);
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);

  act(() => {
    root?.render(
      createElement(DesktopTitleBar, {
        tabs,
        activePath: tabs[0]?.path ?? null,
        onSelect: () => undefined,
        onClose: () => undefined,
        onNew: () => undefined,
      }),
    );
  });
}

afterEach(async () => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
  await Promise.resolve();
  setTauriRuntime(false);
});

describe("desktop title bar", () => {
  it("macOS config uses decorated overlay shell with native traffic lights", () => {
    const macosSource = read("src-tauri/tauri.macos.conf.json");
    const macos = JSON.parse(macosSource) as {
      app?: {
        windows?: Array<{
          trafficLightPosition?: { x?: number; y?: number };
        }>;
      };
    };

    expect(macosSource).toContain('"titleBarStyle": "Overlay"');
    expect(macosSource).toContain('"hiddenTitle": true');
    expect(macosSource).toContain('"decorations": true');
    expect(macosSource).toContain('"transparent": false');
    expect(macos.app?.windows?.[0]?.trafficLightPosition).toEqual({
      x: 14,
      y: 24,
    });
    expect(macosSource).not.toContain("Tauri App");
  });

  it("main window title is Iris in tauri config and rust chrome", () => {
    expect(read("src-tauri/tauri.conf.json")).toContain('"title": "Iris"');
    const chrome = read("src-tauri/src/window_chrome.rs");
    expect(chrome).toContain('MAIN_WINDOW_TITLE: &str = "Iris"');
    expect(chrome).toContain("set_title");
    expect(chrome).toContain("MAIN_WINDOW_TITLE");
    expect(chrome).toContain("set_decorations(false)");
    expect(chrome).toContain('#[cfg(not(target_os = "macos"))]');
    expect(read("src/hooks/useMacOSWindowChromeSync.ts")).toContain(
      "getDesktopChromeMetrics",
    );
  });

  it("macOS runtime keeps native traffic lights as the only chrome owner", () => {
    const chrome = read("src-tauri/src/window_chrome.rs");
    const lib = read("src-tauri/src/lib.rs");
    const syncHook = read("src/hooks/useMacOSWindowChromeSync.ts");
    const actions = read("src/lib/window-actions.ts");

    expect(chrome).not.toContain("apply_macos_rounded_window");
    expect(chrome).not.toContain("apply_traffic_light_position(window)");
    expect(lib).not.toContain("attach_macos_traffic_light_listeners");
    expect(syncHook).not.toContain("reapplyWindowChrome");
    expect(actions).not.toContain("setTitleBarStyle");
    expect(actions).not.toContain("setDecorations");
  });

  it("DesktopTitleBar is the single chrome component with platform-aware controls", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const frame = read("src/components/layout/DesktopFrame.tsx");
    expect(bar).toContain('data-testid="desktop-title-bar"');
    expect(bar).toContain("showCustomWindowControls");
    expect(bar).toContain("customWindowControls");
    expect(bar).toContain("headerNativeDragRegion");
    expect(bar).toContain("iris-titlebar-traffic-spacer");
    expect(bar).toContain("--titlebar-traffic-inset");
    expect(bar).toContain("iris-brand-rail");
    expect(bar).toContain('role="banner"');

    const platform = read("src/lib/platform-chrome.ts");
    expect(platform).toContain("isMacOSDesktopChrome");
    expect(platform).toContain("isWindowsDesktopChrome");
    expect(platform).toContain("showCustomWindowControls");
    expect(platform).toContain(
      "return isTauriRuntime() && !isMacOSDesktopChrome()",
    );

    const controls = read("src/components/layout/WindowControls.tsx");
    expect(controls).toContain("iris-window-controls");
    expect(controls).toContain("MacTrafficLightButton");
    expect(controls).toContain("WindowsControlButton");
    expect(controls).toContain("isMacOSDesktopChrome");
    expect(controls).toContain("isWindowsDesktopChrome");
    expect(controls).toContain("iris-traffic-light--close");
    expect(controls).toContain("iris-traffic-light--minimize");
    expect(controls).toContain("iris-traffic-light--maximize");
    expect(controls).toContain("iris-window-control--windows");
    expect(controls).toContain("iris-window-control--close");
    expect(controls).toContain("Minus");
    expect(controls).toContain("Square");
    expect(controls).toContain("Copy");
    expect(controls).toContain("stopPropagation");
    expect(bar).toContain("--window-controls-width");
    expect(bar).toContain("absolute inset-y-0 right-0");
    expect(frame).toContain("/Mac/i.test(navigator.userAgent)");
    expect(frame).toContain("return false");
  });

  it("gives Windows minimize and maximize controls a visible hover background", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain(".iris-window-control--windows:hover");
    expect(css).toContain("background: hsl(var(--foreground) / 0.08)");
    expect(css).toContain(".iris-window-control--close:hover");
  });

  it("uses soft chrome focus instead of coarse primary rings for titlebar controls", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const controls = read("src/components/layout/WindowControls.tsx");

    expect(bar).toContain("iris-focus-soft-within");
    expect(bar).toContain("iris-focus-soft");
    expect(controls).toContain("iris-focus-soft");
    expect(bar).not.toContain("focus-within:ring-2 focus-within:ring-primary");
    expect(bar).not.toContain(
      "focus-visible:ring-2 focus-visible:ring-primary",
    );
    expect(controls).not.toContain("focus-visible:ring-2");
    expect(controls).not.toContain("focus-visible:ring-primary");
  });

  it("TabBar re-exports DesktopTitleBar for backward compatibility", () => {
    expect(read("src/components/layout/TabBar.tsx")).toContain(
      "DesktopTitleBar",
    );
  });

  it("defines titlebar CSS tokens with a macOS native traffic-light safe area", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--titlebar-height");
    expect(css).toContain("--titlebar-traffic-inset");
    expect(css).toContain(".iris-titlebar-traffic-spacer");
    expect(css).toContain("data-iris-platform-macos");
    expect(css).toMatch(
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-height:\s*2\.75rem/,
    );
    expect(css).toMatch(
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-traffic-inset:\s*88px/,
    );
    expect(css).toMatch(
      /html\[data-iris-platform-macos\]\[data-iris-window-fullscreen\][\s\S]*--titlebar-traffic-inset:\s*0px/,
    );
    expect(css).toMatch(
      /\.iris-titlebar-traffic-spacer\s*\{[\s\S]*transition:\s*width 180ms var\(--motion-ease\)/,
    );
  });

  it("DesktopTitleBar keeps titlebar contents vertically centered", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(bar).toContain("items-center");
    expect(bar).not.toContain("items-stretch");
    expect(bar).not.toContain("items-end");
    expect(bar).not.toContain("mb-1");
  });

  it("preserves macOS native traffic light safe area on the splash titlebar", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const splashBranch = bar.slice(
      bar.indexOf("{isSplash ? ("),
      bar.indexOf(") : (", bar.indexOf("{isSplash ? (")),
    );

    expect(splashBranch).toContain("isMacDesktop");
    expect(splashBranch).toContain("iris-titlebar-traffic-spacer");
    expect(splashBranch).toContain("--titlebar-traffic-inset");
    expect(splashBranch).toContain("AppBrandZone");
  });

  it("useMacOSWindowChromeSync handles fullscreen and chrome metrics IPC", () => {
    const hook = read("src/hooks/useMacOSWindowChromeSync.ts");
    const metrics = read("src/lib/chrome-metrics.ts");
    expect(hook).toContain("isFullscreen");
    expect(hook).toContain("applyDesktopChromeFullscreenStateToDocument");
    expect(metrics).toContain("irisWindowFullscreen");
    expect(metrics).toContain("--titlebar-traffic-inset");
    expect(metrics).toContain('"0px"');
    expect(hook).toContain("getDesktopChromeMetrics");
    expect(read("src/lib/ipc.ts")).toContain("get_desktop_chrome_metrics");
    expect(hook).not.toContain("reapplyWindowChrome");
    expect(hook).not.toContain("restoreMacOSWindowChrome");
  });

  it("keeps the Iris brand rail vertically centered in the titlebar", () => {
    renderTitleBar();

    const titleBar = document.querySelector<HTMLElement>(
      '[data-testid="desktop-title-bar"]',
    );
    const brandRail = document.querySelector<HTMLElement>(
      '[data-testid="iris-brand-rail"]',
    );

    expect(titleBar?.className).toContain("items-center");
    expect(titleBar?.className).not.toContain("items-stretch");
    expect(brandRail?.className).toContain("h-8");
    expect(brandRail?.className).not.toContain("-ml-1.5");
    expect(brandRail?.className).not.toContain("h-full");
  });
  it("DesktopTitleBar exposes Iris Rail brand rail and segment tab hooks", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const css = read("src/styles/globals.css");
    expect(bar).toContain('data-testid="iris-brand-rail"');
    expect(bar).toContain('data-testid="rail-segment-tab"');
    expect(bar).not.toContain('data-testid="home-segment"');
    expect(bar).not.toContain("iris-home-segment");
    expect(bar).toContain("iris-brand-rail flex h-8");
    expect(bar).not.toContain('isMacDesktop && "-ml-1.5"');
    expect(bar).toContain("min-w-[6.75rem]");
    expect(bar).not.toContain("iris-brand-rail flex h-full");
    expect(bar).not.toContain("pointer-events-none");
    expect(bar).toContain("data-tauri-drag-region");
    expect(bar).not.toContain(brandRailActiveClass);
    expect(bar).not.toContain(brandRailHandlerProp);
    expect(bar).not.toContain(brandRailActiveProp);
    expect(bar).not.toContain(brandRailClickBinding);
    expect(bar).not.toContain('role="button"');
    expect(bar).toContain("iris-rail-tab--active");
    expect(css).not.toContain(".iris-brand-rail:hover");
    expect(css).toContain(".iris-brand-rail {");
  });

  it("reserves a left safe-area for the brand rail on Windows and macOS fullscreen", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const css = read("src/styles/globals.css");

    expect(css).toContain("--titlebar-leading-inset: 0px");
    expect(css).toMatch(
      /html\[data-iris-desktop\]:not\(\[data-iris-platform-macos\]\)\s*\{[^}]*--titlebar-leading-inset:\s*0\.5rem/,
    );
    expect(css).toMatch(
      /html\[data-iris-platform-macos\]\[data-iris-window-fullscreen\]\s*\{[^}]*--titlebar-leading-inset:\s*0\.5rem/,
    );
    expect(bar).toContain("pl-[var(--titlebar-leading-inset)]");
    expect(bar).not.toContain("-ml-1.5");
    expect(css).toMatch(
      /html\[data-iris-platform-macos\]:not\(\[data-iris-window-fullscreen\]\)\s+div\.iris-brand-rail\s*\{[^}]*margin-left:\s*-0\.375rem/,
    );
  });

  it("compresses tabs instead of scrolling when the rail overflows", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const css = read("src/styles/globals.css");

    expect(bar).toContain("iris-titlebar-tab-rail");
    expect(bar).toContain("overflow-x-hidden");
    expect(bar).not.toContain("overflow-x-scroll");
    expect(bar).not.toContain("overflow-x-auto");
    expect(bar).not.toContain("overflow-y-auto");
    expect(css).toContain(".iris-rail-tab");
    expect(css).toContain("min-width: 4.5rem");
  });

  it("keeps the new-note button as the trailing action inside the tab rail", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");

    expect(bar).toContain("computeVisibleTabCount");
    expect(bar).toContain("MoreHorizontal");
    expect(bar).toContain("IrisSurfaceMenuPanel");
    expect(bar).toContain("更多笔记");
    expect(bar).toContain('data-testid="rail-new-note-button"');

    const brandIndex = bar.indexOf('data-testid="iris-brand-rail"');
    const tabRailIndex = bar.indexOf("iris-titlebar-tab-rail");
    const newButtonIndex = bar.indexOf('data-testid="rail-new-note-button"');

    expect(brandIndex).toBeGreaterThanOrEqual(0);
    expect(tabRailIndex).toBeGreaterThan(brandIndex);
    expect(newButtonIndex).toBeGreaterThan(tabRailIndex);
  });

  it("renders the new-note button after tab segments inside the tab rail", () => {
    renderTitleBar([
      { path: "/vault/a.md", title: "Alpha" },
      { path: "/vault/b.md", title: "Beta" },
    ]);

    const rail = document.querySelector<HTMLElement>(".iris-titlebar-tab-rail");
    const segments = Array.from(
      document.querySelectorAll<HTMLElement>(
        '[data-testid="rail-segment-tab"]',
      ),
    );
    const newButton = document.querySelector<HTMLButtonElement>(
      '[data-testid="rail-new-note-button"]',
    );

    expect(rail).not.toBeNull();
    expect(newButton).not.toBeNull();
    expect(newButton?.parentElement).toBe(rail);
    expect(segments).toHaveLength(2);
    expect(segments[0]?.parentElement).toBe(rail);
    expect(segments[1]?.parentElement).toBe(rail);
    expect(Array.from(rail!.children).indexOf(newButton!)).toBeGreaterThan(
      Array.from(rail!.children).indexOf(segments[1]!),
    );
  });

  it("keeps the new-note button after the overflow trigger when tabs spill", async () => {
    const clientWidthSpy = vi
      .spyOn(HTMLElement.prototype, "clientWidth", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.className.toString().includes("iris-titlebar-tab-rail")
          ? 240
          : 0;
      });
    const scrollWidthSpy = vi
      .spyOn(HTMLElement.prototype, "scrollWidth", "get")
      .mockImplementation(function (this: HTMLElement) {
        return this.className.toString().includes("iris-titlebar-tab-rail")
          ? 900
          : 0;
      });

    try {
      renderTitleBar(
        Array.from({ length: 8 }, (_, index) => ({
          path: `/vault/${index}.md`,
          title: `Tab ${index}`,
        })),
      );
      await flushEffects();

      const rail = document.querySelector<HTMLElement>(
        ".iris-titlebar-tab-rail",
      );
      const overflowButton = document.querySelector<HTMLButtonElement>(
        'button[aria-label="更多笔记"]',
      );
      const newButton = document.querySelector<HTMLButtonElement>(
        '[data-testid="rail-new-note-button"]',
      );

      expect(rail).not.toBeNull();
      expect(overflowButton).not.toBeNull();
      expect(newButton).not.toBeNull();
      expect(overflowButton?.parentElement?.parentElement).toBe(rail);
      expect(newButton?.parentElement).toBe(rail);
      expect(Array.from(rail!.children).indexOf(newButton!)).toBeGreaterThan(
        Array.from(rail!.children).indexOf(overflowButton!.parentElement!),
      );
    } finally {
      clientWidthSpy.mockRestore();
      scrollWidthSpy.mockRestore();
    }
  });
  it("lets tabs compress instead of locking a 7rem minimum width", () => {
    renderTitleBar([
      { path: "/vault/a.md", title: "Alpha" },
      { path: "/vault/b.md", title: "Beta" },
    ]);

    const segment = document.querySelector<HTMLElement>(
      '[data-testid="rail-segment-tab"]',
    );
    expect(segment?.className).not.toContain("shrink-0");
  });

  it("keeps the close button inside the visible tab segment", () => {
    renderTitleBar();

    const closeButton = document.querySelector<HTMLButtonElement>(
      'button[aria-label$="Sample Note"]',
    );
    const tabSegment = closeButton?.closest('[data-testid="rail-segment-tab"]');

    expect(closeButton).not.toBeNull();
    expect(tabSegment).not.toBeNull();
    expect(tabSegment?.textContent).toContain("Sample Note");
  });

  it("does not expose artifact ids in tab tooltips", () => {
    renderTitleBar([
      {
        path: "artifact:process:req-secret",
        title: "Process Detail",
        kind: "artifact",
      },
    ]);

    const tabSegment = document.querySelector<HTMLElement>(
      '[data-testid="rail-segment-tab"]',
    );

    expect(tabSegment?.getAttribute("title")).toBe("Process Detail");
    expect(tabSegment?.getAttribute("title")).not.toContain("artifact:");
    expect(tabSegment?.getAttribute("title")).not.toContain("req-secret");
  });
  it("does not expose mojibake in visible titlebar text or aria labels", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");

    expect(bar).not.toMatch(/[涓鈥鍏鏂鍏抽棴绗旇]/);
    expect(bar).toContain("临时");
    expect(bar).toContain("•");
    expect(bar).toContain("aria-label={`关闭 ${tab.title}`}");
    expect(bar).toContain('aria-label="新建笔记"');
  });
});
