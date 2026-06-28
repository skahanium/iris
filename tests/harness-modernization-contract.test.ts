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

  it("uses TaskPlan summaries and drops meaningless process placeholders", () => {
    expect(read("src-tauri/src/ai_runtime/task_plan.rs")).toContain(
      "TaskPlanSummary",
    );
    expect(read("src-tauri/src/ai_harness/harness_task.rs")).not.toContain(
      "assistant workflow output summarized by artifact metadata",
    );
  });

  it("keeps direct SkillHub confirmation on unified task_process artifact wires", () => {
    const assistantCommands = read(
      "src-tauri/src/commands/assistant_commands.rs",
    );

    expect(assistantCommands).not.toContain('kind: "tool_confirmation"');
    expect(assistantCommands).toContain('kind: "task_process"');
    expect(assistantCommands).toContain('"schema": "task_process"');
    expect(assistantCommands).toContain('"tool_name": "skills_install"');
    expect(assistantCommands).toContain(
      '"next_action": "wait_for_user_confirmation"',
    );
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

  it("agent task runtime exposes task-id based resume and abort IPC", () => {
    const backend = read("src-tauri/src/commands/ai_commands.rs");
    expect(backend).toContain("pub async fn agent_task_resume");
    expect(backend).toContain("pub async fn agent_task_abort");
    expect(backend).toContain("pub async fn agent_task_get");
    expect(backend).toContain("pub async fn agent_task_list");
    expect(backend).toContain("AgentTaskRuntime::prepare_resume_plan");
    expect(backend).toContain("preflight_agent_task_resume(");
    expect(backend).toContain("AgentTaskRuntime::begin_resume");
    const resumeCommand = backend.slice(
      backend.indexOf("pub async fn agent_task_resume"),
      backend.indexOf("/// Abort a durable Agent Task"),
    );
    expect(resumeCommand.indexOf("preflight_agent_task_resume(")).toBeLessThan(
      resumeCommand.indexOf("resume_harness_after_tool_confirm_or_restore("),
    );
    expect(
      resumeCommand.indexOf("AgentTaskRuntime::begin_resume"),
    ).toBeLessThan(
      resumeCommand.indexOf("resume_harness_after_tool_confirm_or_restore("),
    );

    const lib = read("src-tauri/src/lib.rs");
    expect(lib).toContain("commands::ai_commands::agent_task_resume");
    expect(lib).toContain("commands::ai_commands::agent_task_abort");
    expect(lib).toContain("commands::ai_commands::agent_task_get");
    expect(lib).toContain("commands::ai_commands::agent_task_list");

    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("export async function agentTaskResume");
    expect(ipc).toContain("export async function agentTaskAbort");
    expect(ipc).toContain("export async function agentTaskGet");
    expect(ipc).toContain("export async function agentTaskList");
  });

  it("paused-budget chat can be resumed by durable task id", () => {
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");
    const resume = read("src/components/ai/hooks/useAssistantHarnessResume.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const run = read("src-tauri/src/ai_harness/harness/run.rs");

    expect(tasks).toContain("setPausedTaskId");
    expect(tasks).toContain('result.status === "paused_budget"');
    expect(resume).toContain("agentTaskResume");
    expect(resume).toContain("pausedTaskId");
    expect(panel).toContain("pausedTaskId");
    expect(run).toContain(
      "finish_reason: HarnessFinishReason::BudgetExhausted",
    );
    expect(run).toContain(
      "round_limit_without_budget_exhaustion_completes_with_fallback",
    );
    expect(run.indexOf("save_round_checkpoint(")).toBeLessThan(
      run.lastIndexOf("finish_run("),
    );
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

  it("skills lifecycle exposes update and capability diagnostics to the UI", () => {
    const ipc = read("src/lib/ipc.ts");
    expect(ipc).toContain("export async function skillsUpdate");
    expect(ipc).toContain("export async function skillsPrepareWorkspace");
    expect(ipc).toContain("content_hash?: string");
    expect(ipc).toContain("capability_preview?:");
    expect(ipc).toContain("availability:");

    const panel = read("src/components/ai/SkillsPanel.tsx");
    expect(panel).toContain("skillsUpdate");
    expect(panel).toContain("权限摘要");
    expect(panel).toContain("已准备");
    expect(panel).toContain("工作区");
  });

  it("tool confirmation suppresses duplicate resume calls for the same tool call", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain("toolConfirmInFlightRef");
    expect(panel).toContain("toolConfirmSettledRef");
    expect(panel).toContain("toolConfirmInFlightRef.current.has(confirmKey)");
    expect(panel).toContain("toolConfirmSettledRef.current.has(confirmKey)");
    expect(panel).toContain("toolConfirmSettledRef.current.add(confirmKey)");
  });

  it("composer typing avoids eager conversation and vault-wide mention work", () => {
    const surface = read("src/components/ai/ConversationSurface.tsx");
    expect(surface).toContain("memo(function ConversationSurface");

    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain(
      "mentionOpen ? buildMentionCandidates(vaultFiles, mentionQuery) : []",
    );
    expect(panel).toContain("const handleQuoteToInput = useCallback");
    expect(panel).toContain("onQuoteToInput={handleQuoteToInput}");
  });
});
