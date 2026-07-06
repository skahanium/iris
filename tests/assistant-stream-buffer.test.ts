import { describe, expect, it } from "vitest";

import { AssistantStreamBuffer } from "@/lib/assistant-stream-buffer";

describe("AssistantStreamBuffer", () => {
  it("accumulates chunks with length and hash without exposing raw content in summaries", () => {
    const buffer = new AssistantStreamBuffer();
    buffer.append("hello");
    buffer.append(" world");

    expect(buffer.length).toBe(11);
    expect(buffer.toString()).toBe("hello world");
    expect(buffer.summary()).toMatchObject({ length: 11, empty: false });
    expect(JSON.stringify(buffer.summary())).not.toContain("hello");
  });

  it("returns a bounded tail window for rendering", () => {
    const buffer = new AssistantStreamBuffer();
    buffer.append("A".repeat(100_000));
    buffer.append("TAIL");

    const window = buffer.renderWindow(10_000);
    expect(window.content.length).toBeLessThanOrEqual(10_000);
    expect(window.content.endsWith("TAIL")).toBe(true);
    expect(window.truncated).toBe(true);
    expect(window.fullLength).toBe(100_004);
  });
});
