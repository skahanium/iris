import type { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it, vi } from "vitest";

import {
  EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
  createEditorContextReference,
  installEditorMarkdownSourceProjection,
} from "@/lib/context-reference";
import { parseNoteForEditor } from "@/lib/markdown";

import { createProductionEditorFromNote } from "./helpers/tiptap-serialize-harness";

async function sha256(content: string): Promise<string> {
  const digest = await crypto.subtle.digest(
    "SHA-256",
    new TextEncoder().encode(content),
  );
  return Array.from(new Uint8Array(digest), (byte) =>
    byte.toString(16).padStart(2, "0"),
  ).join("");
}

async function signatureFor(content: string) {
  return {
    byteLength: new TextEncoder().encode(content).length,
    contentHash: await sha256(content),
    isLocked: false,
    modifiedMs: 1,
  };
}

function textPosition(editor: Editor, needle: string): number {
  let position = -1;
  editor.state.doc.descendants((node, pos) => {
    if (position >= 0 || !node.isText) return;
    const index = (node.text ?? "").indexOf(needle);
    if (index >= 0) position = pos + index;
  });
  if (position < 0) throw new Error(`missing editor text: ${needle}`);
  return position;
}

function install(editor: Editor, markdown: string, path = "notes/复杂.md") {
  installEditorMarkdownSourceProjection(editor, {
    filePath: path,
    committedMarkdown: markdown,
    bodyMarkdown: parseNoteForEditor(markdown, "复杂").bodyMd,
  });
}

describe("editor Markdown source projection", () => {
  const editors: Editor[] = [];

  afterEach(() => {
    editors.splice(0).forEach((editor) => editor.destroy());
  });

  it("maps a complex Markdown selection to the same committed UTF-8 byte range", async () => {
    const markdown = [
      "---",
      'title: "复杂"',
      "---",
      "## 复杂标题",
      "",
      "第一段含 **加粗中文**、[链接文本](https://example.com)。",
      "",
      "- 列表甲",
      "- 列表乙与 `code`",
      "",
      "尾段 😀。",
    ].join("\n");
    const editor = createProductionEditorFromNote(markdown);
    editors.push(editor);
    install(editor, markdown);
    const from = textPosition(editor, "中文");
    const to = textPosition(editor, "列表乙") + "列表乙".length;
    editor.commands.setTextSelection({ from, to });

    const result = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const expectedStart = new TextEncoder().encode(
      markdown.slice(0, markdown.indexOf("中文")),
    ).length;
    const expectedEnd = new TextEncoder().encode(
      markdown.slice(0, markdown.indexOf("列表乙") + "列表乙".length),
    ).length;
    expect(result.reference).toMatchObject({
      kind: "selection",
      filePath: "notes/复杂.md",
      contentHash: await sha256(markdown),
      utf8Range: { start: expectedStart, end: expectedEnd },
      editorRange: { from, to },
      stale: false,
      invalidReason: null,
    });
    expect(result.reference.excerpt).toBe("");
    expect(JSON.stringify(result.reference)).not.toContain("中文");
  });

  it("refuses a dirty editor without reading or returning its current body", async () => {
    const markdown = "---\ntitle: 安全\n---\n已提交正文";
    const editor = createProductionEditorFromNote(markdown);
    editors.push(editor);
    install(editor, markdown, "notes/safe.md");
    editor.commands.setTextSelection({ from: 1, to: 4 });
    editor.commands.insertContent("未保存秘密");
    editor.commands.setTextSelection({ from: 1, to: 4 });
    const getFileSignature = vi.fn(() => signatureFor(markdown));

    const result = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature,
    });

    expect(result).toEqual({
      ok: false,
      reason: "dirty",
      message: EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
    });
    expect(getFileSignature).not.toHaveBeenCalled();
    expect(JSON.stringify(result)).not.toContain("未保存秘密");
  });

  it("refuses a selection crossing an explicitly unmappable fragment", async () => {
    const markdown = '开头文本\n\n<div data-x="1">原始块</div>\n\n结尾文本';
    const editor = createProductionEditorFromNote(markdown);
    editors.push(editor);
    install(editor, markdown, "notes/raw.md");
    const from = textPosition(editor, "开头");
    const to = textPosition(editor, "结尾") + "结尾".length;
    editor.commands.setTextSelection({ from, to });

    const result = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });

    expect(result).toEqual({
      ok: false,
      reason: "unmappable",
      message: EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
    });
  });

  it("refuses a stale backend signature and an invalid file path", async () => {
    const markdown = "已提交正文";
    const editor = createProductionEditorFromNote(markdown);
    editors.push(editor);
    install(editor, markdown, "notes/a.md");
    editor.commands.setTextSelection({ from: 1, to: 4 });

    const stale = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature: vi.fn(async () => ({
        ...(await signatureFor(markdown)),
        contentHash: await sha256("磁盘已变化"),
      })),
    });
    install(editor, markdown, "   ");
    const invalidPath = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });

    expect(stale).toEqual({
      ok: false,
      reason: "source_changed",
      message: EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
    });
    expect(invalidPath).toEqual({
      ok: false,
      reason: "invalid_projection",
      message: EDITOR_REFERENCE_SAVE_REQUIRED_MESSAGE,
    });
  });

  it("does not confuse code-fence info or link destinations with visible text", async () => {
    const markdown = [
      "```same",
      "same",
      "```",
      "",
      "[label](target)",
      "",
      "target",
    ].join("\n");
    const editor = createProductionEditorFromNote(markdown);
    editors.push(editor);
    install(editor, markdown, "notes/ambiguous.md");
    const codeFrom = textPosition(editor, "same");
    editor.commands.setTextSelection({
      from: codeFrom,
      to: codeFrom + "same".length,
    });
    const codeReference = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });
    const paragraphFrom = textPosition(editor, "target");
    editor.commands.setTextSelection({
      from: paragraphFrom,
      to: paragraphFrom + "target".length,
    });
    const paragraphReference = await createEditorContextReference({
      editor,
      kind: "selection",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });

    expect(codeReference.ok && codeReference.reference.utf8Range).toEqual({
      start: markdown.indexOf("\nsame\n") + 1,
      end: markdown.indexOf("\nsame\n") + 5,
    });
    expect(
      paragraphReference.ok && paragraphReference.reference.utf8Range,
    ).toEqual({
      start: markdown.lastIndexOf("target"),
      end: markdown.length,
    });
  });

  it("creates a paragraph reference from a cursor without widening to adjacent blocks", async () => {
    const markdown = "前段。\n\n段落含 **加粗中文** 与 emoji 😀。\n\n后段。";
    const editor = createProductionEditorFromNote(markdown);
    editors.push(editor);
    install(editor, markdown, "notes/paragraph.md");
    const cursor = textPosition(editor, "加粗中文") + 2;
    editor.commands.setTextSelection(cursor);

    const result = await createEditorContextReference({
      editor,
      kind: "paragraph",
      getFileSignature: vi.fn(() => signatureFor(markdown)),
    });

    expect(result.ok).toBe(true);
    if (!result.ok) return;
    const paragraphStart = markdown.indexOf("段落含");
    const paragraphEnd = markdown.indexOf("。\n\n后段") + 1;
    expect(result.reference.kind).toBe("paragraph");
    expect(result.reference.utf8Range).toEqual({
      start: new TextEncoder().encode(markdown.slice(0, paragraphStart)).length,
      end: new TextEncoder().encode(markdown.slice(0, paragraphEnd)).length,
    });
  });
});
