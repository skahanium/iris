import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { editorHtmlHasVisibleFailedBold } from "@/lib/editor-html-cache";
import {
  editorBodyHtmlToMarkdown,
  markdownBodyToEditorHtml,
  markdownToHtml,
} from "@/lib/markdown";

const escapedStrongListItem =
  "- \\*\\*Physical layer:\\*\\* keeps the shared KV pool stable.";

describe("escaped strong delimiters in MiMo note", () => {
  it("documents exporter mistake pattern", () => {
    expect(escapedStrongListItem).toContain("\\*\\*Physical layer:");
  });

  it("markdownToHtml repairs backslash-escaped strong delimiters", () => {
    const html = markdownToHtml(escapedStrongListItem);

    expect(html).toContain("<strong>Physical layer:</strong>");
    expect(html).not.toContain("**Physical layer:**");
  });

  it("turndown does not re-escape list item bold", () => {
    const md = "- **Physical layer:** keeps the shared KV pool stable.";
    const html = markdownBodyToEditorHtml(md);
    const back = editorBodyHtmlToMarkdown(html);

    expect(back).not.toMatch(/\\\*\\\*/);
    expect(back).toContain("**Physical layer:**");
  });

  it("repairs backslash-escaped strong delimiters on ingest", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: escapedStrongListItem,
    });

    expect(tipTapHtml).toContain("<strong>Physical layer:</strong>");
    expect(editorHtmlHasVisibleFailedBold(tipTapHtml)).toBe(false);
  });

  it("ingests MiMo note without visible failed bold", () => {
    const body = [
      "---",
      "title: MiMo escaped strong fixture",
      "---",
      "",
      escapedStrongListItem,
      "- \\*\\*Routing layer:\\*\\* keeps the visible title clean.",
    ].join("\n");
    const bodyMd = body.replace(/^---[\s\S]*?---\n?/, "").trim();
    const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: bodyMd });

    expect(editorHtmlHasVisibleFailedBold(tipTapHtml)).toBe(false);
  });
});
