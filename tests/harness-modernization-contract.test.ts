import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("harness modernization remaining contracts", () => {
  it("context preview uses the same executable tool surface as harness runs", () => {
    const backend = read("src-tauri/src/commands/ai_commands.rs");
    expect(backend).toContain("web_search: Option<bool>");
    expect(backend).toContain("tools_for_surface(");
    expect(backend).not.toContain(
      "registry.for_scene(scene).into_iter().cloned().collect()",
    );

    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("web_search?: boolean");
    expect(ipc).toContain("webSearch: params.web_search ?? false");

    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("web_search: webSearch");
  });

  it("tool confirmation pauses at a single pending tool call before dispatching later calls", () => {
    const run = read("src-tauri/src/ai_runtime/harness/run.rs");
    expect(run).toContain("first_pending_confirmation_call");
    expect(run).toContain("pause_for_tool_confirmation");
    expect(run.indexOf("pause_for_tool_confirmation")).toBeLessThan(
      run.indexOf("for tool_call in &other_calls"),
    );
  });

  it("assistant stop controls the active harness request, not only the legacy LLM engine", () => {
    const backend = read("src-tauri/src/commands/ai_commands.rs");
    expect(backend).toContain("pub async fn harness_abort");
    expect(read("src-tauri/src/lib.rs")).toContain(
      "commands::ai_commands::harness_abort",
    );

    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("export async function harnessAbort");

    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("harnessAbort(id)");
  });

  it("non-chat assistant tasks drive the unified run state while in flight", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain(
      'assistantRun.setFromTaskStatus("running", "writing")',
    );
    expect(panel).toContain(
      'assistantRun.setFromTaskStatus("running", "citation")',
    );
    expect(panel).toContain(
      'assistantRun.setFromTaskStatus("running", "organize")',
    );
    expect(panel).toContain(
      'assistantRun.setFromTaskStatus("running", "research")',
    );
    expect(panel).toContain(
      'assistantRun.setFromTaskStatus("running", "chapter")',
    );
    expect(panel).toContain(
      'assistantRun.setFromTaskStatus("running", "document")',
    );
  });
});
