import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  clearCachedEditorHtml,
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";

describe("editor-html-cache", () => {
  it("stores and retrieves html per path", () => {
    setCachedEditorHtml("a.md", "<p>x</p>", "digest-a");
    expect(getCachedEditorHtml("a.md", "digest-a")).toBe("<p>x</p>");
    clearCachedEditorHtml("a.md");
    expect(getCachedEditorHtml("a.md", "digest-a")).toBeUndefined();
  });

  it("misses and evicts stale html when digest differs", () => {
    setCachedEditorHtml("a.md", "<p>old</p>", "old");

    expect(getCachedEditorHtml("a.md", "new")).toBeUndefined();
    expect(getCachedEditorHtml("a.md", "old")).toBeUndefined();
  });

  it("keeps FIFO eviction behavior with digest entries", () => {
    for (let i = 0; i < 30; i++) {
      setCachedEditorHtml(`${i}.md`, `<p>${i}</p>`, `digest-${i}`);
    }
    setCachedEditorHtml("30.md", "<p>30</p>", "digest-30");

    expect(getCachedEditorHtml("0.md", "digest-0")).toBeUndefined();
    expect(getCachedEditorHtml("30.md", "digest-30")).toBe("<p>30</p>");
  });

  it("TipTapEditor passes digest to all editor HTML cache reads and writes", () => {
    const source = readSource("src/components/editor/TipTapEditor.tsx");

    expect(source).toContain("editorHtmlDigest(initialBodyMarkdown)");
    expect(source).toMatch(/getCachedEditorHtml\([^,\n]+,\s*htmlDigest\)/);
    expect(source).toMatch(
      /setCachedEditorHtml\([^,\n]+,[^,\n]+,\s*htmlDigest\)/,
    );
  });
});

function readSource(path: string): string {
  return readFileSync(path, "utf8");
}
