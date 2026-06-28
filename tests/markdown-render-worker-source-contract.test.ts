import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("markdown render worker source contract", () => {
  it("worker delegates rendering to the shared markdown contract core", () => {
    const worker = read("src/workers/markdown-render.worker.ts");

    expect(worker).toContain("renderMarkdownForWorker");
    expect(worker).not.toContain("marked.parse");
    expect(worker).not.toContain("replace(/<script");
  });

  it("worker skips duplicate content and honors abort messages", () => {
    const worker = read("src/workers/markdown-render.worker.ts");

    expect(worker).toContain("lastRenderedHash");
    expect(worker).toContain('type === "abort"');
    expect(worker).toContain('reason: "duplicate"');
    expect(worker).toContain('reason: "aborted"');
  });
});
