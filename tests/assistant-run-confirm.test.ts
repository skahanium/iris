import { describe, expect, it } from "vitest";

import type { AssistantTaskStatus } from "../src/types/ai";

/** Mirrors useAssistantRun taskStatus → runState mapping for tool confirm flows. */
function taskStatusToRunState(
  status: AssistantTaskStatus,
  activityHint: string | null,
): string {
  if (status === "awaiting_confirmation") return "awaiting_tool_confirmation";
  if (status === "running") {
    if (activityHint?.includes("拒绝")) return "running";
    if (activityHint?.includes("继续")) return "running";
    return "running";
  }
  if (status === "completed") return "completed";
  if (status === "error") return "error";
  return "idle";
}

describe("assistant run confirm state", () => {
  it("reject path stays running until completed, not error", () => {
    expect(
      taskStatusToRunState("running", "已拒绝，正在生成替代回答…"),
    ).toBe("running");
  });

  it("completed after resume is not error", () => {
    expect(taskStatusToRunState("completed", null)).toBe("completed");
    expect(taskStatusToRunState("error", null)).toBe("error");
  });

  it("pending confirmation maps to awaiting_tool_confirmation", () => {
    expect(taskStatusToRunState("awaiting_confirmation", null)).toBe(
      "awaiting_tool_confirmation",
    );
  });
});
