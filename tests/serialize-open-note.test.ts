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

  it("serializes appended tail paragraphs from the live editor", () => {
    const bodyHtml = markdownBodyToEditorHtml("Alpha\n\nBeta");
    editor = bodyEditor(bodyHtml);
    editor.commands.insertContentAt(editor.state.doc.content.size, {
      type: "paragraph",
      content: [{ type: "text", text: "SAVE_MARKER" }],
    });
    const md = serializeOpenNote({
      yaml: null,
      title: "t",
      editor,
      bodyFallbackMd: "Alpha\n\nBeta",
    });
    expect(md).toContain("SAVE_MARKER");
  });

  it("serializes edited middle paragraphs from the live editor", () => {
    const bodyHtml = markdownBodyToEditorHtml("Alpha\n\nBeta\n\nGamma");
    editor = bodyEditor(bodyHtml);
    editor.commands.setContent(
      markdownBodyToEditorHtml("Alpha\n\nEdited Beta\n\nGamma"),
      false,
    );
    const md = serializeOpenNote({
      yaml: null,
      title: "t",
      editor,
      bodyFallbackMd: "Alpha\n\nBeta\n\nGamma",
    });
    expect(md).toContain("Edited Beta");
    expect(md).not.toContain("\n\nBeta\n\n");
  });

  it("serializes deleted paragraphs from the live editor", () => {
    const bodyHtml = markdownBodyToEditorHtml("Alpha\n\nBeta\n\nGamma");
    editor = bodyEditor(bodyHtml);
    editor.commands.setContent(
      markdownBodyToEditorHtml("Alpha\n\nGamma"),
      false,
    );
    const md = serializeOpenNote({
      yaml: null,
      title: "t",
      editor,
      bodyFallbackMd: "Alpha\n\nBeta\n\nGamma",
    });
    expect(md).toContain("Alpha");
    expect(md).toContain("Gamma");
    expect(md).not.toContain("Beta");
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

  it("uses bodyFallback when the editor exists but is not ready for persistence", () => {
    editor = bodyEditor("<p></p>");
    const md = serializeOpenNote({
      yaml: null,
      title: "仅标题",
      editor,
      editorReady: false,
      bodyFallbackMd: "已加载但尚未灌入编辑器的正文",
    });

    expect(displayTitleFromMarkdown(md)).toBe("仅标题");
    expect(md).toContain("已加载但尚未灌入编辑器的正文");
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

  it("keeps no-frontmatter leading h1 as an editable body heading", () => {
    const parsed = parseNoteForEditor(
      "# Classified Section\n\nBody",
      "fallback",
    );
    expect(parsed.title).toBe("fallback");
    expect(parsed.bodyMd).toContain("# Classified Section");
    expect(parsed.bodyMd).toContain("Body");
  });
});
