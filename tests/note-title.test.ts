import { describe, expect, it } from "vitest";

import { displayTitleFromMarkdown } from "@/lib/note-title";
import { markdownBodyToEditorHtml, parseNoteForEditor } from "@/lib/markdown";

describe("displayTitleFromMarkdown", () => {
  it("does not treat legacy frontmatter title as a document title", () => {
    const md = "---\ntitle: 吃早饭\n---\n\n# 一级标题\n\n正文";
    expect(displayTitleFromMarkdown(md)).toBe("无标题");
  });

  it("does not treat body h1 as document title", () => {
    const md = '---\ntitle: ""\n---\n\n# 一级标题\n\n正文';
    expect(displayTitleFromMarkdown(md, "无标题")).toBe("无标题");
  });

  it("returns fallback when frontmatter title is empty", () => {
    const md = '---\ntitle: ""\n---\n\n';
    expect(displayTitleFromMarkdown(md, "无标题")).toBe("无标题");
  });
});

describe("parseNoteForEditor + markdownBodyToEditorHtml", () => {
  it("keeps no-frontmatter body h1 as a normal section heading", () => {
    const md = "# Classified Section\n\nBody";
    const { title, bodyMd } = parseNoteForEditor(md, "secret");
    const html = markdownBodyToEditorHtml(bodyMd);
    expect(title).toBe("secret");
    expect(html).toContain("<h1>Classified Section</h1>");
    expect(html).not.toContain("iris-doc-title");
  });

  it("keeps body h1 as section heading when frontmatter title is empty", () => {
    const md = '---\ntitle: ""\n---\n\n# 一级标题\n\n正文';
    const { bodyMd } = parseNoteForEditor(md);
    const html = markdownBodyToEditorHtml(bodyMd);
    expect(html).toContain("<h1>一级标题</h1>");
    expect(html).not.toContain("iris-doc-title");
  });
});
