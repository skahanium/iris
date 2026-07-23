import { describe, expect, it } from "vitest";

import {
  deriveDisplayRunState,
  deriveRunOutputting,
} from "@/lib/assistant-run-activity";

describe("deriveRunOutputting", () => {
  it("ends outputting when presentation answerComplete arrives before durable completed", () => {
    expect(
      deriveRunOutputting(
        { runId: "run-1", state: "running" },
        { runId: "run-1", answerComplete: true },
      ),
    ).toBe(false);
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

describe("deriveDisplayRunState", () => {
  it("hides busy badge when outputting has ended", () => {
    expect(deriveDisplayRunState("running", false)).toBe("idle");
    expect(deriveDisplayRunState("running", true)).toBe("running");
    expect(deriveDisplayRunState("completed", false)).toBe("completed");
  });
});
