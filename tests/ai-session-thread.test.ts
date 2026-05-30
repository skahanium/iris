import { describe, expect, it } from "vitest";

import {
  hasAssistantTurnInPanel,
  shouldStartNewAiSession,
} from "@/lib/ai/session-thread";

describe("shouldStartNewAiSession", () => {
  it("starts fresh when panel has no assistant reply", () => {
    expect(
      shouldStartNewAiSession([{ role: "user", content: "mlxg?" }], false),
    ).toBe(true);
  });

  it("continues thread when assistant already replied", () => {
    expect(
      shouldStartNewAiSession(
        [
          { role: "user", content: "良子?" },
          { role: "assistant", content: "他是…" },
        ],
        false,
      ),
    ).toBe(false);
  });

  it("respects explicit new-chat flag", () => {
    expect(
      shouldStartNewAiSession(
        [{ role: "assistant", content: "prior" }],
        true,
      ),
    ).toBe(true);
  });
});

describe("hasAssistantTurnInPanel", () => {
  it("ignores empty assistant placeholder", () => {
    expect(
      hasAssistantTurnInPanel([
        { role: "user", content: "q" },
        { role: "assistant", content: "" },
      ]),
    ).toBe(false);
  });
});
