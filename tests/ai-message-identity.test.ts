import { describe, expect, it } from "vitest";

import {
  assistantMessageIdentity,
  assistantSessionIdentity,
} from "@/lib/ai-message-identity";

describe("assistantMessageIdentity", () => {
  it("prefers runId over clientRequestId, seq and index", () => {
    expect(
      assistantMessageIdentity(
        {
          role: "assistant",
          runId: "run-a",
          clientRequestId: "request-a",
          turnId: "turn-a",
          seq: 3,
        },
        7,
      ),
    ).toBe("run:run-a|assistant|turn-a");
  });

  it("falls back through request, seq and index", () => {
    expect(
      assistantMessageIdentity(
        { role: "user", clientRequestId: "request-b", seq: 4 },
        9,
      ),
    ).toBe("request:request-b|user|");
    expect(
      assistantMessageIdentity({ role: "system", seq: 12 }, 5),
    ).toBe("seq:12|system|");
    expect(assistantMessageIdentity({ role: "system" }, 5)).toBe(
      "index:5|system|",
    );
  });

  it("keeps the same identity while content streams", () => {
    const base = { role: "assistant" as const, runId: "run-streaming" };
    expect(assistantMessageIdentity(base, 0)).toBe(
      assistantMessageIdentity(base, 0),
    );
  });
});

describe("assistantSessionIdentity", () => {
  it("uses domain and sessionKey when present", () => {
    expect(
      assistantSessionIdentity({ domain: "normal", sessionKey: "abc" }),
    ).toBe("normal:abc");
  });

  it("uses a stable new-session identity for a new chat", () => {
    expect(assistantSessionIdentity(null)).toBe("new-session");
    expect(assistantSessionIdentity(undefined)).toBe("new-session");
  });
});
