import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  clearMarkdownRenderCache,
  getMarkdownRenderCacheStats,
  renderMarkdownWithProfile,
} from "@/lib/markdown-contract/contract";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("markdown render cache memory budget", () => {
  it("does not include raw source in cache keys", () => {
    const source = read("src/lib/markdown-contract/contract.ts");

    expect(source).not.toContain("${source}");
    expect(source).toContain("markdownContentHash");
  });

  it("does not cache oversized rendered entries", () => {
    clearMarkdownRenderCache();
    const huge = `# Huge\n\n${"A".repeat(420_000)}`;

    renderMarkdownWithProfile(huge, "chat_assistant", { streaming: false });

    const stats = getMarkdownRenderCacheStats();
    expect(stats.entryCount).toBe(0);
    expect(stats.estimatedBytes).toBe(0);
  });
});
