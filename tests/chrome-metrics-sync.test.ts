import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
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
  it("TypeScript mirror matches Rust chrome_metrics constants", () => {
    const rust = read("src-tauri/src/chrome_metrics.rs");
    expect(rust).toContain("pub const TITLEBAR_HEIGHT: f64 = 44.0");
    expect(rust).toContain("pub const TRAFFIC_INSET: f64 = 88.0");
    expect(rust).toContain("pub const DEFAULT_TITLEBAR_HEIGHT: f64 = 44.0");
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
    expect(mainWindow?.trafficLightPosition).toEqual({ x: 14, y: 18 });
    expect(read("src/lib/platform-chrome.ts")).toContain(
      "return isTauriRuntime() && !isMacOSDesktopChrome()",
    );
  });
});
