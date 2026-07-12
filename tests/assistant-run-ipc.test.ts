import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Assistant Run IPC contract", () => {
  it("registers only the three new Run commands beside the legacy facade", () => {
    const lib = read("src-tauri/src/lib.rs");

    expect(lib).toContain("commands::assistant_commands::assistant_run_start");
    expect(lib).toContain("commands::assistant_commands::assistant_run_control");
    expect(lib).toContain("commands::assistant_commands::assistant_run_get");
  });

  it("exposes typed wrappers through ipc.ts rather than direct component invoke", () => {
    const ipc = read("src/lib/ipc.ts");

    expect(ipc).toContain('invoke<AssistantRunAccepted>("assistant_run_start"');
    expect(ipc).toContain('invoke<void>("assistant_run_control"');
    expect(ipc).toContain('invoke<AssistantRunGetResponse | null>("assistant_run_get"');
  });

  it("uses one dedicated replayable Run event channel instead of llm token forwarding", () => {
    const events = read("src/lib/ipc-events.ts");
    const ipc = read("src/lib/ipc.ts");
    const engine = read("src-tauri/src/ai_runtime/run_engine.rs");

    expect(events).toContain('ASSISTANT_RUN_EVENT: "assistant_run_event"');
    expect(ipc).toContain("listenAssistantRunEvent");
    expect(ipc).toContain("IPC_EVENTS.ASSISTANT_RUN_EVENT");
    expect(engine).toContain('emit("assistant_run_event"');
    expect(engine).not.toContain('emit("llm:token"');
  });

  it("keeps the new command facade independent from the legacy execute request", () => {
    const facade = read("src-tauri/src/commands/assistant_commands.rs");
    const start = facade.split("pub async fn assistant_run_start")[1] ?? "";

    expect(start).toContain("AssistantRunStartRequest");
    expect(start).toContain("RunIntake::start");
    expect(start).not.toContain("route_assistant_execute");
    expect(start).not.toContain("run_harness_task");
  });
});
