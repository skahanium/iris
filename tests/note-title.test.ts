import { describe, expect, it } from "vitest";

import { displayTitleFromMarkdown } from "@/lib/note-title";
import { markdownToEditorHtml } from "@/lib/markdown";

describe("displayTitleFromMarkdown", () => {
  it("reads title from frontmatter only", () => {
    const md = '---\ntitle: 吃早饭\n---\n\n# 一级标题\n\n正文';
    expect(displayTitleFromMarkdown(md)).toBe("吃早饭");
  });

  it("does not treat body h1 as document title", () => {
    const md = '---\ntitle: ""\n---\n\n# 一级标题\n\n正文';
    expect(displayTitleFromMarkdown(md, "无标题")).toBe("无标题");
  });

  it("returns fallback when frontmatter title is empty", () => {
    const md = "---\ntitle: \"\"\n---\n\n";
    expect(displayTitleFromMarkdown(md, "无标题")).toBe("无标题");
  });
});

describe("markdownToEditorHtml (noteTitle vs section h1)", () => {
  it("keeps body h1 as section heading when frontmatter title is empty", () => {
    const md = '---\ntitle: ""\n---\n\n# 一级标题\n\n正文';
    const html = markdownToEditorHtml(md);
    expect(html).toMatch(/<h1 class="iris-doc-title"><\/h1>/);
    expect(html).toContain("<h1>一级标题</h1>");
    expect(html).not.toMatch(/iris-doc-title[^>]*>一级标题/);
  });
});
