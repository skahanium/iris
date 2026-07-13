import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Assistant Run IPC contract", () => {
  it("registers the sole Run lifecycle and domain-routed session commands", () => {
    const lib = read("src-tauri/src/lib.rs");

    for (const command of [
      "assistant_run_start",
      "assistant_run_control",
      "assistant_run_get",
      "assistant_session_list",
      "assistant_session_load",
      "assistant_session_rename",
      "assistant_session_delete",
      "assistant_session_retract",
    ]) {
      expect(lib).toContain(`commands::assistant_commands::${command}`);
    }

    for (const removed of [
      "assistant_execute",
      "context_assemble",
      "ai_send_message",
      "tool_confirm",
      "session_list",
      "session_load",
      "agent_task_resume",
      "harness_resume",
      "writing_execute",
      "citation_check",
      "organize_execute",
      "chapter_writing_execute",
      "document_check_execute",
    ]) {
      expect(lib).not.toContain(`commands::assistant_commands::${removed}`);
    }
  });

  it("exposes only typed Run/session lifecycle wrappers", () => {
    const ipc = read("src/lib/ipc.ts");

    expect(ipc).toContain('invoke<AssistantRunAccepted>("assistant_run_start"');
    expect(ipc).toContain('invoke<void>("assistant_run_control"');
    expect(ipc).toContain(
      'invoke<AssistantRunGetResponse | null>("assistant_run_get"',
    );

    for (const removed of [
      "assistantExecute",
      "contextAssemble",
      "aiSendMessage",
      "toolConfirm",
      "sessionList",
      "sessionLoad",
      "agentTaskResume",
      "harnessResume",
      "writingExecute",
      "citationCheck",
      "organizeExecute",
      "chapterWritingExecute",
      "documentCheckExecute",
      "classifiedAiThreadLoad",
    ]) {
      expect(ipc).not.toContain(`function ${removed}`);
    }
  });

  it("uses one durable replayable Run event channel", () => {
    const events = read("src/lib/ipc-events.ts");
    const ipc = read("src/lib/ipc.ts");
    const engine = read("src-tauri/src/ai_runtime/run_engine.rs");

    expect(events).toContain('ASSISTANT_RUN_EVENT: "assistant:run_event"');
    expect(ipc).toContain("listenAssistantRunEvent");
    expect(ipc).toContain("IPC_EVENTS.ASSISTANT_RUN_EVENT");
    expect(engine).toContain('emit("assistant:run_event"');

    for (const removed of [
      "LLM_TOKEN",
      "LLM_DONE",
      "LLM_ERROR",
      "LLM_RESET",
      "AI_RETRY_STATUS",
      "HARNESS_TRACE",
      "AI_THINKING",
      "AI_REQUEST_STARTED",
      "TOOL_CONFIRM_REQUEST",
    ]) {
      expect(events).not.toContain(removed);
    }
  });

  it("keeps Run intake independent from legacy intent, scene, and harness routing", () => {
    const facade = read("src-tauri/src/commands/assistant_commands.rs");
    const start =
      (facade.split("pub async fn assistant_run_start")[1] ?? "").split(
        "#[cfg(test)]",
      )[0] ?? "";

    expect(start).toContain("AssistantRunStartRequest");
    expect(start).toContain("RunIntake::start");
    expect(start).not.toContain("AgentIntent");
    expect(start).not.toContain("route_assistant_execute");
    expect(start).not.toContain("run_harness_task");
  });

  it("has no standalone Research executor, event, or artifact surface", () => {
    const lib = read("src-tauri/src/lib.rs");
    const ipc = read("src/lib/ipc.ts");
    const events = read("src/lib/ipc-events.ts");
    expect(lib).not.toContain("research_execute");
    expect(ipc).not.toContain("researchExecute");
    expect(events).not.toContain("RESEARCH_PROGRESS");
    expect(existsSync("src/types/assistant-artifact.ts")).toBe(false);
    expect(existsSync("src/components/layout/ArtifactWorkspaceView.tsx")).toBe(
      false,
    );
  });
});
