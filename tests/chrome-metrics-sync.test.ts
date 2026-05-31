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
    expect(rust).toContain("pub const MACOS_TITLEBAR_HEIGHT: f64 = 32.0");
    expect(rust).toContain("pub const DEFAULT_TITLEBAR_HEIGHT: f64 = 40.0");
    expect(MACOS_TITLEBAR_HEIGHT_PX).toBe(32);
    expect(DEFAULT_TITLEBAR_HEIGHT_PX).toBe(40);
    expect(MACOS_TRAFFIC_INSET_PX).toBe(72);
    expect(rust).toContain("macos_traffic_inset_default_is_72");
  });

  it("globals.css uses 2rem titlebar on macOS only", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain(":root");
    expect(css).toMatch(/--titlebar-height:\s*2\.5rem/);
    expect(css).toContain("html[data-iris-platform-macos]");
    expect(css).toMatch(
      /html\[data-iris-platform-macos\][\s\S]*--titlebar-height:\s*2rem/,
    );
  });

  it("macOS traffic light config initial y matches center formula", () => {
    const macos = read("src-tauri/tauri.macos.conf.json");
    expect(macos).toContain('"y": 10');
    expect(read("src-tauri/src/macos_traffic_lights.rs")).toContain(
      "target_height",
    );
    expect(read("src-tauri/src/macos_traffic_lights.rs")).toContain(
      "vertical_offset",
    );
  });
});
