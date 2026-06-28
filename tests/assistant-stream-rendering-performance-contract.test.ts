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
    const streamingRule =
      css.split(".ai-message-bubble-streaming[data-streaming]")[1] ?? "";

    expect(streamingRule).toContain("contain: layout paint style");
    expect(streamingRule).not.toContain("content-visibility: auto");
  });
});
