import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("IPC event registry", () => {
  it("keeps frontend Tauri event names in one typed registry", () => {
    const registry = read("src/lib/ipc-events.ts");
    const ipc = read("src/lib/ipc.ts");

    for (const eventName of [
      "version:save_complete",
      "file:changed",
      "classified:file_taken",
      "skills:changed",
      "assistant:run_event",
      "embedding-index-progress",
      "app-update:status",
      "app-update:progress",
    ]) {
      expect(registry).toContain(eventName);
      expect(ipc).not.toContain(`listen<${eventName}`);
    }

    expect(registry).not.toContain("ai:harness_trace");
    expect(registry).not.toContain("llm:token");
    expect(registry).not.toContain("ai:tool_confirm_request");
    expect(ipc).toContain("IPC_EVENTS.ASSISTANT_RUN_EVENT");
    expect(ipc).toContain("listenAssistantRunEvent");
    expect(registry).toContain("export type IpcEventName");
  });

  it("uses registry constants for every Tauri listen wrapper", () => {
    const ipc = read("src/lib/ipc.ts");
    const listenCalls = [...ipc.matchAll(/listen(?:<[^>]+>)?\(([^,]+)/g)].map(
      (match) => match[1]?.trim(),
    );

    expect(listenCalls.length).toBeGreaterThan(0);
    expect(listenCalls.every((arg) => arg?.startsWith("IPC_EVENTS."))).toBe(
      true,
    );
  });
});
