import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("desktop title bar", () => {
  it("macOS config uses overlay title bar without visible default title", () => {
    const macos = read("src-tauri/tauri.macos.conf.json");
    expect(macos).toContain('"titleBarStyle": "Overlay"');
    expect(macos).toContain('"hiddenTitle": true');
    expect(macos).toContain("trafficLightPosition");
    expect(macos).not.toContain("Tauri App");
  });

  it("main window title is Iris in tauri config and rust chrome", () => {
    expect(read("src-tauri/tauri.conf.json")).toContain('"title": "Iris"');
    const chrome = read("src-tauri/src/window_chrome.rs");
    expect(chrome).toContain('MAIN_WINDOW_TITLE: &str = "Iris"');
    expect(chrome).toContain("set_title");
    expect(chrome).toContain("MAIN_WINDOW_TITLE");
    expect(read("src-tauri/src/macos_traffic_lights.rs")).toContain(
      "apply_traffic_light_position",
    );
    expect(read("src/hooks/useMacOSWindowChromeSync.ts")).toContain(
      "reapplyWindowChrome",
    );
  });

  it("DesktopTitleBar is the single chrome component with platform-aware controls", () => {
    const bar = read("src/components/layout/DesktopTitleBar.tsx");
    expect(bar).toContain('data-testid="desktop-title-bar"');
    expect(bar).toContain("showCustomWindowControls");
    expect(bar).toContain("customWindowControls");
    expect(bar).toContain("headerNativeDragRegion");
    expect(bar).toContain("macEmptyToolbar");
    expect(bar).toContain('role="banner"');

    const platform = read("src/lib/platform-chrome.ts");
    expect(platform).toContain("isMacOSDesktopChrome");
    expect(platform).toContain("showCustomWindowControls");

    const controls = read("src/components/layout/WindowControls.tsx");
    expect(controls).toContain("iris-window-controls");
    expect(controls).toContain("stopPropagation");
    expect(bar).toContain("--window-controls-width");
    expect(bar).toContain("absolute inset-y-0 right-0");
  });

  it("TabBar re-exports DesktopTitleBar for backward compatibility", () => {
    expect(read("src/components/layout/TabBar.tsx")).toContain(
      "DesktopTitleBar",
    );
  });

  it("defines titlebar CSS tokens and macOS traffic inset", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--titlebar-height");
    expect(css).toContain("--titlebar-traffic-inset");
    expect(css).toContain("data-iris-platform-macos");
    expect(css).toMatch(
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-height:\s*2rem/,
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
  });

  it("macOS config traffic light y is 10 for 32px bar", () => {
    expect(read("src-tauri/tauri.macos.conf.json")).toContain('"y": 10');
  });
});
