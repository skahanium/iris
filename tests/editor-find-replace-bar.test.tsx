import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, describe, expect, it, vi } from "vitest";

import { EditorFindReplaceBar } from "@/components/editor/EditorFindReplaceBar";
import { FindHighlightExtension } from "@/components/editor/extensions/FindHighlightExtension";

let root: Root | null = null;
let host: HTMLDivElement | null = null;
let editor: Editor | null = null;

function renderBar(mode: "find" | "replace" = "replace") {
  host = document.createElement("div");
  document.body.append(host);
  editor = new Editor({
    extensions: [StarterKit, FindHighlightExtension],
    content: "<p>Alpha beta alpha</p>",
  });
  root = createRoot(host);
  act(() => {
    root?.render(
      <EditorFindReplaceBar
        editor={editor}
        mode={mode}
        open
        onClose={() => {}}
      />,
    );
  });
}

function changeInput(name: string, value: string): void {
  const input = document.querySelector<HTMLInputElement>(
    `input[aria-label="${name}"]`,
  );
  if (!input) throw new Error(`missing input: ${name}`);
  act(() => {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

afterEach(() => {
  if (root) {
    act(() => root?.unmount());
  }
  editor?.destroy();
  host?.remove();
  root = null;
  host = null;
  editor = null;
});

describe("EditorFindReplaceBar", () => {
  it("shows match count for current document find", () => {
    renderBar("find");

    changeInput("查找", "alpha");

    expect(document.body.textContent).toContain("1 / 2");
  });

  it("replaces all matches in the current ProseMirror document", () => {
    renderBar("replace");

    changeInput("查找", "alpha");
    changeInput("替换为", "gamma");
    act(() => {
      document
        .querySelector<HTMLButtonElement>('[data-testid="replace-all"]')
        ?.click();
    });

    expect(editor?.getText()).toBe("gamma beta gamma");
  });

  it("closes when Escape is pressed inside the bar", () => {
    const onClose = vi.fn();
    host = document.createElement("div");
    document.body.append(host);
    editor = new Editor({
      extensions: [StarterKit, FindHighlightExtension],
      content: "<p>Alpha</p>",
    });
    root = createRoot(host);
    act(() => {
      root?.render(
        <EditorFindReplaceBar
          editor={editor}
          mode="find"
          open
          onClose={onClose}
        />,
      );
    });

    act(() => {
      document
        .querySelector<HTMLInputElement>('input[aria-label="查找"]')
        ?.dispatchEvent(
          new KeyboardEvent("keydown", { key: "Escape", bubbles: true }),
        );
    });

    expect(onClose).toHaveBeenCalledTimes(1);
  });
});
