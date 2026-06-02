import { describe, expect, it } from "vitest";

import { mapAssistantExecuteToArtifacts } from "@/lib/map-assistant-execute-response";
import type { AssistantExecuteResponse } from "@/types/ai";

describe("mapAssistantExecuteToArtifacts", () => {
  it("maps server artifact wires when present", () => {
    const response = {
      kind: "chat",
      payload: {
        request_id: "r1",
        session_id: 1,
        content: "hi",
        status: "completed",
      },
      requestId: "r1",
      runStatus: "completed",
      artifacts: [
        {
          kind: "message",
          title: "回答",
          status: "ready",
          sourceTask: "chat",
          evidenceCount: 2,
          payload: { content: "hi" },
        },
      ],
    } as AssistantExecuteResponse;

    const items = mapAssistantExecuteToArtifacts(response);
    expect(items).toHaveLength(1);
    expect(items[0]?.title).toBe("回答");
    expect(items[0]?.evidenceCount).toBe(2);
  });
});
