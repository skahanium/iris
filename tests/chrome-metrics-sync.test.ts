import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  applyDesktopChromeFullscreenStateToDocument,
  applyDesktopChromeMetricsToDocument,
  DEFAULT_TITLEBAR_HEIGHT_PX,
  MACOS_TITLEBAR_HEIGHT_PX,
  MACOS_TRAFFIC_INSET_PX,
} from "@/lib/chrome-metrics";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("chrome metrics SSOT", () => {
  it("fullscreen state overrides the inline macOS traffic inset", () => {
    applyDesktopChromeMetricsToDocument({
      titlebarHeightLogical: MACOS_TITLEBAR_HEIGHT_PX,
      trafficInsetLogical: MACOS_TRAFFIC_INSET_PX,
      scaleFactor: 2,
    });

    expect(
      document.documentElement.style.getPropertyValue(
        "--titlebar-traffic-inset",
      ),
    ).toBe("88px");

    applyDesktopChromeFullscreenStateToDocument(true);

    expect(document.documentElement.dataset.irisWindowFullscreen).toBe("");
    expect(
      document.documentElement.style.getPropertyValue(
        "--titlebar-traffic-inset",
      ),
    ).toBe("0px");

    applyDesktopChromeFullscreenStateToDocument(false, {
      titlebarHeightLogical: MACOS_TITLEBAR_HEIGHT_PX,
      trafficInsetLogical: MACOS_TRAFFIC_INSET_PX,
      scaleFactor: 2,
    });

    expect(document.documentElement.dataset.irisWindowFullscreen).toBe(
      undefined,
    );
    expect(
      document.documentElement.style.getPropertyValue(
        "--titlebar-traffic-inset",
      ),
    ).toBe("88px");
  });

  it("TypeScript mirror matches Rust chrome_metrics constants", () => {
    const rust = read("src-tauri/src/chrome_metrics.rs");
    expect(rust).toContain("pub const DEFAULT_TITLEBAR_HEIGHT: f64 = 44.0");
    expect(rust).toContain(
      "pub const TITLEBAR_HEIGHT: f64 = super::DEFAULT_TITLEBAR_HEIGHT",
    );
    expect(rust).toContain("pub const TRAFFIC_INSET: f64 = 88.0");
    expect(rust).toContain("TITLEBAR_HEIGHT as MACOS_TITLEBAR_HEIGHT");
    expect(rust).toContain("TRAFFIC_INSET as MACOS_TRAFFIC_INSET");
    expect(MACOS_TITLEBAR_HEIGHT_PX).toBe(44);
    expect(DEFAULT_TITLEBAR_HEIGHT_PX).toBe(44);
    expect(MACOS_TRAFFIC_INSET_PX).toBe(88);
    expect(read("src-tauri/src/commands/window_chrome_cmd.rs")).toContain(
      "traffic_inset_logical: crate::chrome_metrics::MACOS_TRAFFIC_INSET",
    );
  });

  it("globals.css uses a unified 44px titlebar on all desktop platforms", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain(":root");
    expect(css).toMatch(/--titlebar-height:\s*2\.75rem/);
    expect(css).toContain("html[data-iris-platform-macos]");
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

  it("macOS uses decorated overlay shell with native traffic lights", () => {
    const macosSource = read("src-tauri/tauri.macos.conf.json");
    const macos = JSON.parse(macosSource) as {
      app?: {
        windows?: Array<{
          trafficLightPosition?: { x?: number; y?: number };
        }>;
      };
    };
    const mainWindow = macos.app?.windows?.[0];

    expect(macosSource).toContain('"decorations": true');
    expect(macosSource).toContain('"transparent": false');
    expect(macosSource).toContain('"titleBarStyle": "Overlay"');
    expect(macosSource).toContain('"hiddenTitle": true');
    expect(mainWindow?.trafficLightPosition).toEqual({ x: 14, y: 24 });
    expect(read("src/lib/platform-chrome.ts")).toContain(
      "return isTauriRuntime() && !isMacOSDesktopChrome()",
    );
  });
});
