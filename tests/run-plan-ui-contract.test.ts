import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Run plan UI contract", () => {
  it("feeds process details instead of rendering a standalone run plan layer", () => {
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const facade = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");

    expect(hook).toContain("AgentRunPlanSummary");
    expect(hook).toContain("IntentDetectionResult");
    expect(hook).toContain("PermissionPreflightSummary");
    expect(hook).toContain("blockedCount");
    expect(hook).toContain("confirmationCount");
    expect(panel).toContain("useAssistantRunPlan");
    expect(panel).toContain("runPlanControls: runPlan");
    expect(panel).not.toContain("{runPlan.layer}");
    expect(panel).toContain("runPlanSummary={runPlan.runPlanSummary}");
    expect(tasks).toContain("recordRunPlan");
    expect(tasks).toContain("response.intentDetection ?? null");
    expect(tasks).toContain("response.runPlanSummary ?? null");
    expect(tasks).toContain("response.permissionPreflightSummary ?? null");
    expect(facade).not.toContain("RunPlanSummary");
    expect(facade).not.toContain("RunPlanDrawer");
  });

  it("keeps run plan details folded instead of adding standalone drawers", () => {
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");
    const details = read("src/components/ai/AgentTaskStatusPanel.tsx");

    expect(hook).not.toContain("components/ai/RunPlanSummary");
    expect(hook).not.toContain("components/ai/RunPlanDrawer");
    expect(hook).not.toContain('data-testid="run-plan-drawer"');
    expect(details).toContain("过程详情");
    expect(details).toContain("runPlanSummary");
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
