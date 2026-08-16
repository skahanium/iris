import { existsSync, readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant stream rendering performance contract", () => {
  it("sends each committed streaming delta directly to the isolated tail", () => {
    const src = read("src/components/ai/AiMessageBubble.tsx");

    expect(src).toContain("content={streamingContent}");
    expect(src).toContain("createStreamingRenderableContent(content)");
    expect(src).not.toContain("useStreamingContent");
  });

  it("removes the obsolete streaming content throttle module", () => {
    expect(existsSync("src/hooks/useStreamingContent.ts")).toBe(false);
  });

  it("contains streaming assistant bubble layout and paint work", () => {
    const css = read("src/styles/globals.css");
    const after =
      css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";
    const streamingRule = after.split("}")[0] ?? "";

    expect(streamingRule).toContain("contain: layout style");
    expect(streamingRule).not.toContain("contain: paint");
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
