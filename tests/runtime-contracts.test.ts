import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("runtime configuration contracts", () => {
  it("keeps the main window hidden until the React startup splash reveals it", () => {
    const config = JSON.parse(read("src-tauri/tauri.conf.json")) as {
      app?: { windows?: Array<{ label?: string; visible?: boolean }> };
    };
    const main = config.app?.windows?.find((window) => window.label === "main");
    const lib = read("src-tauri/src/lib.rs");
    const splash = read("src/components/layout/StartupSplash.tsx");

    expect(main?.visible).toBe(false);
    expect(lib).toContain("show_main_window_when_ready");
    expect(splash).toContain("showMainWindowWhenReady");
  });

  it("bootstraps persisted theme before React first render", () => {
    const main = read("src/main.tsx");
    const bootstrapIndex = main.indexOf("bootstrapStoredTheme()");
    const renderIndex = main.indexOf("createRoot(");

    expect(bootstrapIndex).toBeGreaterThanOrEqual(0);
    expect(renderIndex).toBeGreaterThan(bootstrapIndex);
  });

  it("defines a token-driven startup splash with reduced-motion fallback", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain(".iris-startup-splash");
    expect(css).toContain("var(--knowledge-accent)");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });

  it("keeps the explicit sqlite-vec review gate", () => {
    const cargoToml = read("src-tauri/Cargo.toml");

    expect(cargoToml).toContain(
      "Experimental: sqlite-vec registration uses unsafe",
    );
    expect(cargoToml).toContain("default = []");
    expect(cargoToml).toContain('sqlite-vec = ["dep:sqlite-vec"]');
  });

  it("registers reversible link-index migration files", () => {
    const migrate = read("src-tauri/src/storage/migrate.rs");

    expect(
      existsSync("src-tauri/migrations/031_links_single_column_indexes.sql"),
    ).toBe(true);
    expect(
      existsSync(
        "src-tauri/migrations/031_links_single_column_indexes.down.sql",
      ),
    ).toBe(true);
    expect(migrate).toContain("MIGRATION_031_UP");
  });

  it("uses the Run IPC contract instead of deleted research or assistant-execute commands", () => {
    const ipc = read("src/lib/ipc.ts");
    const types = read("src/types/ai.ts");

    expect(ipc).toContain('invoke<AssistantRunAccepted>("assistant_run_start"');
    expect(ipc).toContain('invoke<void>("assistant_run_control"');
    expect(ipc).toContain("listenAssistantRunEvent");
    expect(types).toContain("export interface AssistantRunStartRequest");
    expect(types).toContain("export type AssistantRunEvent");
    expect(existsSync("src-tauri/src/commands/research_commands.rs")).toBe(
      false,
    );
  });
});
