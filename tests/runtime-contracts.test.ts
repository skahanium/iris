import { readFileSync } from "node:fs";
import { existsSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("runtime configuration contracts", () => {
  it("keeps the main window hidden until the React startup splash reveals it", () => {
    const tauriConfig = JSON.parse(read("src-tauri/tauri.conf.json")) as {
      app?: { windows?: Array<{ label?: string; visible?: boolean }> };
    };
    const mainWindow = tauriConfig.app?.windows?.find(
      (window) => window.label === "main",
    );
    const lib = read("src-tauri/src/lib.rs");
    const chromeCommand = read("src-tauri/src/commands/window_chrome_cmd.rs");
    const ipc = read("src/lib/ipc.ts");
    const splash = read("src/components/layout/StartupSplash.tsx");

    const setupStart = lib.indexOf(".setup(|app| {");
    const invokeHandlerStart = lib.indexOf(".invoke_handler(");
    const setupBlock = lib.slice(setupStart, invokeHandlerStart);
    const chromeIndex = chromeCommand.indexOf(
      "window_chrome::apply_main_window_chrome(&window)",
    );
    const showIndex = chromeCommand.indexOf(".show()");

    expect(mainWindow?.visible).toBe(false);
    expect(setupBlock).toContain(
      "window_chrome::apply_main_window_chrome(&window)",
    );
    expect(setupBlock).not.toContain(".show()");
    expect(setupBlock).not.toContain("set_focus()");
    expect(chromeIndex).toBeGreaterThanOrEqual(0);
    expect(showIndex).toBeGreaterThan(chromeIndex);
    expect(ipc).toContain("showMainWindowWhenReady");
    expect(ipc).toContain('invoke("show_main_window_when_ready")');
    expect(splash).toContain("showMainWindowWhenReady");
    expect(lib).toContain(
      "commands::window_chrome_cmd::show_main_window_when_ready",
    );
    expect(lib).not.toContain("Theme::Dark");
  });

  it("bootstraps persisted theme before React first render", () => {
    const main = read("src/main.tsx");
    const themeHook = read("src/hooks/useTheme.ts");
    const bootstrapIndex = main.indexOf("bootstrapStoredTheme()");
    const renderIndex = main.indexOf("createRoot(");

    expect(main).toContain("function bootstrapStoredTheme");
    expect(bootstrapIndex).toBeGreaterThanOrEqual(0);
    expect(renderIndex).toBeGreaterThan(bootstrapIndex);
    expect(themeHook).toContain("function readStoredTheme");
    expect(themeHook).toContain('useState<"dark" | "light">(readStoredTheme)');
    expect(themeHook).not.toContain('useState<"dark" | "light">("dark")');
  });

  it("defines a token-driven startup splash with reduced-motion fallback", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain(".iris-startup-splash");
    expect(css).toContain("var(--background)");
    expect(css).toContain("var(--knowledge-accent)");
    expect(css).toContain("@keyframes iris-startup-orbit");
    expect(css).toContain("@media (prefers-reduced-motion: reduce)");
  });

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

  it("keeps external write paths on single-read indexing helpers", () => {
    const watcher = read("src-tauri/src/watcher/mod.rs");
    const writing = read("src-tauri/src/commands/writing_commands.rs");
    const organize = read("src-tauri/src/commands/organize_commands.rs");

    expect(watcher).toContain("index_file_from_content");
    expect(writing).toContain("index_file_from_content");
    expect(organize).toContain("index_file_from_content");

    expect(watcher).not.toContain("index_file_with_embed(conn, &vault, path");
    expect(writing).not.toContain("index_file_with_embed(conn, &vault, &abs");
    expect(organize).not.toContain("index_file_with_embed(conn, &vault, &abs");
  });

  it("registers links single-column index migration", () => {
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
    expect(migrate).toContain("031_links_single_column_indexes");
  });

  it("keeps long-lived secret strings zeroized on runtime boundaries", () => {
    const credentials = read("src-tauri/src/credentials.rs");
    const classified = read("src-tauri/src/commands/classified.rs");

    expect(credentials).toContain("impl Drop for ApiKeyBundle");
    expect(credentials).toContain("value.zeroize()");
    expect(classified).toContain("Zeroizing::new(password)");
  });

  it("keeps Rust cosine semantic fallback bounded", () => {
    const engine = read("src-tauri/src/embedding/engine.rs");

    expect(engine).toContain("MAX_COSINE_FALLBACK_CHUNKS: i64 = 8_000");
    expect(engine).toContain("chunk_count > MAX_COSINE_FALLBACK_CHUNKS");
    expect(engine).toContain("cosine fallback skipped: too many chunks");
  });

  it("keeps fixed AI IPC boundaries typed", () => {
    const aiTypes = read("src-tauri/src/ai_types/mod.rs");
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");
    const researchCommands = read(
      "src-tauri/src/commands/research_commands.rs",
    );

    expect(aiTypes).toContain("pub fn parse_wire(value: &str) -> Option<Self>");
    expect(aiCommands).not.toContain(
      'serde_json::from_str(&format!("\\"{scene}\\""',
    );
    expect(aiCommands).toContain("AppResult<AiChatResponse>");
    expect(aiCommands).toContain("AppResult<Vec<AiToolInfo>>");
    expect(aiCommands).toContain("AppResult<KnowledgeReindexResponse>");
    expect(researchCommands).toContain("AppResult<ResearchExecuteResponse>");
    expect(researchCommands).toContain("AppResult<ResearchStatusResponse>");
  });
});
