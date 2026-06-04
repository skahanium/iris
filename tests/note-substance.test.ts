import { describe, expect, it } from "vitest";

import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";

describe("isNoteSubstantivelyEmpty", () => {
  it("treats default new-note template as empty", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: ""\n---\n\n')).toBe(true);
  });

  it("treats frontmatter 无标题1 with empty body as empty", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: "无标题1"\n---\n\n')).toBe(
      true,
    );
  });

  it("treats legacy frontmatter 无标题 with empty body as empty", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: "无标题"\n---\n\n')).toBe(
      true,
    );
  });

  it("treats numbered 无标题2 placeholders as empty", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: "无标题2"\n---\n\n')).toBe(
      true,
    );
  });

  it("treats 新建文档 with empty body as empty", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: "新建文档"\n---\n\n')).toBe(
      true,
    );
  });

  it("treats numbered 新建文档（1） placeholders as empty", () => {
    expect(
      isNoteSubstantivelyEmpty('---\ntitle: "新建文档（1）"\n---\n\n'),
    ).toBe(true);
  });

  it("is not empty when title is set", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: "我的笔记"\n---\n\n')).toBe(
      false,
    );
  });

  it("is not empty when body has text", () => {
    expect(isNoteSubstantivelyEmpty('---\ntitle: ""\n---\n\nHello')).toBe(
      false,
    );
  });

  it("is not empty for legacy body-only note with content", () => {
    expect(isNoteSubstantivelyEmpty("# Title\n\nParagraph.")).toBe(false);
  });

  it("treats legacy default heading-only new document as empty", () => {
    expect(isNoteSubstantivelyEmpty("# 新建文档\n\n")).toBe(true);
    expect(isNoteSubstantivelyEmpty("# 新建文档（1）\n\n")).toBe(true);
    expect(isNoteSubstantivelyEmpty("# 未命名文档\n\n")).toBe(true);
    expect(isNoteSubstantivelyEmpty("# 未命名文档（1）\n\n")).toBe(true);
  });
});
