import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase2 run plan UI contract", () => {
  it("keeps run plan data internal instead of exposing a user-facing panel", () => {
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const facade = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");

    expect(hook).toContain("AgentRunPlanSummary");
    expect(hook).toContain("IntentDetectionResult");
    expect(hook).toContain("PermissionPreflightSummary");
    expect(hook).toContain("layer: null");
    expect(panel).toContain("useAssistantRunPlan");
    expect(panel).toContain("runPlanControls: runPlan");
    expect(panel).not.toContain("{runPlan.layer}");
    expect(tasks).toContain("recordRunPlan");
    expect(tasks).toContain("response.intentDetection ?? null");
    expect(tasks).toContain("response.runPlanSummary ?? null");
    expect(tasks).toContain("response.permissionPreflightSummary ?? null");
    expect(facade).not.toContain("RunPlanSummary");
    expect(facade).not.toContain("RunPlanDrawer");
  });

  it("does not import or render run plan UI components", () => {
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");

    expect(hook).not.toContain("components/ai/RunPlanSummary");
    expect(hook).not.toContain("components/ai/RunPlanDrawer");
    expect(hook).not.toContain('data-testid="run-plan-summary"');
    expect(hook).not.toContain('data-testid="run-plan-drawer"');
  });

  it("does not expose sensitive full-content fields in run plan props", () => {
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");

    expect(hook).not.toContain("noteContent");
    expect(hook).not.toContain("base64");
    expect(hook).not.toContain("clipboard");
    expect(hook).not.toContain("apiKey");
    expect(hook).not.toContain("shellOutput");
  });
});
