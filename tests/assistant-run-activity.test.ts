import { describe, expect, it } from "vitest";

import { deriveRunOutputting } from "@/lib/assistant-run-activity";

describe("deriveRunOutputting", () => {
  it("keeps processing until durable completion even when presentation completes first", () => {
    expect(
      deriveRunOutputting(
        { runId: "run-1", state: "running" },
        { runId: "run-1", answerComplete: true },
      ),
    ).toBe(true);
  });

  it("keeps outputting while running without answerComplete", () => {
    expect(
      deriveRunOutputting(
        { runId: "run-1", state: "running" },
        { runId: "run-1", answerComplete: false },
      ),
    ).toBe(true);
  });

  it("ignores presentation from another run", () => {
    expect(
      deriveRunOutputting(
        { runId: "run-1", state: "running" },
        { runId: "run-other", answerComplete: true },
      ),
    ).toBe(true);
  });

  it("is false for durable terminal states even without presentation", () => {
    expect(
      deriveRunOutputting({ runId: "run-1", state: "completed" }, null),
    ).toBe(false);
    expect(deriveRunOutputting({ runId: "run-1", state: "failed" }, null)).toBe(
      false,
    );
  });
});
