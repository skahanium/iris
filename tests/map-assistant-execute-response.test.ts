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
          kind: "evidence_sources",
          title: "证据来源",
          status: "ready",
          sourceTask: "research",
          evidenceCount: 2,
          payload: { sources: [{ title: "source" }] },
        },
      ],
    } as AssistantExecuteResponse;

    const items = mapAssistantExecuteToArtifacts(response);
    expect(items).toHaveLength(1);
    expect(items[0]?.kind).toBe("evidence_sources");
    expect(items[0]?.title).toBe("证据来源");
    expect(items[0]?.evidenceCount).toBe(2);
  });

  it("drops legacy wire kinds instead of casting them into artifacts", () => {
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
          kind: ["research", "report"].join("_"),
          title: "旧研究报告",
          status: "ready",
          sourceTask: "research",
          evidenceCount: 0,
          payload: {},
        },
      ],
    } as AssistantExecuteResponse;

    expect(mapAssistantExecuteToArtifacts(response)).toEqual([]);
  });
});
