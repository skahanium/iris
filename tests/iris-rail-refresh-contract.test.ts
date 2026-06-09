import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Iris Rail complete interface contracts", () => {
  it("defines semantic tokens for the full Iris Rail interface system", () => {
    const css = read("src/styles/globals.css");
    expect(css).toContain("--knowledge-accent");
    expect(css).toContain("--iris-rail-bg");
    expect(css).toContain("--iris-rail-active");
    expect(css).toContain("--outline-rail-bg");
    expect(css).toContain("--outline-rail-active");
    expect(css).toContain("--ai-workspace-bg");
    expect(css).toContain("--ai-workspace-border");
    expect(css).toContain("--overlay-task-header");
  });

  it("documents the complete Iris Rail target surfaces", () => {
    const design = read("docs/design-system.md");
    expect(design).toContain("Iris Rail 完整刷新设计");
    expect(design).toContain("Rail Segments Tab");
    expect(design).toContain("Outline Rail");
    expect(design).toContain("AI Conversation Workspace");
    expect(design).toContain("Overlay Family");
  });
});
