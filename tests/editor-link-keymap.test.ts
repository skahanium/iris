import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it, vi } from "vitest";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { IrisParagraphExtension } from "@/components/editor/extensions/IrisParagraphExtension";
import { LinkExtension } from "@/components/editor/extensions/LinkExtension";

describe("LinkExtension keyboard shortcut", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
    vi.restoreAllMocks();
  });

  it("Mod-k delegates to the Iris link editor instead of using window.prompt", () => {
    const promptSpy = vi.spyOn(window, "prompt");
    editor = new Editor({
      extensions: [
        IrisDocument,
        StarterKit.configure({ document: false, paragraph: false }),
        IrisParagraphExtension,
        LinkExtension,
      ],
      content: "<p>Visit docs here.</p>",
    });
    const openSpy = vi.fn((event: Event) => event.preventDefault());
    editor.view.dom.addEventListener("iris-open-link-editor", openSpy);

    editor.commands.selectAll();
    const handled = editor.commands.keyboardShortcut("Mod-k");

    expect(handled).toBe(true);
    expect(openSpy).toHaveBeenCalledTimes(1);
    expect(promptSpy).not.toHaveBeenCalled();
  });
});
