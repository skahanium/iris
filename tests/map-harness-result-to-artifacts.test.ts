import { describe, expect, it } from "vitest";

import {
  mapChatResultToArtifacts,
  mapCitationToArtifacts,
  mapWritingToArtifacts,
} from "../src/lib/map-harness-result-to-artifacts";

describe("mapHarnessResultToArtifacts", () => {
  it("maps chat message and pending confirmation", () => {
    const artifacts = mapChatResultToArtifacts({
      request_id: "r1",
      session_id: 1,
      status: "pending_tools",
      content: "partial",
      tool_calls: [{ id: "tc1", function: { name: "fetch_web_page" } }],
      tool_results: [{ tool_call_id: "tc1", status: "pending_confirmation" }],
    });
    expect(artifacts.some((a) => a.kind === "message")).toBe(true);
    expect(artifacts.some((a) => a.kind === "tool_confirmation")).toBe(true);
  });

  it("maps writing patches", () => {
    const artifacts = mapWritingToArtifacts({
      request_id: "w1",
      suggestions: [],
      patches: [
        {
          id: "p1",
          target_path: "a.md",
          base_content_hash: "abc",
          range: { start: 0, end: 1 },
          original_text: "a",
          replacement_text: "b",
          evidence_packet_ids: [],
          risk_level: "low",
          warnings: [],
          created_at: "2026-01-01T00:00:00.000Z",
        },
      ],
      evidence_used: [],
      total_tokens: {
        prompt_tokens: 1,
        completion_tokens: 1,
        total_tokens: 2,
      },
    });
    expect(artifacts[0]?.kind).toBe("patches");
  });

  it("maps citation report", () => {
    const artifacts = mapCitationToArtifacts({
      request_id: "c1",
      claims: [],
      coverage: "well_supported",
      evidence_used: [],
      total_tokens: {
        prompt_tokens: 0,
        completion_tokens: 0,
        total_tokens: 0,
      },
      suggestions: [],
    });
    expect(artifacts[0]?.kind).toBe("citation_report");
  });
});
