import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant stream rendering performance contract", () => {
  it("keeps streaming markdown render throttling explicit and responsive", () => {
    const src = read("src/hooks/useStreamingContent.ts");

    expect(src).toContain("const MIN_FLUSH_INTERVAL_MS = 80");
    expect(src).toContain("const STREAMING_SHORT_CONTENT_LIMIT = 200");
    expect(src).toContain("const STREAMING_BIG_JUMP_CHARS = 240");
    expect(src).toContain("paragraphBreak");
    expect(src).toContain("return cacheRef.current.rendered");
  });

  it("documents that the hook protects markdown parsing frequency", () => {
    const src = read("src/hooks/useStreamingContent.ts");

    expect(src).toContain("Markdown");
    expect(src).toContain("重解析");
    expect(src).toContain("80ms");
  });

  it("contains streaming assistant bubble layout and paint work", () => {
    const css = read("src/styles/globals.css");
    const after =
      css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";
    const streamingRule = after.split("}")[0] ?? "";

    expect(streamingRule).toContain("contain: layout paint style");
    expect(streamingRule).not.toContain("content-visibility: auto");
  });

  it("allows content-visibility only for finalized assistant bubbles", () => {
    const css = read("src/styles/globals.css");
    const afterFinalized =
      css.split(".ai-message-bubble-assistant:not([data-streaming])")[1] ?? "";
    const finalizedRule = afterFinalized.split("}")[0] ?? "";
    const afterStreaming =
      css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";
    const streamingRule = afterStreaming.split("}")[0] ?? "";

    expect(finalizedRule).toContain("content-visibility: auto");
    expect(finalizedRule).toContain("contain-intrinsic-size");
    expect(streamingRule).not.toContain("content-visibility: auto");
  });

  it("assistant bubbles expose stable data attributes for finalized and streaming states", () => {
    const src = read("src/components/ai/AiMessageBubble.tsx");

    expect(src).toContain("data-role={role}");
    expect(src).toContain('data-streaming={streaming ? "" : undefined}');
  });
});
