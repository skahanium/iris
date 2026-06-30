import { describe, expect, it } from "vitest";

import {
  resolveAssistantDisplayContent,
  resolveAssistantReconcileContent,
} from "@/lib/assistant-message-content";

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

  it("does not replace an equivalent final answer during reconcile", () => {
    expect(
      resolveAssistantReconcileContent({
        currentContent: "最终回答",
        serverContent: " 最终回答 ",
        streamBuffer: "最终回答",
        toolCalls: undefined,
      }),
    ).toEqual({
      content: "最终回答",
      mutation: "noop",
      reason: "equivalent_noop",
    });
  });

  it("uses authoritative server content when it differs after visible streaming", () => {
    expect(
      resolveAssistantReconcileContent({
        currentContent: "流式草稿",
        serverContent: "最终回答",
        streamBuffer: "流式草稿",
        toolCalls: undefined,
      }),
    ).toEqual({
      content: "最终回答",
      mutation: "replace",
      reason: "server_authoritative",
    });
  });

  it("fills an empty visible stream from server content", () => {
    expect(
      resolveAssistantReconcileContent({
        currentContent: "",
        serverContent: "最终回答",
        streamBuffer: "",
        toolCalls: undefined,
      }),
    ).toEqual({
      content: "最终回答",
      mutation: "replace",
      reason: "server_fills_empty_stream",
    });
  });
});
