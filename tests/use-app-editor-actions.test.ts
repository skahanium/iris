import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { useAppEditorActions } from "@/hooks/useAppEditorActions";

describe("useAppEditorActions", () => {
  let editor: Editor;
  let host: HTMLDivElement;
  let root: Root;
  let api!: ReturnType<typeof useAppEditorActions>;

  function Harness() {
    api = useAppEditorActions({
      activeNoteIsClassified: false,
      activePathRef: { current: "note.md" },
      editorRef: { current: editor },
      getLiveMarkdown: () => "",
      inlineAi: {
        run: vi.fn(),
        runSlash: vi.fn(),
      },
      isMutationBlocked: () => true,
      scheduleUndoRedoStateRefresh: vi.fn(),
      sendSelectionToAi: vi.fn(),
      setAiStatus: vi.fn(),
    });
    return null;
  }

  beforeEach(() => {
    editor = new Editor({
      extensions: [StarterKit],
      content: "<p>Original</p>",
    });
    editor.commands.focus("end");
    editor.commands.insertContent(" local edit");
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);
    act(() => {
      root.render(createElement(Harness));
    });
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    editor.destroy();
  });

  it("rejects assistant insertion and undo while the persistence barrier is active", () => {
    expect(editor.getText()).toBe("Original local edit");

    act(() => {
      api.handleInsertToEditor("AI output");
    });
    expect(editor.getText()).toBe("Original local edit");

    act(() => {
      api.handleUndo();
    });
    expect(editor.getText()).toBe("Original local edit");
  });
});
