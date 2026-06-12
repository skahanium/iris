import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Phase2 run plan UI contract", () => {
  it("exposes minimal run plan summary and drawer components", () => {
    const summary = read("src/components/ai/RunPlanSummary.tsx");
    const drawer = read("src/components/ai/RunPlanDrawer.tsx");

    expect(summary).toContain("AgentRunPlanSummary");
    expect(summary).toContain('data-testid="run-plan-summary"');
    expect(summary).toContain("intentDetection");
    expect(summary).toContain("permissionPreflightSummary");

    expect(drawer).toContain('data-testid="run-plan-drawer"');
    expect(drawer).toContain("Intent");
    expect(drawer).toContain("Context");
    expect(drawer).toContain("Permissions");
    expect(drawer).toContain("Progress");
    expect(drawer).toContain("alternatives");
    expect(drawer).toContain("fallbackBehavior");
  });

  it("wires run plan visibility into the unified assistant panel", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const facade = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const hook = read("src/components/ai/hooks/useAssistantRunPlan.tsx");

    expect(panel).toContain("useAssistantRunPlan");
    expect(panel).toContain("runPlan.layer");
    expect(hook).toContain("RunPlanSummary");
    expect(hook).toContain("RunPlanDrawer");
    expect(facade).toContain("RunPlanSummary");
    expect(facade).toContain("RunPlanDrawer");
  });

  it("does not expose sensitive full-content fields in run plan props", () => {
    const summary = read("src/components/ai/RunPlanSummary.tsx");
    const drawer = read("src/components/ai/RunPlanDrawer.tsx");
    const combined = `${summary}\n${drawer}`;

    expect(combined).not.toContain("noteContent");
    expect(combined).not.toContain("base64");
    expect(combined).not.toContain("clipboard");
    expect(combined).not.toContain("apiKey");
    expect(combined).not.toContain("shellOutput");
  });
});
