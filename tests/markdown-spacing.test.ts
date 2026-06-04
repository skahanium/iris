import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

import {
  createProductionEditorFromIngestedBody,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("markdown block spacing (contract space → spacer paragraph)", () => {
  it("ingest emits spacer paragraphs for blank lines between blocks", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "段落 A\n\n段落 B",
    });
    expect(tipTapHtml).toContain('data-iris-spacer="true"');
    expect(tipTapHtml).toContain("段落 A");
    expect(tipTapHtml).toContain("段落 B");
  });

  it("preserves blank line between paragraphs through PM export", () => {
    const editor = createProductionEditorFromIngestedBody("段落 A\n\n段落 B");
    try {
      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toMatch(/段落 A[\s\S]*\n\n[\s\S]*段落 B/);
    } finally {
      editor.destroy();
    }
  });

  it("preserves multiple consecutive blank lines as multiple spacers", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "A\n\n\n\nB",
    });
    expect(tipTapHtml).toContain('data-iris-gap-count="2"');

    const editor = createProductionEditorFromIngestedBody("A\n\n\n\nB");
    try {
      let gapAttr: number | null = null;
      editor.state.doc.descendants((node) => {
        if (node.type.name === "paragraph" && node.attrs.irisSpacer) {
          gapAttr = node.attrs.irisGapCount as number;
        }
      });
      expect(gapAttr).toBe(2);

      const md = normalizeMd(pmSerializeBody(editor));
      expect(md).toMatch(/A[\s\S]*\n\n[\s\S]*\n\n[\s\S]*B/);
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
