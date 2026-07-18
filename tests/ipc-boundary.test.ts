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
    expect(llmCommands).toContain("list_providers()");
  });

  it("exposes the Run-only execution contract with typed wrappers", () => {
    const ipc = read("src/lib/ipc.ts");
    const commands = read("src-tauri/src/commands/assistant_commands.rs");
    const lib = read("src-tauri/src/lib.rs");

    for (const command of [
      "assistant_run_start",
      "assistant_run_control",
      "assistant_run_get",
    ]) {
      expect(commands).toContain(`pub async fn ${command}`);
      expect(ipc).toContain(`"${command}"`);
      expect(lib).toContain(`commands::assistant_commands::${command}`);
    }
    expect(ipc).toContain("AssistantRunStartRequest");
    expect(ipc).toContain("AssistantRunControlRequest");
    expect(ipc).toContain("AssistantRunGetRequest");

    for (const retired of [
      "assistant_execute",
      "ai_send_message",
      "context_assemble",
      "tool_confirm",
      "agent_task_resume",
      "harness_resume",
    ]) {
      expect(commands).not.toContain(`pub async fn ${retired}`);
      expect(ipc).not.toContain(`"${retired}"`);
    }
  });

  it("routes session history through opaque domain references and preserves content parts", () => {
    const ipc = read("src/lib/ipc.ts");
    const aiTypes = read("src/types/ai.ts");
    const commands = read("src-tauri/src/commands/assistant_commands.rs");

    expect(aiTypes).toContain("export interface AssistantSessionRef");
    expect(aiTypes).toContain("domain: SecurityDomain");
    expect(aiTypes).toContain("export interface AssistantSessionMessage");
    expect(aiTypes).toContain("contentParts?: unknown");
    expect(commands).toContain("pub struct AssistantSessionMessage");
    expect(commands).toContain("pub content_parts: Option<serde_json::Value>");
    expect(commands).toContain("SecurityDomain::Normal =>");
    expect(commands).toContain("SecurityDomain::Classified =>");
    expect(ipc).toContain(
      'invoke<AssistantSessionMessage[]>("assistant_session_load"',
    );
    expect(commands).not.toContain("note_path");
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

  it("keeps web evidence and Skills provider configuration outside the execution boundary", () => {
    const ipc = read("src/lib/ipc.ts");
    const aiCommands = read("src-tauri/src/commands/ai_commands.rs");

    for (const command of [
      "web_evidence_provider_upsert",
      "web_evidence_providers_list",
      "web_evidence_provider_diagnostics",
      "skills_create_draft",
      "skills_confirm",
    ]) {
      expect(aiCommands).toContain(command);
      expect(ipc).toContain(`"${command}"`);
    }
    expect(ipc).toContain("export interface WebEvidenceProviderSummary");
    expect(ipc).toContain("export async function skillsCreateDraft");
    expect(ipc).toContain("export async function skillsConfirm");
    expect(aiCommands).not.toContain("pub async fn ai_send_message");
  });

  it("documents the Run-only execution and prompt-only Skills boundary", () => {
    const docs = read("docs/ipc-api-reference.md");

    expect(docs).toContain("assistant_run_start");
    expect(docs).toContain("assistant_run_control");
    expect(docs).toContain("assistant_run_get");
    expect(docs).toContain("assistant_execute");
    expect(docs).toContain("Skills are prompt-only");
    expect(docs).not.toContain("mcpRuntimeCapabilityCall");
  });
});
