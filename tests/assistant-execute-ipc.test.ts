import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant_execute IPC contract", () => {
  it("registers the unified command in Tauri", () => {
    const lib = read("src-tauri/src/lib.rs");
    expect(lib).toContain("commands::assistant_commands::assistant_execute");
  });

  it("exposes typed assistantExecute in ipc.ts", () => {
    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain(
      'invoke<AssistantExecuteResponse>("assistant_execute"',
    );
  });

  it("routes intents in Rust assistant_commands", () => {
    const source = read("src-tauri/src/commands/assistant_commands.rs");
    expect(source).toContain("AssistantIntent::Writing");
    expect(source).toContain("AssistantIntent::Research");
    expect(source).toContain("AssistantIntent::Document");
  });

  it("UnifiedAssistantPanel calls assistantExecute", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("assistantExecute(");
    expect(panel).not.toContain("writingExecute(");
    expect(panel).not.toContain("researchExecute(");
  });
});
