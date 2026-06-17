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
import { insertAssistantMarkdownAtCursor } from "@/lib/editor-insert";
import { createProductionEditorFromIngestedBody } from "./helpers/tiptap-serialize-harness";

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
