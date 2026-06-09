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
    expect(rust).toContain("pub const DEFAULT_TITLEBAR_HEIGHT: f64 = 44.0");
    expect(rust).toContain("TITLEBAR_HEIGHT as MACOS_TITLEBAR_HEIGHT");
    expect(MACOS_TITLEBAR_HEIGHT_PX).toBe(44);
    expect(DEFAULT_TITLEBAR_HEIGHT_PX).toBe(44);
    expect(MACOS_TRAFFIC_INSET_PX).toBe(0);
    expect(read("src-tauri/src/commands/window_chrome_cmd.rs")).toContain(
      "traffic_inset_logical: 0.0",
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
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-traffic-inset:\s*0px/,
    );
  });

  it("macOS uses undecorated overlay shell with custom window controls", () => {
    const macos = read("src-tauri/tauri.macos.conf.json");
    expect(macos).toContain('"decorations": false');
    expect(macos).not.toContain("trafficLightPosition");
    expect(read("src/lib/platform-chrome.ts")).toContain(
      "return isTauriRuntime()",
    );
  });
});
