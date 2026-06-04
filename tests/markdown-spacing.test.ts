import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

import {
  createProductionEditorFromIngestedBody,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("markdown block spacing", () => {
  it("ingest treats blank lines as block boundaries without spacer paragraphs", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "段落 A\n\n段落 B",
    });
    expect(tipTapHtml).not.toContain("data-iris-spacer");
    expect(tipTapHtml).toContain("段落 A");
    expect(tipTapHtml).toContain("段落 B");
  });

  it("serializes paragraphs with one standard markdown blank line", () => {
    const editor = createProductionEditorFromIngestedBody("段落 A\n\n段落 B");
    try {
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toBe("段落 A\n\n段落 B");
      expect(md).not.toMatch(/\n{4,}/);
    } finally {
      editor.destroy();
    }
  });

  it("normalizes multiple consecutive blank lines to one standard block gap", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "A\n\n\n\nB",
    });
    expect(tipTapHtml).not.toContain("data-iris-gap-count");

    const editor = createProductionEditorFromIngestedBody("A\n\n\n\nB");
    try {
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toBe("A\n\nB");
      expect(md).not.toMatch(/\n{4,}/);
    } finally {
      editor.destroy();
    }
  });

  it("does not treat user-entered empty paragraphs as contract spacers", () => {
    const editor = createProductionEditorFromIngestedBody("段落 A\n\n段落 B");
    try {
      editor.commands.insertContentAt(
        editor.state.doc.content.size - 1,
        { type: "paragraph" },
        { updateSelection: false },
      );
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).not.toMatch(/\n{4,}/);
      expect(md).toMatch(/段落 A[\s\S]*\n\n[\s\S]*段落 B/);
    } finally {
      editor.destroy();
    }
  });

  it("serializes editor doc with plain empty paragraph without extra blank lines", () => {
    const editor = createProductionEditorFromIngestedBody("段落 A");
    try {
      editor.commands.insertContentAt(editor.state.doc.content.size, {
        type: "doc",
        content: [
          { type: "paragraph" },
          { type: "paragraph", content: [{ type: "text", text: "段落 B" }] },
        ],
      });
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).not.toMatch(/\n{4,}/);
      expect(md).toContain("段落 A");
      expect(md).toContain("段落 B");
    } finally {
      editor.destroy();
    }
  });

  it("preserves blank line around headings", () => {
    const body = ["Intro.", "", "## Section", "", "Body."].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toContain("Intro.");
      expect(md).toContain("## Section");
      expect(md).toContain("Body.");
      expect(md).toMatch(/Intro\.[\s\S]*\n\n[\s\S]*## Section/);
    } finally {
      editor.destroy();
    }
  });
});
