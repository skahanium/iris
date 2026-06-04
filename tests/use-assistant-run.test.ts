import { describe, expect, it } from "vitest";

import { isAssistantRunBusy } from "@/hooks/useAssistantRun";

describe("isAssistantRunBusy", () => {
  it("does not block chrome while awaiting tool confirmation", () => {
    expect(isAssistantRunBusy("awaiting_tool_confirmation")).toBe(false);
  });

  it("blocks while running", () => {
    expect(isAssistantRunBusy("running")).toBe(true);
  });
});
