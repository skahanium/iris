import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it } from "vitest";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { displayTitleFromMarkdown } from "@/lib/note-title";
import { markdownBodyToEditorHtml, parseNoteForEditor } from "@/lib/markdown";
import { serializeOpenNote } from "@/lib/serialize-open-note";

function bodyEditor(html: string): Editor {
  return new Editor({
    extensions: [
      IrisDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: { levels: [1, 2, 3, 4, 5, 6] },
      }),
    ],
    content: html,
  });
}

describe("serializeOpenNote", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("combines title state with editor body", () => {
    const bodyHtml = markdownBodyToEditorHtml("正文段落。");
    editor = bodyEditor(bodyHtml);
    const md = serializeOpenNote({
      yaml: 'title: "旧"\ntags: [x]',
      title: "新标题",
      editor,
      bodyFallbackMd: "",
    });
    expect(displayTitleFromMarkdown(md)).toBe("新标题");
    expect(md).toContain("正文段落");
    expect(md).toContain("tags: [x]");
  });

  it("uses bodyFallback when editor is null", () => {
    const md = serializeOpenNote({
      yaml: null,
      title: "仅标题",
      editor: null,
      bodyFallbackMd: "后备正文",
    });
    expect(displayTitleFromMarkdown(md)).toBe("仅标题");
    expect(md).toContain("后备正文");
  });

  it("preserves special characters in title", () => {
    const md = serializeOpenNote({
      yaml: null,
      title: '引号"与\\反斜杠',
      editor: null,
      bodyFallbackMd: "",
    });
    expect(displayTitleFromMarkdown(md)).toBe('引号"与\\反斜杠');
  });
});

describe("parseNoteForEditor", () => {
  it("extracts title and body separately from frontmatter note", () => {
    const md = "---\ntitle: 吃早饭\n---\n\n# 一级\n\n正文";
    const parsed = parseNoteForEditor(md);
    expect(parsed.title).toBe("吃早饭");
    expect(parsed.bodyMd).toContain("# 一级");
    expect(parsed.bodyMd).toContain("正文");
    expect(parsed.bodyMd).not.toMatch(/^#\s+吃早饭/);
  });

  it("migrates legacy leading h1 into title", () => {
    const parsed = parseNoteForEditor("# Legacy\n\nBody", "fallback");
    expect(parsed.title).toBe("Legacy");
    expect(parsed.bodyMd).toContain("Body");
    expect(parsed.bodyMd).not.toMatch(/^#\s+Legacy/);
  });
});
