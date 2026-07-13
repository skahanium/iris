import { describe, expect, it } from "vitest";

import { isAssistantRunBusy } from "@/hooks/useAssistantRun";

describe("isAssistantRunBusy", () => {
  it("keeps the composer available while a Run awaits confirmation", () => {
    expect(isAssistantRunBusy("awaiting_confirmation")).toBe(false);
  });

  it("blocks the composer for persisted dispatch states", () => {
    expect(isAssistantRunBusy("accepted")).toBe(true);
    expect(isAssistantRunBusy("preparing")).toBe(true);
    expect(isAssistantRunBusy("running")).toBe(true);
    expect(isAssistantRunBusy("verifying")).toBe(true);
  });
});
