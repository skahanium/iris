import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { useEditorContextMenu } from "@/hooks/useEditorContextMenu";

describe("useEditorContextMenu", () => {
  let editor: Editor | null = null;
  let root: Root | null = null;
  let host: HTMLDivElement | null = null;
  let api: ReturnType<typeof useEditorContextMenu> | null = null;

  function renderHook(locked: boolean) {
    editor = new Editor({
      extensions: [StarterKit],
      content: "<p>Selected text</p>",
    });
    editor.commands.selectAll();
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    function Harness() {
      api = useEditorContextMenu(editor, true, vi.fn(), locked);
      return null;
    }

    act(() => {
      root?.render(createElement(Harness));
    });
  }

  afterEach(() => {
    act(() => root?.unmount());
    root = null;
    host?.remove();
    host = null;
    editor?.destroy();
    editor = null;
    api = null;
  });

  it("opens a clipboard-only menu for selected editable text", () => {
    renderHook(false);
    const preventDefault = vi.fn();
    const stopPropagation = vi.fn();

    act(() => {
      api?.handleContextMenu({
        preventDefault,
        stopPropagation,
        clientX: 10,
        clientY: 20,
      } as unknown as React.MouseEvent);
    });

    expect(preventDefault).toHaveBeenCalled();
    expect(stopPropagation).toHaveBeenCalled();
    expect(
      api?.groups.flatMap((group) => group.items.map((item) => item.id)),
    ).toEqual(["cut", "copy", "paste", "select-all"]);
  });

  it("keeps copy and select-all available for locked text", () => {
    renderHook(true);

    act(() => {
      api?.handleContextMenu({
        preventDefault: vi.fn(),
        stopPropagation: vi.fn(),
        clientX: 10,
        clientY: 20,
      } as unknown as React.MouseEvent);
    });

    expect(api?.menu.open).toBe(true);
    expect(
      api?.groups.flatMap((group) => group.items.map((item) => item.id)),
    ).toEqual(["copy", "select-all"]);
  });
});
