import { readdirSync, readFileSync } from "node:fs";
import { join } from "node:path";

import { describe, expect, it } from "vitest";

const root = process.cwd();

function read(path: string): string {
  return readFileSync(join(root, path), "utf8");
}

function sourceFiles(dir: string): string[] {
  return readdirSync(join(root, dir), { withFileTypes: true }).flatMap(
    (entry) => {
      const path = `${dir}/${entry.name}`;
      if (entry.isDirectory()) return sourceFiles(path);
      return /\.(ts|tsx)$/.test(entry.name) ? [path] : [];
    },
  );
}

describe("IPC boundary", () => {
  it("keeps direct Tauri invoke calls inside src/lib/ipc.ts", () => {
    const directInvokeFiles = sourceFiles("src").filter((path) =>
      /\binvoke\s*\(/.test(read(path)),
    );

    expect(directInvokeFiles).toEqual(["src/lib/ipc.ts"]);
  });

  it("exposes registered maintenance commands through typed wrappers", () => {
    const ipc = read("src/lib/ipc.ts");
    const llmCommands = read("src-tauri/src/commands/llm.rs");

    expect(ipc).toContain("export async function settingsReset");
    expect(ipc).toContain('invoke("settings_reset"');
    expect(ipc).toContain("export async function versionCleanup");
    expect(ipc).toContain('invoke<number>("version_cleanup_cmd"');
    expect(llmCommands).toContain("Deprecated compatibility alias");
  });

  it("keeps AI boundary scope structs camelCase for TypeScript callers", () => {
    const aiTypes = read("src-tauri/src/ai_types/mod.rs");

    expect(aiTypes).toMatch(
      /#\[serde\(rename_all = "camelCase"\)\]\s*pub struct CitationCheckScope[\s\S]*path_prefixes/,
    );
    expect(aiTypes).toMatch(
      /#\[serde\(rename_all = "camelCase"\)\]\s*pub struct OrganizeTaskScope[\s\S]*path_prefixes/,
    );
  });

  it("types tool confirmation as the full chat response returned by Rust", () => {
    const ipc = read("src/lib/ipc.ts");
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");

    expect(aiCommands).toContain("pub async fn tool_confirm");
    expect(aiCommands).toContain("AppResult<AiChatResponse>");
    expect(ipc).toContain("}): Promise<AiSendMessageResult>");
    expect(ipc).toContain('invoke<AiSendMessageResult>("tool_confirm"');
    expect(ipc).not.toContain(
      "Promise<{ request_id: string; tool_call_id: string; status: string }>",
    );
  });

  it("types harness resume as the full chat response and removes call-site casts", () => {
    const ipc = read("src/lib/ipc.ts");
    const resumeHook = read(
      "src/components/ai/hooks/useAssistantHarnessResume.ts",
    );

    expect(ipc).toMatch(
      /export async function harnessResume\([\s\S]*Promise<AiSendMessageResult>/,
    );
    expect(ipc).toContain('invoke<AiSendMessageResult>("harness_resume"');
    expect(resumeHook).not.toContain("as AiSendMessageResult");
  });

  it("keeps organize scope payload camelCase at the IPC edge", () => {
    const ipc = read("src/lib/ipc.ts");
    const organizeWrapper = ipc.slice(
      ipc.indexOf("type OrganizeScopeInput"),
      ipc.indexOf("export async function organizeApply"),
    );

    expect(organizeWrapper).toContain("pathPrefixes?: string[]");
    expect(organizeWrapper).toContain("corpusIds?: string[]");
    expect(organizeWrapper).toMatch(
      /pathPrefixes:[\s\S]*params\.scope\.pathPrefixes[\s\S]*params\.scope\.path_prefixes/,
    );
    expect(organizeWrapper).toMatch(
      /corpusIds:[\s\S]*params\.scope\.corpusIds[\s\S]*params\.scope\.corpus_ids/,
    );
    expect(organizeWrapper).not.toMatch(/scope:\s*params\.scope \?\? null/);
  });

  it("does not expose the unused absolute-path export command", () => {
    const ipc = read("src/lib/ipc.ts");
    const lib = read("src-tauri/src/lib.rs");
    const commands = read("src-tauri/src/commands/mod.rs");

    expect(ipc).not.toContain("export async function exportFile");
    expect(ipc).not.toContain('invoke("export_file"');
    expect(lib).not.toContain("commands::export::export_file");
    expect(commands).not.toContain("pub mod export");
  });

  it("exposes session message content_parts returned by Rust", () => {
    const types = read("src/types/ipc.ts");
    const session = read("src-tauri/src/ai_runtime/session.rs");

    expect(session).toContain("pub content_parts: Option<String>");
    expect(types).toContain("content_parts?: string | null");
  });
});
