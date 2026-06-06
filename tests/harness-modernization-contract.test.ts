import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("harness modernization remaining contracts", () => {
  it("context preview uses the same executable tool surface as harness runs", () => {
    const backend = read("src-tauri/src/commands/ai_commands.rs");
    expect(backend).toContain("web_search: Option<bool>");
    expect(backend).toContain("ToolPolicyContext");
    expect(backend).toContain("tools_for_policy_surface(");
    expect(backend).not.toContain(
      "registry.for_scene(scene).into_iter().cloned().collect()",
    );

    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("web_search?: boolean");
    expect(ipc).toContain("webSearch: params.web_search ?? false");

    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("web_search: webSearch");
  });

  it("tool confirmation executes auto tools before pausing on confirm", () => {
    const toolTurn = read("src-tauri/src/ai_harness/tool_turn.rs");
    expect(toolTurn).toContain("outstanding_confirm_tool");

    const run = read("src-tauri/src/ai_harness/harness/run.rs");
    expect(run).toContain("outstanding_confirm_tool");
    expect(run).toContain("pause_for_tool_confirmation");
    expect(run).toContain(
      "if registry.requires_confirmation(&tool_call.function.name)",
    );
    expect(
      run.indexOf("requires_confirmation(&tool_call.function.name)"),
    ).toBeLessThan(
      run.lastIndexOf(
        "outstanding_confirm_tool(&registry, &messages, &policy_ctx)",
      ),
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
