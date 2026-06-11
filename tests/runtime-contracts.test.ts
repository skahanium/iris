import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("runtime configuration contracts", () => {
  it("keeps DeepSeek default provider reachable from Tauri CSP", () => {
    const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json")) as {
      app?: { security?: { csp?: string } };
    };
    const providers = read("src-tauri/src/llm/providers.rs");
    const routing = read("src-tauri/src/llm/config.rs");

    expect(providers).toContain('"https://api.deepseek.com"');
    expect(routing).toContain('provider_id: "deepseek"');
    expect(tauriConfig.app?.security?.csp).toContain(
      "https://api.deepseek.com",
    );
  });

  it("cleans up the App editor stats debounce timer on unmount", () => {
    const app = read("src/App.tsx");
    const hook = read("src/hooks/useEditorStats.ts");

    expect(app).toContain("useEditorStats");
    expect(app).not.toContain("editorStatsTimerRef");
    expect(hook).toContain("editorStatsTimerRef");
    expect(hook).toMatch(/clearTimeout\(\s*editorStatsTimerRef\.current\s*\)/);
  });

  it("reuses the shared editor stats hook in status bar contexts", () => {
    const statusBarContext = read("src/components/layout/StatusBarContext.tsx");

    expect(statusBarContext).toContain("useEditorStats");
    expect(statusBarContext).not.toContain("editorStatsTimerRef");
  });

  it("keeps sqlite-vec behind an explicit experimental unsafe review gate", () => {
    const cargoToml = read("src-tauri/Cargo.toml");

    expect(cargoToml).toContain(
      "# Experimental: sqlite-vec registration uses unsafe",
    );
    expect(cargoToml).toContain(
      'sqlite-vec = { version = "0.1.10-alpha.4", default-features = false, optional = true }',
    );
    expect(cargoToml).toContain("default = []");
    expect(cargoToml).toContain('sqlite-vec = ["dep:sqlite-vec"]');
  });
});
