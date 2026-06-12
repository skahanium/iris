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

  it("returns flattened harness metadata on AssistantExecuteResponse", () => {
    const facade = read("src-tauri/src/commands/assistant_commands.rs");
    expect(facade).toContain("pub struct AssistantExecuteResponse");
    expect(facade).toContain("artifacts:");
    expect(facade).toContain("intent_detection:");
    expect(facade).toContain("run_plan_summary:");
    expect(facade).toContain("permission_preflight_summary:");
    const types = read("src/types/ai.ts");
    expect(types).toContain("HarnessArtifactWire");
    expect(types).toContain("runStatus");
    expect(types).toContain("IntentDetectionResult");
    expect(types).toContain("AgentRunPlanSummary");
  });

  it("accepts Phase2 agentIntent while keeping legacy intent optional", () => {
    const facade = read("src-tauri/src/commands/assistant_commands.rs");
    expect(facade).toContain("pub agent_intent: Option<AgentIntent>");
    expect(facade).toContain("pub intent: Option<AssistantIntent>");
    expect(facade).toContain("effective_agent_intent");

    const types = read("src/types/ai.ts");
    expect(types).toContain("agentIntent?: AgentIntent");
    expect(types).toContain("intent?: AssistantIntent");
  });

  it("routes intents via harness_task layer", () => {
    const facade = read("src-tauri/src/commands/assistant_commands.rs");
    expect(facade).toContain("run_harness_task");

    const router = read("src-tauri/src/ai_harness/harness_task.rs");
    expect(router).toContain("AssistantIntent::Writing");
    expect(router).toContain("AssistantIntent::Research");
    expect(router).toContain("AssistantIntent::Document");
  });

  it("UnifiedAssistantPanel calls assistantExecute", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");

    expect(panel).toContain("assistantExecute(");
    expect(panel).toContain("agentIntent");
    expect(panel).not.toContain("writingExecute(");
    expect(panel).not.toContain("researchExecute(");
    expect(tasks).toContain("explicitIntentDetection(");
    expect(tasks).not.toContain("intentDetection: null");
  });
});
