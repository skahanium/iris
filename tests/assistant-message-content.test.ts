import { describe, expect, it } from "vitest";

import { resolveAssistantDisplayContent } from "@/lib/assistant-message-content";

describe("resolveAssistantDisplayContent", () => {
  it("prefers server content over stream buffer", () => {
    expect(resolveAssistantDisplayContent("hello", "world", undefined)).toBe(
      "hello",
    );
  });

  it("uses stream buffer when server empty", () => {
    expect(resolveAssistantDisplayContent("", "streamed", undefined)).toBe(
      "streamed",
    );
  });

  it("falls back to tool summaries", () => {
    expect(
      resolveAssistantDisplayContent("", "", [
        {
          id: "1",
          name: "spawn_subagent",
          status: "completed",
          result_summary: "Found 3 notes",
        },
      ]),
    ).toBe("Found 3 notes");
  });

  it("shows explicit message when everything empty", () => {
    expect(resolveAssistantDisplayContent("", "", undefined)).toContain(
      "模型未返回正文",
    );
  });
});
