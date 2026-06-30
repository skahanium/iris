import { describe, expect, it, vi, beforeEach } from "vitest";
import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

import {
  IrisClipboardError,
  readClipboardText,
  writeClipboardText,
  copyTextFieldSelection,
  cutTextFieldSelection,
  normalizePastedEditorHtml,
  pasteIntoEditor,
  pasteIntoTextField,
} from "@/lib/iris-clipboard";

describe("iris-clipboard", () => {
  beforeEach(() => {
    vi.stubGlobal("navigator", {
      userAgent: "vitest",
      clipboard: {
        readText: vi.fn(async () => "pasted"),
        writeText: vi.fn(async () => undefined),
      },
    });
  });

  it("readClipboardText returns clipboard content", async () => {
    await expect(readClipboardText()).resolves.toBe("pasted");
  });

  it("writeClipboardText throws IrisClipboardError on failure", async () => {
    vi.mocked(navigator.clipboard.writeText).mockRejectedValueOnce(
      new Error("denied"),
    );
    await expect(writeClipboardText("x")).rejects.toBeInstanceOf(
      IrisClipboardError,
    );
  });

  it("copyTextFieldSelection copies slice", async () => {
    const ok = await copyTextFieldSelection("hello", { start: 1, end: 4 });
    expect(ok).toBe(true);
    expect(navigator.clipboard.writeText).toHaveBeenCalledWith("ell");
  });

  it("cutTextFieldSelection removes selected range", async () => {
    const result = await cutTextFieldSelection("hello", { start: 1, end: 4 });
    expect(result).toEqual({ value: "ho", caret: 1 });
  });

  it("pasteIntoTextField inserts clipboard at selection", async () => {
    const result = await pasteIntoTextField("hi", { start: 2, end: 2 });
    expect(result).toEqual({ value: "hipasted", caret: 8 });
  });

  it("normalizes pasted web html spacing after bold text", () => {
    const html = "<p><strong>标题：</strong>&nbsp;&nbsp;&nbsp;\t&nbsp;正文</p>";

    expect(normalizePastedEditorHtml(html)).toBe(
      "<p><strong>标题：</strong> 正文</p>",
    );
  });

  it("keeps code and pre spacing intact while normalizing prose", () => {
    const html =
      "<p><strong>标题：</strong>&nbsp;&nbsp;正文</p><pre>a&nbsp;&nbsp;b</pre><code>x&nbsp;&nbsp;y</code>";

    expect(normalizePastedEditorHtml(html)).toBe(
      "<p><strong>标题：</strong> 正文</p><pre>a&nbsp;&nbsp;b</pre><code>x&nbsp;&nbsp;y</code>",
    );
  });

  it("collapses non-code web html spacing during editor paste", () => {
    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p>前</p>",
      editorProps: {
        transformPastedHTML: normalizePastedEditorHtml,
      },
    });
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);

    try {
      const event = new Event("paste", { bubbles: true, cancelable: true });
      Object.defineProperty(event, "clipboardData", {
        value: {
          types: ["text/html", "text/plain"],
          getData: (type: string) => {
            if (type === "text/html") {
              return "<p><strong>标题：</strong>&nbsp;&nbsp;&nbsp;&nbsp;正文</p>";
            }
            if (type === "text/plain") return "标题：    正文";
            return "";
          },
        },
      });

      editor.view.dom.dispatchEvent(event);

      expect(editor.state.doc.textContent).toBe("前标题： 正文");
      expect(editor.state.doc.textContent).not.toContain("\u00a0\u00a0");
    } finally {
      editor.destroy();
    }
  });

  it("pasteIntoEditor ingests markdown with tight bold labels", async () => {
    vi.mocked(navigator.clipboard.readText).mockResolvedValueOnce(
      "1. **DP-Attention 同步：**多 DP 段的计算拖慢。",
    );

    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p></p>",
    });

    try {
      await pasteIntoEditor(editor);

      expect(editor.getHTML()).toContain(
        "<strong>DP-Attention 同步：</strong>",
      );
      expect(editor.getHTML()).not.toContain("**DP-Attention 同步：**");
    } finally {
      editor.destroy();
    }
  });

  it("pasteIntoEditor keeps ordinary text paste inline", async () => {
    vi.mocked(navigator.clipboard.readText).mockResolvedValueOnce("插入");

    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p>前后</p>",
    });
    editor.commands.setTextSelection(2);

    try {
      await pasteIntoEditor(editor);

      expect(editor.getHTML()).toBe("<p>前插入后</p>");
    } finally {
      editor.destroy();
    }
  });
});
