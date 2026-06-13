import { describe, expect, it } from "vitest";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { editorHtmlHasVisibleFailedBold } from "@/lib/editor-html-cache";
import { editorBodyHtmlToMarkdown, markdownBodyToEditorHtml, markdownToHtml } from "@/lib/markdown";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";

const mimoPath = resolve(
  process.env.HOME ?? "",
  "Iris-test/MiMo-V2.5 系列推理全链路优化：将 Hybrid SWA 效率推向极致.md",
);

describe("escaped strong delimiters in MiMo note", () => {
  it("documents exporter mistake pattern", () => {
    const escaped = "- \\*\\*物理层面：\\*\\*分别维护";
    expect(escaped).toContain("\\*\\*物理层面");
  });

  it("markdownToHtml repairs backslash-escaped strong delimiters", () => {
    const md = "- \\*\\*物理层面：\\*\\*分别维护 Full KV pool";
    const html = markdownToHtml(md);
    expect(html).toContain("<strong>物理层面：</strong>");
    expect(html).not.toContain("**物理层面：**");
  });

  it("turndown does not re-escape list item bold", () => {
    const md = "- **物理层面：**分别维护 Full KV pool";
    const html = markdownBodyToEditorHtml(md);
    const back = editorBodyHtmlToMarkdown(html);
    expect(back).not.toMatch(/\\\*\\\*/);
    expect(back).toContain("**物理层面：**");
  });

  it("repairs backslash-escaped strong delimiters on ingest", () => {
    const md = "- \\*\\*物理层面：\\*\\*分别维护 Full KV pool";
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(tipTapHtml).toContain("<strong>物理层面：</strong>");
    expect(editorHtmlHasVisibleFailedBold(tipTapHtml)).toBe(false);
  });

  it("ingests MiMo note without visible failed bold", () => {
    const body = readFileSync(mimoPath, "utf8");
    const bodyMd = body.replace(/^---[\s\S]*?---\n?/, "").trim();
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: bodyMd });
    expect(editorHtmlHasVisibleFailedBold(tipTapHtml)).toBe(false);
  });
});
