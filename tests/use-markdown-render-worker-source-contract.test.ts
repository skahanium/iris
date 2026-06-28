import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("useMarkdownRenderWorker source contract", () => {
  it("owns only worker lifecycle and async render results", () => {
    const src = read("src/hooks/useMarkdownRenderWorker.ts");

    expect(src).toContain("new Worker");
    expect(src).toContain("markdown-render.worker.ts");
    expect(src).toContain("postMessage");
    expect(src).toContain("terminate");
    expect(src).not.toContain("useStreamingContent");
  });

  it("keeps previous html while worker render is pending", () => {
    const src = read("src/hooks/useMarkdownRenderWorker.ts");

    expect(src).toContain("lastHtmlRef");
    expect(src).toContain("setState");
    expect(src).toContain("pending: true");
  });
});
