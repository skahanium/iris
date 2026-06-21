import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Agent Task Runtime Phase E UI contract", () => {
  it("syncs task, step, event DTOs through typed IPC wrappers", () => {
    const types = read("src/types/ipc.ts");
    const ipc = read("src/lib/ipc.ts");
    const backend = read("src-tauri/src/commands/ai_commands.rs");
    const lib = read("src-tauri/src/lib.rs");

    expect(types).toContain("export interface AgentTaskDto");
    expect(types).toContain("export interface AgentTaskStepDto");
    expect(types).toContain("export interface AgentTaskEventDto");
    expect(types).toContain("export type AgentTaskStatus");

    expect(ipc).toContain("export async function agentTaskSteps");
    expect(ipc).toContain("export async function agentTaskEvents");
    expect(ipc).toContain("invoke<AgentTaskStepDto[]>");
    expect(ipc).toContain("invoke<AgentTaskEventDto[]>");

    expect(backend).toContain("pub async fn agent_task_steps");
    expect(backend).toContain("pub async fn agent_task_events");
    expect(lib).toContain("commands::ai_commands::agent_task_steps");
    expect(lib).toContain("commands::ai_commands::agent_task_events");
  });

  it("renders complex task status in assistant surfaces without exposing raw internals", () => {
    const panel = read("src/components/ai/AgentTaskStatusPanel.tsx");
    const surfaces = read("src/components/ai/AssistantTaskSurfaces.tsx");
    const unified = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const assistantTasks = read("src/components/ai/hooks/useAssistantTasks.ts");
    const taskStatusHook = read(
      "src/components/ai/hooks/useAgentTaskStatus.ts",
    );

    expect(panel).toContain("AgentTaskStatusPanel");
    expect(panel).toContain('kind !== "complex"');
    expect(panel).toContain("过程详情");
    expect(panel).toContain("onOpenArtifact");
    expect(panel).toContain("权限等待");
    expect(panel).not.toContain("checkpoint_json");
    expect(panel).not.toContain("payload_json");
    expect(panel).not.toContain("noteContent");
    expect(panel).not.toContain("apiKey");

    expect(surfaces).not.toContain("<AssistantProcessStatusBar");
    expect(surfaces).toContain("AssistantArtifactTagStrip");
    expect(unified).toContain("AgentTaskStatusPanel");
    expect(unified).toContain("AssistantProcessStatusBar");
    expect(unified).toContain("agentTaskId");
    expect(unified).toContain("useAgentTaskStatus");
    expect(assistantTasks).toContain("setAgentTaskId(response.taskId");
    expect(taskStatusHook).toContain("agentTaskSteps");
    expect(taskStatusHook).toContain("agentTaskEvents");
    expect(taskStatusHook).toContain("agentTaskAbort");
  });
});
