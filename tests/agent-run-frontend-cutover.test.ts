import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Agent Run frontend cutover", () => {
  it("has no scene, task-plan, or assistant-execute type model", () => {
    const types = read("src/types/ai.ts");

    for (const removed of [
      "AiScene",
      "TaskPlan",
      "AssistantExecute",
      "AgentRunPlanSummary",
      "IntentDetectionResult",
      "AssistantActionState",
      "RuntimeDocumentSnapshot",
      "WritingEditorContext",
      "ContextPacket",
    ]) {
      expect(types).not.toContain(removed);
    }
  });

  it("does not select connectivity by scene or persist a browser scene", () => {
    const hook = read("src/hooks/useConnectivityStatus.ts");
    const ipc = read("src/lib/ipc.ts");
    const llm = read("src/types/llm.ts");

    expect(hook).not.toContain("scene");
    expect(hook).not.toContain("sessionStorage");
    expect(ipc).not.toMatch(/connectivityStatus\s*\(\s*scene/);
    expect(llm).not.toContain("scene:");
    expect(existsSync("src/lib/ai/scene-types.ts")).toBe(false);
  });

  it("does not retain packet payload UI or packet-derived citation helpers", () => {
    const payloadStore = read("src/lib/ai-payload-store.ts");
    for (const removed of [
      "task_event",
      "artifact_payload",
      "document_summary",
      "research_payload",
      "evidence_packet",
    ]) {
      expect(payloadStore).not.toContain(removed);
    }

    for (const path of [
      "src/components/ai/ContextPacketCard.tsx",
      "src/components/ai/EvidenceChainView.tsx",
      "src/components/ai/hooks/useCitationClick.ts",
      "src/lib/ai/evidence-citations.ts",
      "src/lib/ai/merge-context-packets.ts",
    ]) {
      expect(existsSync(path)).toBe(false);
    }
  });
});
