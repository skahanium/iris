import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("retired runtime contract", () => {
  it("does not compile retired runtime modules", () => {
    for (const path of [
      "src-tauri/src/ai_runtime/context_cache.rs",
      "src-tauri/src/ai_runtime/tool_effects.rs",
      "src-tauri/src/ai_runtime/tool_fallback.rs",
      "src-tauri/src/ai_runtime/writing_state.rs",
    ]) {
      expect(existsSync(path), path).toBe(false);
    }
  });

  it("keeps only the current Run event contract", () => {
    const eventRegistry = read("src/lib/ipc-events.ts");
    const ipc = read("src/lib/ipc.ts");
    const ipcTypes = read("src/types/ipc.ts");

    expect(eventRegistry).not.toContain("version:save_complete");
    expect(
      read("src-tauri/src/ai_runtime/model_gateway/streaming.rs"),
    ).not.toContain("llm:reset");
    expect(ipc).not.toContain("listenVersionSaveComplete");
    for (const typeName of [
      "ToolConfirmRequestEvent",
      "LlmTokenEvent",
      "LlmDoneEvent",
      "LlmErrorEvent",
      "LlmResetEvent",
      "HarnessTraceEvent",
      "VersionSaveCompleteEvent",
    ]) {
      expect(ipcTypes).not.toContain(`interface ${typeName}`);
    }
  });

  it("does not register orphan maintenance commands", () => {
    const lib = read("src-tauri/src/lib.rs");
    const ipc = read("src/lib/ipc.ts");

    for (const command of [
      "llm_providers",
      "version_cleanup_cmd",
      "document_title_audit_cmd",
      "skills_paths",
      "classified_ai_retrieval_clear",
    ]) {
      expect(lib).not.toContain(command);
      expect(ipc).not.toContain(`\"${command}\"`);
    }
  });

  it("does not generate legacy path identities for editor surfaces", () => {
    expect(read("src/components/layout/AppEditorWorkspace.tsx")).not.toContain(
      "legacy:${effectiveNotePath}",
    );
  });

  it("keeps production dependencies free of retired editor and Rust packages", () => {
    const packageJson = JSON.parse(read("package.json")) as {
      dependencies: Record<string, string>;
    };
    const cargoToml = read("src-tauri/Cargo.toml");

    expect(packageJson.dependencies).not.toHaveProperty(
      "@tiptap/extension-code-block-lowlight",
    );
    expect(cargoToml).not.toContain("urlencoding =");
    expect(cargoToml).not.toContain("webpki-roots =");
  });
});
