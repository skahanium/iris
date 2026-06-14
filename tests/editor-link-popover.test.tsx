import type { Editor } from "@tiptap/react";
import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import { TipTapEditor } from "@/components/editor/TipTapEditor";
import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";

describe("TipTapEditor link popover", () => {
  let host: HTMLDivElement;
  let root: Root;
  let editor: Editor | null;

  beforeEach(() => {
    host = document.createElement("div");
    document.body.appendChild(host);
    root = createRoot(host);
    editor = null;
  });

  afterEach(() => {
    act(() => root.unmount());
    host.remove();
    vi.restoreAllMocks();
  });

  async function renderEditor(markdown = "Visit docs here.") {
    await act(async () => {
      root.render(
        <TipTapEditor
          initialBodyMarkdown={markdown}
          onEditorReady={(nextEditor) => {
            editor = nextEditor;
          }}
        />,
      );
    });
    expect(editor).not.toBeNull();
  }

  function setInputValue(input: HTMLInputElement, value: string) {
    const setter = Object.getOwnPropertyDescriptor(
      HTMLInputElement.prototype,
      "value",
    )?.set;
    setter?.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  }

  it("opens an Iris link editor on Cmd/Ctrl+K and writes a Markdown link", async () => {
    const promptSpy = vi.spyOn(window, "prompt");
    await renderEditor();

    act(() => {
      editor?.commands.setTextSelection({ from: 1, to: 6 });
      editor?.commands.keyboardShortcut("Mod-k");
    });

    const dialog = host.querySelector('[data-testid="editor-link-popover"]');
    expect(dialog).toBeInstanceOf(HTMLElement);
    expect(promptSpy).not.toHaveBeenCalled();

    const input = host.querySelector<HTMLInputElement>(
      '[data-testid="editor-link-url-input"]',
    );
    expect(input).toBeInstanceOf(HTMLInputElement);

    await act(async () => {
      setInputValue(input!, "https://example.com/docs");
    });

    await act(async () => {
      host
        .querySelector<HTMLButtonElement>('[data-testid="editor-link-apply"]')
        ?.click();
    });

    expect(editor?.isActive("link", { href: "https://example.com/docs" })).toBe(
      true,
    );
    expect(editorDocToMarkdown(editor!)).toContain(
      "[Visit](https://example.com/docs)",
    );
  });

  it("removes an existing link through the link editor", async () => {
    await renderEditor("[Visit](https://example.com/docs) docs here.");

    act(() => {
      editor?.commands.setTextSelection({ from: 1, to: 6 });
      editor?.commands.keyboardShortcut("Mod-k");
    });

    expect(
      host.querySelector<HTMLInputElement>(
        '[data-testid="editor-link-url-input"]',
      )?.value,
    ).toBe("https://example.com/docs");

    await act(async () => {
      host
        .querySelector<HTMLButtonElement>('[data-testid="editor-link-remove"]')
        ?.click();
    });

    expect(editor?.isActive("link")).toBe(false);
    expect(editorDocToMarkdown(editor!)).toContain("Visit docs here.");
  });
});
