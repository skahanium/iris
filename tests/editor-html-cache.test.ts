import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

import {
  clearCachedEditorHtml,
  editorHtmlDigest,
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";

function legacyEditorHtmlDigest(markdown: string): string {
  let hash = 0x811c9dc5;
  for (let i = 0; i < markdown.length; i++) {
    hash ^= markdown.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16);
}

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

  it("rejects cached html with visible failed colon-bold markdown", () => {
    const markdown = "1. **DP-Attention 同步：**多 DP 段的计算拖慢。";
    const digest = editorHtmlDigest(markdown);

    setCachedEditorHtml(
      "bad.md",
      "<ol><li>**DP-Attention 同步：**多 DP 段的计算拖慢。</li></ol>",
      digest,
    );

    expect(getCachedEditorHtml("bad.md", digest)).toBeUndefined();
  });

  it("rejects cached html that contains visible unparsed markdown block markers", () => {
    const markdown = "# 反映的问题\n\n1. 大师的\n\n+ 打算打";
    const digest = editorHtmlDigest(markdown);

    setCachedEditorHtml(
      "raw-markdown.md",
      "<p># 反映的问题</p><p>1. 大师的</p><p>+ 打算打</p>",
      digest,
    );

    expect(getCachedEditorHtml("raw-markdown.md", digest)).toBeUndefined();
  });

  it("stores healthy cached html with strong tags", () => {
    const markdown = "1. **DP-Attention 同步：**多 DP 段的计算拖慢。";
    const digest = editorHtmlDigest(markdown);
    const html =
      "<ol><li><strong>DP-Attention 同步：</strong>多 DP 段的计算拖慢。</li></ol>";

    setCachedEditorHtml("good.md", html, digest);
    expect(getCachedEditorHtml("good.md", digest)).toBe(html);
  });

  it("busts stale cached HTML when editor ingest semantics change", () => {
    const markdown = "1. **DP-Attention 同步：**多 DP 段的计算拖慢。";
    const staleDigest = legacyEditorHtmlDigest(markdown);
    const currentDigest = editorHtmlDigest(markdown);

    expect(currentDigest).not.toBe(staleDigest);

    setCachedEditorHtml(
      "bold-label.md",
      "<ol><li>**DP-Attention 同步：**多 DP 段的计算拖慢。</li></ol>",
      staleDigest,
    );

    expect(getCachedEditorHtml("bold-label.md", currentDigest)).toBeUndefined();
  });

  it("TipTapEditor passes digest to all editor HTML cache reads and writes", () => {
    const source = readSource("src/components/editor/TipTapEditor.tsx");

    expect(source).toContain("editorHtmlDigest(initialBodyMarkdown)");
    expect(source).toMatch(/getCachedEditorHtml\([^,\n]+,\s*htmlDigest\)/);
    expect(source).toMatch(
      /setCachedEditorHtml\([^,\n]+,[^,\n]+,\s*htmlDigest\)/,
    );
  });

  it("AppEditorWorkspace remounts TipTapEditor when ingest cache format changes", () => {
    const source = readSource("src/components/layout/AppEditorWorkspace.tsx");

    expect(source).toContain("EDITOR_HTML_CACHE_FORMAT_VERSION");
    expect(source).toContain(
      "key={`${activePath}:${EDITOR_HTML_CACHE_FORMAT_VERSION}`}",
    );
  });
});

function readSource(path: string): string {
  return readFileSync(path, "utf8");
}
