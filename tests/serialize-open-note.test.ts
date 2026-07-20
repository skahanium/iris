import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { afterEach, describe, expect, it } from "vitest";

import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { markdownBodyToEditorHtml, parseNoteForEditor } from "@/lib/markdown";
import { serializeOpenNote } from "@/lib/serialize-open-note";

function bodyEditor(html: string): Editor {
  return new Editor({
    extensions: [
      IrisDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: { levels: [1, 2, 3, 4, 5, 6] },
      }),
    ],
    content: html,
  });
}

describe("serializeOpenNote", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("persists the live editor body and removes a legacy system title", () => {
    editor = bodyEditor(markdownBodyToEditorHtml("Alpha\n\nBeta"));
    editor.commands.insertContentAt(editor.state.doc.content.size, {
      type: "paragraph",
      content: [{ type: "text", text: "SAVE_MARKER" }],
    });

    const md = serializeOpenNote({
      yaml: 'title: "Historical"\ntags: [work]',
      editor,
      bodyFallbackMd: "",
    });

    expect(md).not.toContain("title:");
    expect(md).toContain("tags: [work]");
    expect(md).toContain("SAVE_MARKER");
  });

  it("uses the persisted body when the editor is not ready", () => {
    editor = bodyEditor("<p></p>");
    const md = serializeOpenNote({
      yaml: null,
      editor,
      editorReady: false,
      bodyFallbackMd: "Loaded body",
    });

    expect(md).toBe("Loaded body\n");
  });
});

describe("parseNoteForEditor", () => {
  it("derives the title from the filename and keeps legacy YAML and H1 in content", () => {
    const md = "---\ntitle: Historical\n---\n\n# Section\n\nBody";
    const parsed = parseNoteForEditor(md, "File name");

    expect(parsed.title).toBe("File name");
    expect(parsed.bodyMd).toContain("# Section");
    expect(parsed.bodyMd).toContain("Body");
  });
});
