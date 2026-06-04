import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it, vi } from "vitest";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { IrisParagraphExtension } from "@/components/editor/extensions/IrisParagraphExtension";
import { LinkExtension } from "@/components/editor/extensions/LinkExtension";

describe("LinkExtension keyboard shortcut", () => {
  let editor: Editor | undefined;
  const promptSpy = vi.spyOn(window, "prompt");

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
    promptSpy.mockReset();
  });

  it("Mod-k applies link mark when user enters a safe URL", () => {
    promptSpy.mockReturnValue("https://example.com/docs");

    editor = new Editor({
      extensions: [
        IrisDocument,
        StarterKit.configure({ document: false, paragraph: false }),
        IrisParagraphExtension,
        LinkExtension,
      ],
      content: "<p>Visit docs here.</p>",
    });

    editor.commands.selectAll();
    const handled = editor.commands.keyboardShortcut("Mod-k");
    expect(handled).toBe(true);
    expect(editor.isActive("link", { href: "https://example.com/docs" })).toBe(
      true,
    );
  });
});
