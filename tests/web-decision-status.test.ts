import { describe, expect, it } from "vitest";

import { formatWebDecisionStatus } from "@/lib/web-decision-status";
import { normalizeFreshness } from "@/types/ai";

describe("normalizeFreshness", () => {
  it("maps legacy wire values onto the binary model", () => {
    expect(normalizeFreshness("offline")).toBe("offline");
    expect(normalizeFreshness("online")).toBe("online");
    expect(normalizeFreshness("web_preferred")).toBe("online");
    expect(normalizeFreshness("web_required")).toBe("online");
    expect(normalizeFreshness(undefined)).toBe("offline");
    expect(normalizeFreshness("unknown")).toBe("offline");
  });
});

describe("formatWebDecisionStatus", () => {
  it("returns null when no decision signal is present", () => {
    expect(formatWebDecisionStatus({})).toBeNull();
  });

  it("labels offline decisions with a short reason", () => {
    expect(
      formatWebDecisionStatus({
        freshness: "offline",
        webReason: "local_transformation",
      }),
    ).toBe("离线（本地变换）");
  });

  it("maps legacy web_preferred onto online availability", () => {
    expect(
      formatWebDecisionStatus({
        freshness: "web_preferred",
        webReason: "general_question",
      }),
    ).toBe("在线（搜索可用）");
  });

  it("upgrades to searched once a web tool completed", () => {
    expect(
      formatWebDecisionStatus({
        freshness: "online",
        webReason: "default_online",
        searched: true,
      }),
    ).toBe("在线（已搜索）");
  });
});
