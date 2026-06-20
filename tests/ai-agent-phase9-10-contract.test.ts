import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI Agent phase 9/10 frontend and sandbox contracts", () => {
  it("polls live agent tasks and stops on terminal statuses", () => {
    const hook = read("src/components/ai/hooks/useAgentTaskStatus.ts");

    expect(hook).toContain("POLLABLE_TASK_STATUSES");
    expect(hook).toContain("TERMINAL_TASK_STATUSES");
    expect(hook).toContain("window.setInterval");
    expect(hook).toContain("window.clearInterval");
    expect(hook).toContain("paused_budget");
    expect(hook).toContain("paused_recoverable");
  });

  it("feeds run plan state into folded process details", () => {
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(hook).toContain("permissionPreflightSummary");
    expect(hook).toContain("intentDetection");
    expect(hook).toContain("runPlanSummary");
    expect(panel).toContain("runPlanSummary={runPlan.runPlanSummary}");
    expect(panel).not.toContain("{runPlan.layer}");
  });

  it("keeps assistant task wiring grouped instead of a flat parameter surface", () => {
    const hook = read("src/components/ai/hooks/useAssistantTasks.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(hook).toContain("interface AssistantTaskRuntimePorts");
    expect(hook).toContain("interface AssistantTaskContext");
    expect(hook).toContain("interface AssistantTaskRefs");
    expect(hook).toContain("interface AssistantTaskStatePorts");
    expect(hook).toContain("runtime:");
    expect(hook).toContain("context:");
    expect(hook).toContain("refs:");
    expect(hook).toContain("state:");
    expect(panel).toContain("runtime: {");
    expect(panel).toContain("context: {");
    expect(panel).toContain("refs: {");
    expect(panel).toContain("state: {");
  });

  it("shows permission, sandbox, confirmation index, and skill trust warnings", () => {
    const dialog = read("src/components/ai/ToolConfirmDialog.tsx");
    const types = read("src/types/ipc.ts");

    expect(types).toContain("SandboxProfileSummary");
    expect(types).toContain("sandboxProfile?: SandboxProfileSummary");
    expect(dialog).toContain("request.permissionDecision");
    expect(dialog).toContain("request.sandboxProfile");
    expect(dialog).toContain("pendingConfirmationIndex");
    expect(dialog).toContain("trust_profile_preview");
    expect(dialog).toContain('data-testid="tool-confirm-sandbox-profile"');
    expect(dialog).not.toContain("checkpoint_json");
    expect(dialog).not.toContain("noteContent");
    expect(dialog).not.toContain("apiKey");
    expect(dialog).not.toContain("password");
  });
});
