import { act, createElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it } from "vitest";

import { TipTapEditor, type Editor } from "@/components/editor/TipTapEditor";

describe("TipTapEditor persistence interaction lock", () => {
  let host: HTMLDivElement | null = null;
  let root: Root | null = null;

  afterEach(() => {
    act(() => root?.unmount());
    host?.remove();
    root = null;
    host = null;
  });

  it("sets the real editor non-editable while a departure barrier is active and restores it on release", async () => {
    const editorRef: { current: Editor | null } = { current: null };
    host = document.createElement("div");
    document.body.append(host);
    root = createRoot(host);

    await act(async () => {
      root?.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Original body",
          locked: false,
          onEditorReady: (next: Editor | null) => {
            editorRef.current = next;
          },
        }),
      );
    });
    expect(editorRef.current?.isEditable).toBe(true);

    await act(async () => {
      root?.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Original body",
          locked: true,
          onEditorReady: (next: Editor | null) => {
            editorRef.current = next;
          },
        }),
      );
    });
    expect(editorRef.current?.isEditable).toBe(false);

    await act(async () => {
      root?.render(
        createElement(TipTapEditor, {
          initialBodyMarkdown: "Original body",
          locked: false,
          onEditorReady: (next: Editor | null) => {
            editorRef.current = next;
          },
        }),
      );
    });
    expect(editorRef.current?.isEditable).toBe(true);
  });
});
