import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

import {
  createProductionEditorFromIngestedBody,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("markdown block spacing", () => {
  it("ingest treats ordinary markdown block separators as source formatting, not spacer paragraphs", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "Paragraph A\n\nParagraph B",
    });
    expect(tipTapHtml).not.toContain('data-iris-spacer="true"');
    expect(tipTapHtml).toContain("Paragraph A");
    expect(tipTapHtml).toContain("Paragraph B");
  });

  it("serializes paragraphs with exactly one standard markdown blank line", () => {
    const editor = createProductionEditorFromIngestedBody(
      "Paragraph A\n\nParagraph B",
    );
    try {
      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");
      expect(md).toBe("Paragraph A\n\nParagraph B");
      expect(md).not.toMatch(/\n{3,}/);
    } finally {
      editor.destroy();
    }
  });

  it("normalizes multiple consecutive blank lines to one standard block gap", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "A\n\n\n\nB",
    });
    expect(tipTapHtml).not.toContain('data-iris-spacer="true"');
    expect(tipTapHtml).not.toContain("data-iris-gap-count");

    const editor = createProductionEditorFromIngestedBody("A\n\n\n\nB");
    try {
      const md = pmSerializeBody(editor).replace(/\r\n/g, "\n");
      expect(md).toBe("A\n\nB");
      expect(md).not.toMatch(/\n{3,}/);
    } finally {
      editor.destroy();
    }
  });

  it("does not treat user-entered empty paragraphs as contract spacers", () => {
    const editor = createProductionEditorFromIngestedBody(
      "Paragraph A\n\nParagraph B",
    );
    try {
      editor.commands.insertContentAt(
        editor.state.doc.content.size - 1,
        { type: "paragraph" },
        { updateSelection: false },
      );
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).not.toMatch(/\n{4,}/);
      expect(md).toMatch(/Paragraph A[\s\S]*\n\n[\s\S]*Paragraph B/);
    } finally {
      editor.destroy();
    }
  });

  it("serializes editor doc with plain empty paragraph without extra blank lines", () => {
    const editor = createProductionEditorFromIngestedBody("Paragraph A");
    try {
      editor.commands.insertContentAt(editor.state.doc.content.size, {
        type: "doc",
        content: [
          { type: "paragraph" },
          {
            type: "paragraph",
            content: [{ type: "text", text: "Paragraph B" }],
          },
        ],
      });
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).not.toMatch(/\n{4,}/);
      expect(md).toContain("Paragraph A");
      expect(md).toContain("Paragraph B");
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
