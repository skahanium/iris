import { readFileSync } from "node:fs";
import { existsSync } from "node:fs";

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
