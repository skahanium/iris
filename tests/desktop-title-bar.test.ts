import { readFileSync } from "node:fs";

import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

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

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderTitleBar(
  tabs: TabItem[] = [
    {
      path: "/vault/女友的闺蜜.md",
      title: "女友的闺蜜",
      dirty: true,
    },
  ],
) {
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

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  host?.remove();
  root = null;
  host = null;
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

  it("DesktopTitleBar uses items-center on macOS and avoids items-end for tabs", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(bar).toContain("macCenteredChrome");
    expect(bar).toContain("items-center");
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

  it("DesktopTitleBar exposes Iris Rail brand rail and segment tab hooks", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const css = read("src/styles/globals.css");
    expect(bar).toContain('data-testid="iris-brand-rail"');
    expect(bar).toContain('data-testid="rail-segment-tab"');
    expect(bar).not.toContain('data-testid="home-segment"');
    expect(bar).not.toContain("iris-home-segment");
    expect(bar).toContain("iris-brand-rail flex h-8");
    expect(bar).toContain("min-w-[6.75rem]");
    expect(bar).not.toContain("iris-brand-rail flex h-full");
    expect(bar).toContain("iris-brand-rail--active");
    expect(bar).toContain("iris-rail-tab--active");
    expect(css).toContain(".iris-brand-rail:hover");
    expect(css).toContain("inset 0 0 0 1px hsl(var(--knowledge-accent)");
  });

  it("keeps the tab rail single-row without native scrollbars", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    const css = read("src/styles/globals.css");

    expect(bar).toContain("iris-titlebar-tab-rail");
    expect(bar).not.toContain("overflow-x-auto");
    expect(bar).not.toContain("overflow-y-auto");
    expect(css).toContain(".iris-titlebar-tab-rail");
    expect(css).toContain("scrollbar-width: none");
    expect(css).toContain(".iris-titlebar-tab-rail::-webkit-scrollbar");
    expect(css).toContain("display: none");
  });

  it("keeps the close button inside the visible tab segment", () => {
    renderTitleBar();

    const closeButton = document.querySelector<HTMLButtonElement>(
      '[aria-label="关闭 女友的闺蜜"]',
    );
    const tabSegment = closeButton?.closest('[data-testid="rail-segment-tab"]');

    expect(closeButton).not.toBeNull();
    expect(tabSegment).not.toBeNull();
    expect(tabSegment?.textContent).toContain("女友的闺蜜");
  });

  it("does not expose artifact ids in tab tooltips", () => {
    renderTitleBar([
      {
        path: "artifact:process:req-secret",
        title: "过程详情",
        kind: "artifact",
      },
    ]);

    const tabSegment = document.querySelector<HTMLElement>(
      '[data-testid="rail-segment-tab"]',
    );

    expect(tabSegment?.getAttribute("title")).toBe("过程详情");
    expect(tabSegment?.getAttribute("title")).not.toContain("artifact:");
    expect(tabSegment?.getAttribute("title")).not.toContain("req-secret");
  });
});
