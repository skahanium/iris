import { readFileSync } from "node:fs";

import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { DesktopTitleBar } from "@/components/layout/DesktopTitleBar";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

let root: Root | null = null;
let host: HTMLDivElement | null = null;

function renderTitleBar() {
  host = document.createElement("div");
  document.body.append(host);
  root = createRoot(host);

  act(() => {
    root?.render(
      createElement(DesktopTitleBar, {
        tabs: [
          {
            path: "/vault/女友的闺蜜.md",
            title: "女友的闺蜜",
            dirty: true,
          },
        ],
        activePath: "/vault/女友的闺蜜.md",
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
  it("macOS config uses undecorated overlay shell with custom window controls", () => {
    const macos = read("src-tauri/tauri.macos.conf.json");
    expect(macos).toContain('"titleBarStyle": "Overlay"');
    expect(macos).toContain('"hiddenTitle": true');
    expect(macos).toContain('"decorations": false');
    expect(macos).not.toContain("trafficLightPosition");
    expect(macos).not.toContain("Tauri App");
  });

  it("main window title is Iris in tauri config and rust chrome", () => {
    expect(read("src-tauri/tauri.conf.json")).toContain('"title": "Iris"');
    const chrome = read("src-tauri/src/window_chrome.rs");
    expect(chrome).toContain('MAIN_WINDOW_TITLE: &str = "Iris"');
    expect(chrome).toContain("set_title");
    expect(chrome).toContain("MAIN_WINDOW_TITLE");
    expect(chrome).toContain("set_decorations(false)");
    expect(read("src/hooks/useMacOSWindowChromeSync.ts")).toContain(
      "getDesktopChromeMetrics",
    );
  });

  it("macOS runtime does not re-enable native traffic lights when custom right controls are used", () => {
    const chrome = read("src-tauri/src/window_chrome.rs");
    const lib = read("src-tauri/src/lib.rs");
    const syncHook = read("src/hooks/useMacOSWindowChromeSync.ts");

    expect(chrome).not.toContain("set_decorations(true)");
    expect(chrome).not.toContain("apply_traffic_light_position(window)");
    expect(lib).not.toContain("attach_macos_traffic_light_listeners");
    expect(syncHook).not.toContain("reapplyWindowChrome");
  });

  it("DesktopTitleBar is the single chrome component with platform-aware controls", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(bar).toContain('data-testid="desktop-title-bar"');
    expect(bar).toContain("showCustomWindowControls");
    expect(bar).toContain("customWindowControls");
    expect(bar).toContain("headerNativeDragRegion");
    expect(bar).toContain("iris-brand-rail");
    expect(bar).toContain('role="banner"');

    const platform = read("src/lib/platform-chrome.ts");
    expect(platform).toContain("isMacOSDesktopChrome");
    expect(platform).toContain("isWindowsDesktopChrome");
    expect(platform).toContain("showCustomWindowControls");

    const controls = read("src/components/layout/WindowControls.tsx");
    expect(controls).toContain("iris-window-controls");
    expect(controls).toContain("MacTrafficLightButton");
    expect(controls).toContain("WindowsControlButton");
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
  });

  it("TabBar re-exports DesktopTitleBar for backward compatibility", () => {
    expect(read("src/components/layout/TabBar.tsx")).toContain(
      "DesktopTitleBar",
    );
  });

  it("defines titlebar CSS tokens without a macOS left traffic inset", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--titlebar-height");
    expect(css).toContain("--titlebar-traffic-inset");
    expect(css).toContain("data-iris-platform-macos");
    expect(css).toMatch(
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-height:\s*2\.75rem/,
    );
    expect(css).toMatch(
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-traffic-inset:\s*0px/,
    );
  });

  it("DesktopTitleBar uses items-center on macOS and avoids items-end for tabs", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(bar).toContain("macCenteredChrome");
    expect(bar).toContain("items-center");
    expect(bar).not.toContain("items-end");
    expect(bar).not.toContain("mb-1");
  });

  it("useMacOSWindowChromeSync handles fullscreen and chrome metrics IPC", () => {
    const hook = read("src/hooks/useMacOSWindowChromeSync.ts");
    expect(hook).toContain("isFullscreen");
    expect(hook).toContain("irisWindowFullscreen");
    expect(hook).toContain("getDesktopChromeMetrics");
    expect(read("src/lib/ipc.ts")).toContain("get_desktop_chrome_metrics");
    expect(hook).not.toContain("reapplyWindowChrome");
  });

  it("DesktopTitleBar exposes Iris Rail brand rail and segment tab hooks", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(bar).toContain('data-testid="iris-brand-rail"');
    expect(bar).toContain('data-testid="rail-segment-tab"');
    expect(bar).not.toContain('data-testid="home-segment"');
    expect(bar).not.toContain("iris-home-segment");
    expect(bar).toContain("iris-brand-rail--active");
    expect(bar).toContain("iris-rail-tab--active");
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
});
