import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { readFileSync } from "node:fs";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { describe, expect, it, vi } from "vitest";

import {
  TipTapEditor,
  type Editor as ReactEditor,
} from "@/components/editor/TipTapEditor";
import { resetEditorContentBaseline } from "@/lib/editor-baseline";
import { EDITOR_PARSE_OPTIONS } from "@/lib/editor-parse-options";
import {
  clearCachedEditorHtml,
  editorHtmlDigest,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";
import { insertAssistantMarkdownAtCursor } from "@/lib/editor-insert";
import { createProductionEditorFromIngestedBody } from "./helpers/tiptap-serialize-harness";

function expectReadyEditor(editor: ReactEditor | null): ReactEditor {
  expect(editor).not.toBeNull();
  if (!editor) throw new Error("Expected editor to be ready");
  return editor;
}

function typeTextThroughInputRules(editor: Editor, text: string) {
  for (const ch of text) {
    const { from, to } = editor.state.selection;
    let handled = false;
    editor.view.someProp("handleTextInput", (handler) => {
      if (handler(editor.view, from, to, ch, () => editor.state.tr)) {
        handled = true;
        return true;
      }
      return false;
    });
    if (!handled) {
      editor.commands.insertContent(ch);
    }
  }
}

describe("editor performance regressions", () => {
  it("keeps markdown shortcut input rules active for headings and lists", () => {
    const editor = createProductionEditorFromIngestedBody("");

    typeTextThroughInputRules(editor, "# ");
    expect(editor.state.doc.child(0).type.name).toBe("heading");
    expect(editor.state.doc.child(0).attrs.level).toBe(1);

    editor.commands.clearContent();
    typeTextThroughInputRules(editor, "1. ");
    expect(editor.state.doc.child(0).type.name).toBe("orderedList");

    editor.commands.clearContent();
    typeTextThroughInputRules(editor, "+ ");
    expect(editor.state.doc.child(0).type.name).toBe("bulletList");

    editor.destroy();
  });

  it("inserts assistant markdown as block nodes instead of one huge text paragraph", () => {
    const editor = new Editor({
      extensions: [StarterKit],
      content: "<p>Intro</p>",
    });
    editor.commands.setTextSelection(editor.state.doc.content.size - 1);

    const largeMarkdown = Array.from(
      { length: 120 },
      (_, i) => `## Section ${i + 1}\n\n${"正文 ".repeat(24).trim()}`,
    ).join("\n\n");

    insertAssistantMarkdownAtCursor(editor, largeMarkdown);

    expect(editor.state.doc.childCount).toBeGreaterThan(120);
    expect(editor.state.doc.child(1).type.name).toBe("heading");
    expect(editor.state.doc.child(2).type.name).toBe("paragraph");
    expect(editor.state.doc.child(1).textContent).toBe("Section 1");
    expect(editor.getHTML()).not.toContain("## Section 1\n\n");

    editor.destroy();
  });

  it("resets loaded document content as a clean undo baseline", () => {
    const editor = createProductionEditorFromIngestedBody("Original body.");

    editor.commands.setTextSelection(editor.state.doc.content.size - 1);
    editor.commands.insertContent(" Local edit.");
    expect(editor.can().undo()).toBe(true);

    resetEditorContentBaseline(editor, "<p>Loaded baseline body.</p>", {
      parseOptions: EDITOR_PARSE_OPTIONS,
    });

    expect(editor.getText()).toContain("Loaded baseline body.");
    expect(editor.can().undo()).toBe(false);
    expect(editor.commands.undo()).toBe(false);
    expect(editor.getText()).toContain("Loaded baseline body.");
    expect(editor.state.selection.head).toBeLessThan(
      editor.state.doc.content.size,
    );

    editor.destroy();
  });

  it("keeps TipTapEditor document loads out of undo history", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    let editor: ReactEditor | null = null;

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Opened body should be the clean baseline.",
          reingestKey: 11,
          onEditorReady: (nextEditor: ReactEditor | null) => {
            editor = nextEditor;
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(editor?.getText()).toContain(
        "Opened body should be the clean baseline.",
      );
    });

    const loadedEditor = expectReadyEditor(editor);
    expect(loadedEditor.can().undo()).toBe(false);
    expect(loadedEditor.commands.undo()).toBe(false);
    expect(loadedEditor.getText()).toContain(
      "Opened body should be the clean baseline.",
    );

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Reingested body is another clean baseline.",
          reingestKey: 12,
          onEditorReady: (nextEditor: ReactEditor | null) => {
            editor = nextEditor;
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(editor?.getText()).toContain(
        "Reingested body is another clean baseline.",
      );
    });

    const reingestedEditor = expectReadyEditor(editor);
    expect(reingestedEditor.can().undo()).toBe(false);
    expect(reingestedEditor.state.selection.head).toBeLessThan(
      reingestedEditor.state.doc.content.size,
    );

    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("keeps cached editor HTML loads out of undo history", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    let editor: ReactEditor | null = null;
    const cacheKey = "cache-hit.md";
    const markdown = "Fallback body for cache digest.";

    clearCachedEditorHtml(cacheKey);
    setCachedEditorHtml(
      cacheKey,
      "<p>Cached body baseline.</p>",
      editorHtmlDigest(markdown),
    );

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: markdown,
          contentCacheKey: cacheKey,
          reingestKey: 14,
          onEditorReady: (nextEditor: ReactEditor | null) => {
            editor = nextEditor;
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(editor?.getText()).toContain("Cached body baseline.");
    });

    const cachedEditor = expectReadyEditor(editor);
    expect(cachedEditor.can().undo()).toBe(false);
    expect(cachedEditor.commands.undo()).toBe(false);
    expect(cachedEditor.getText()).toContain("Cached body baseline.");

    act(() => {
      root.unmount();
    });
    clearCachedEditorHtml(cacheKey);
    container.remove();
  });

  it("keeps prepared editor HTML loads out of undo history", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    let editor: ReactEditor | null = null;

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Fallback body.",
          initialEditorHtml: "<p>Prepared body baseline.</p>",
          reingestKey: 13,
          onEditorReady: (nextEditor: ReactEditor | null) => {
            editor = nextEditor;
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(editor?.getText()).toContain("Prepared body baseline.");
    });

    const preparedEditor = expectReadyEditor(editor);
    expect(preparedEditor.can().undo()).toBe(false);
    expect(preparedEditor.commands.undo()).toBe(false);
    expect(preparedEditor.getText()).toContain("Prepared body baseline.");

    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("coalesces expensive body stats after edit transactions", async () => {
    vi.useFakeTimers();
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    const onStats = vi.fn();
    let editor: ReactEditor | null = null;

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "初始正文",
          onEditorReady: (ed: ReactEditor | null) => {
            editor = ed;
          },
          onBodyStatsChange: onStats,
        }),
      );
    });

    expect(editor).not.toBeNull();
    onStats.mockClear();

    act(() => {
      editor!.commands.insertContent("追加内容");
    });

    expect(onStats).not.toHaveBeenCalled();

    await act(async () => {
      vi.advanceTimersByTime(450);
    });

    expect(onStats).toHaveBeenCalledTimes(1);

    act(() => {
      root.unmount();
    });
    container.remove();
    vi.useRealTimers();
  });

  it("applies parsed body content on first mount when reingestKey is non-zero", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    let editor: ReactEditor | null = null;

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Visible body after a later document open.",
          reingestKey: 2,
          onEditorReady: (nextEditor: ReactEditor | null) => {
            editor = nextEditor;
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(editor?.getText()).toContain(
        "Visible body after a later document open.",
      );
    });

    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("fires first-frame readiness only after the body is in ProseMirror", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    let firstFrameText: string | null = null;

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Ready means the body is visible.",
          reingestKey: 3,
          onFirstFrameReady: (editor: ReactEditor) => {
            firstFrameText = editor.getText();
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(firstFrameText).toContain("Ready means the body is visible.");
    });

    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("fires first-frame readiness again after a same-editor body reingest", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    const firstFrameTexts: string[] = [];

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "First body.",
          reingestKey: 1,
          onFirstFrameReady: (editor: ReactEditor) => {
            firstFrameTexts.push(editor.getText());
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(firstFrameTexts.at(-1)).toContain("First body.");
    });

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Second body after reingest.",
          reingestKey: 2,
          onFirstFrameReady: (editor: ReactEditor) => {
            firstFrameTexts.push(editor.getText());
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(firstFrameTexts.at(-1)).toContain("Second body after reingest.");
    });

    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("keeps the editor instance stable after opening a note with code blocks", async () => {
    const container = document.createElement("div");
    document.body.appendChild(container);
    const root: Root = createRoot(container);
    const readyEditors: ReactEditor[] = [];

    await act(async () => {
      root.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: [
            "```bash",
            "npm install -g @mimo-ai/cli",
            "```",
            "",
            "After code block.",
          ].join("\n"),
          reingestKey: 4,
          onEditorReady: (editor: ReactEditor | null) => {
            if (editor) readyEditors.push(editor);
          },
        }),
      );
    });

    await vi.waitFor(() => {
      expect(readyEditors.at(-1)?.getText()).toContain("After code block.");
    });

    await act(async () => {
      await Promise.resolve();
      await Promise.resolve();
    });

    expect(new Set(readyEditors).size).toBe(1);

    act(() => {
      root.unmount();
    });
    container.remove();
  });

  it("does not rebuild the whole outline from selectionUpdate events", () => {
    const source = readFileSync(
      "src/components/editor/EditorOutline.tsx",
      "utf8",
    );

    expect(source).not.toContain('editor.on("selectionUpdate", onUpdate)');
    expect(source).toContain('editor.on("selectionUpdate", updateActiveIndex)');
  });

  it("schedules undo/redo refresh for every transaction and command click", () => {
    const source = readFileSync("src/App.tsx", "utf8");

    expect(source).toContain("scheduleUndoRedoStateRefresh");
    expect(source).toContain("requestAnimationFrame");
    expect(source).not.toContain("if (!transaction.docChanged) return");
    expect(source).toMatch(/handleUndo[\s\S]*scheduleUndoRedoStateRefresh/);
    expect(source).toMatch(/handleRedo[\s\S]*scheduleUndoRedoStateRefresh/);
  });
});
