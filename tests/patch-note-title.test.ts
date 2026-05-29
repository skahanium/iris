import { describe, expect, it } from "vitest";

import { patchNoteTitleInMarkdown } from "@/lib/patch-note-title";
import { displayTitleFromMarkdown } from "@/lib/note-title";
import { noteMarkdownRoundTrip } from "@/lib/markdown";

describe("patchNoteTitleInMarkdown", () => {
  it("updates frontmatter title without changing body", () => {
    const md = "---\ntitle: 旧标题\n---\n\n# 章节\n\n正文";
    const next = patchNoteTitleInMarkdown(md, "新标题");
    expect(displayTitleFromMarkdown(next)).toBe("新标题");
    expect(next).toContain("# 章节");
    expect(next).toContain("正文");
    expect(next).not.toContain("title: 旧标题");
  });

  it("round-trip keeps patched title in noteTitle block", () => {
    const md = "---\ntitle: Alpha\n---\n\n正文";
    const patched = patchNoteTitleInMarkdown(md, "Beta");
    const round = noteMarkdownRoundTrip(patched, "fallback");
    expect(displayTitleFromMarkdown(round)).toBe("Beta");
  });
});
