import { describe, expect, it } from "vitest";

import { mapChatToolCallsForUi } from "@/lib/map-chat-tool-calls";

describe("mapChatToolCallsForUi", () => {
  it("hides completed read-only tools", () => {
    const out = mapChatToolCallsForUi(
      [
        {
          id: "c1",
          function: { name: "web_search", arguments: '{"query":"today"}' },
        },
      ],
      [{ tool_call_id: "c1", status: "completed" }],
    );
    expect(out).toBeUndefined();
  });

  it("shows completed spawn_subagent with result summary", () => {
    const out = mapChatToolCallsForUi(
      [
        {
          id: "sub1",
          function: {
            name: "spawn_subagent",
            arguments: '{"task":"检索法规"}',
          },
        },
      ],
      [
        {
          tool_call_id: "sub1",
          status: "completed",
          result: { content: "找到 3 条相关法规摘要。" },
        },
      ],
    );
    expect(out).toHaveLength(1);
    expect(out?.[0]?.name).toBe("spawn_subagent");
    expect(out?.[0]?.status).toBe("completed");
    expect(out?.[0]?.result_summary).toContain("法规");
  });

  it("summarizes request-ordered subagent batch reports", () => {
    const out = mapChatToolCallsForUi(
      [
        {
          id: "batch1",
          function: {
            name: "spawn_subagent",
            arguments: '{"tasks":[{"task":"核验 A"},{"task":"核验 B"}]}',
          },
        },
      ],
      [
        {
          tool_call_id: "batch1",
          status: "completed",
          result: {
            subagentBatchReport: {
              items: [
                { summary: "A 已核验", errors: [] },
                {
                  summary: "",
                  errors: ["child_run_web_evidence_required"],
                },
              ],
            },
          },
        },
      ],
    );

    expect(out?.[0]?.result_summary).toBe(
      "A 已核验\nchild_run_web_evidence_required",
    );
  });

  it("keeps pending confirmation tools", () => {
    const out = mapChatToolCallsForUi(
      [
        {
          id: "c2",
          function: { name: "replace_selection", arguments: "{}" },
        },
      ],
      [{ tool_call_id: "c2", status: "pending_confirmation" }],
    );
    expect(out).toHaveLength(1);
    expect(out?.[0]?.status).toBe("pending");
    expect(out?.[0]?.name).toBe("replace_selection");
  });
});
