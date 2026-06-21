import type { Editor } from "@tiptap/react";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

export class IrisClipboardError extends Error {
  constructor(message: string) {
    super(message);
    this.name = "IrisClipboardError";
  }
}

/** 读取系统剪贴板纯文本 */
export async function readClipboardText(): Promise<string> {
  try {
    return await navigator.clipboard.readText();
  } catch {
    throw new IrisClipboardError("clipboard_unavailable");
  }
}

/** 写入系统剪贴板纯文本 */
export async function writeClipboardText(text: string): Promise<void> {
  try {
    await navigator.clipboard.writeText(text);
  } catch {
    throw new IrisClipboardError("clipboard_unavailable");
  }
}

export function getEditorSelectedText(editor: Editor): string {
  const { from, to } = editor.state.selection;
  if (from === to) return "";
  return editor.state.doc.textBetween(from, to, "\n");
}

export interface EditorSelectionSnapshot {
  text: string;
  content: string;
  editorRange: { from: number; to: number };
}

export function getEditorSelectionSnapshot(
  editor: Editor,
): EditorSelectionSnapshot | null {
  const { from, to } = editor.state.selection;
  if (from === to) return null;
  const text = editor.state.doc.textBetween(from, to, "\n");
  if (!text) return null;
  return {
    text,
    content: editor.state.doc.textBetween(
      0,
      editor.state.doc.content.size,
      "\n",
    ),
    editorRange: { from, to },
  };
}

/** 复制 TipTap 选区 */
export async function copyEditorSelection(editor: Editor): Promise<boolean> {
  const text = getEditorSelectedText(editor);
  if (!text) return false;
  await writeClipboardText(text);
  return true;
}

/** 剪切 TipTap 选区 */
export async function cutEditorSelection(editor: Editor): Promise<boolean> {
  const { from, to } = editor.state.selection;
  if (from === to) return false;
  const text = editor.state.doc.textBetween(from, to, "\n");
  await writeClipboardText(text);
  editor.chain().focus().deleteRange({ from, to }).run();
  return true;
}

function shouldPasteAsEditorMarkdown(text: string): boolean {
  const trimmed = text.trim();
  if (!trimmed) return false;

  return (
    /(^|\n)[ \t]{0,3}(#{1,6}\s|[-*+]\s+|\d+[.)]\s+|>\s+)/u.test(trimmed) ||
    /(^|\n)[ \t]{0,3}(```|~~~)/u.test(trimmed) ||
    /(^|[^\\])(\*\*|__)\S/u.test(trimmed) ||
    /!\[[^\]]*\]\([^)]+\)|\[[^\]]+\]\([^)]+\)|\[\[[^\]]+\]\]/u.test(trimmed) ||
    /(^|\n)[ \t]*\|.+\|[ \t]*(\n|$)/u.test(trimmed)
  );
}

/** 粘贴到 TipTap 光标/选区 */
export async function pasteIntoEditor(editor: Editor): Promise<boolean> {
  const text = await readClipboardText();
  if (!text) return false;

  if (shouldPasteAsEditorMarkdown(text)) {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: text.trim(),
    });
    if (!tipTapHtml.trim()) return false;
    editor.chain().focus().insertContent(tipTapHtml).run();
    return true;
  }

  editor.chain().focus().insertContent(text).run();
  return true;
}

export interface TextFieldSelection {
  start: number;
  end: number;
}

/** 复制 HTML input/textarea 选区 */
export async function copyTextFieldSelection(
  value: string,
  selection: TextFieldSelection,
): Promise<boolean> {
  const { start, end } = selection;
  if (start === end) return false;
  await writeClipboardText(value.slice(start, end));
  return true;
}

/** 剪切 HTML input/textarea 选区；返回新全文与光标位置 */
export async function cutTextFieldSelection(
  value: string,
  selection: TextFieldSelection,
): Promise<{ value: string; caret: number } | null> {
  const { start, end } = selection;
  if (start === end) return null;
  await writeClipboardText(value.slice(start, end));
  return { value: value.slice(0, start) + value.slice(end), caret: start };
}

/** 粘贴到 input/textarea；返回新全文与光标位置 */
export async function pasteIntoTextField(
  value: string,
  selection: TextFieldSelection,
): Promise<{ value: string; caret: number } | null> {
  const text = await readClipboardText();
  if (!text) return null;
  const { start, end } = selection;
  const caret = start + text.length;
  return {
    value: value.slice(0, start) + text + value.slice(end),
    caret,
  };
}

export function applyTextFieldCaret(
  el: HTMLInputElement | HTMLTextAreaElement,
  caret: number,
): void {
  requestAnimationFrame(() => {
    el.focus();
    el.selectionStart = caret;
    el.selectionEnd = caret;
  });
}
