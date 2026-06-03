import { MarkdownSerializer } from "prosemirror-markdown";
import { afterEach, describe, expect, it, vi } from "vitest";

import * as markdownLib from "@/lib/markdown";

import {
  createProductionEditorFromBody,
  createProductionEditorFromIngestedBody,
  fullNoteRoundTrip,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("editorDocToMarkdown (prosemirror-markdown hot path)", () => {
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it("serializes a simple paragraph without Turndown fallback", () => {
    const turndownSpy = vi
      .spyOn(markdownLib, "editorBodyHtmlToMarkdown")
      .mockImplementation(() => {
        throw new Error("Turndown must not run for native GFM");
      });

    const editor = createProductionEditorFromBody("Hello **world**.");
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("Hello");
      expect(md).toContain("world");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips wiki-links via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = "See [[Architecture Notes]] for details.";
    const editor = createProductionEditorFromBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("[[Architecture Notes]]");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips GFM tables and task lists via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = [
      "- [x] Done",
      "- [ ] Todo",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
    ].join("\n");

    const editor = createProductionEditorFromBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(md).toContain("- [x] Done");
      expect(md).toContain("- [ ] Todo");
      expect(md).toContain("| A | B |");
      expect(md).toContain("| 1 | 2 |");
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("round-trips preserve-only callout blocks via PM serializer", () => {
    const turndownSpy = vi.spyOn(markdownLib, "editorBodyHtmlToMarkdown");

    const body = "> [!note] Info\n> Callout body.";
    const editor = createProductionEditorFromIngestedBody(body);
    try {
      const md = pmSerializeBody(editor);
      expect(normalizeMd(md)).toBe(normalizeMd(body));
      expect(turndownSpy).not.toHaveBeenCalled();
    } finally {
      editor.destroy();
    }
  });

  it("falls back to Turndown when prosemirror-markdown throws", () => {
    const serializeSpy = vi
      .spyOn(MarkdownSerializer.prototype, "serialize")
      .mockImplementation(() => {
        throw new Error("unsupported node");
      });
    const turndown = vi
      .spyOn(markdownLib, "editorBodyHtmlToMarkdown")
      .mockReturnValue("turndown-body");

    const editor = createProductionEditorFromBody("Fallback path.");
    try {
      expect(pmSerializeBody(editor)).toBe("turndown-body");
      expect(turndown).toHaveBeenCalled();
    } finally {
      serializeSpy.mockRestore();
      editor.destroy();
    }
  });
});

describe("serializeOpenNote integration (PM + ingest)", () => {
  it("preserves mixed advanced syntax through full note pipeline", () => {
    const md = [
      "---",
      'title: "PM Round Trip"',
      "---",
      "",
      "See [[Target Note]].",
      "",
      "> [!warning] Heads up",
      "> Stay careful.",
      "",
      "| Col |",
      "| --- |",
      "| x |",
    ].join("\n");

    const out = normalizeMd(fullNoteRoundTrip(md));
    expect(out).toContain("[[Target Note]]");
    expect(out).toContain("[!warning]");
    expect(out).toContain("Stay careful");
    expect(out).toContain("| Col |");
  });

  it("preserves calloutType on blockquote after ingest", () => {
    const editor = createProductionEditorFromIngestedBody(
      "> [!note] Info\n> Callout body.",
    );
    try {
      let found = false;
      editor.state.doc.descendants((node) => {
        if (node.type.name === "blockquote" && node.attrs.calloutType === "note") {
          found = true;
        }
      });
      expect(found).toBe(true);
    } finally {
      editor.destroy();
    }
  });

  it("matches production editor round-trip for links, tasks, tables, wiki-links", () => {
    const md = [
      "---",
      'title: "Round Trip"',
      "---",
      "",
      "See [Iris](https://example.com/docs).",
      "",
      "- [x] Done",
      "- [ ] Todo",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "See [[Architecture Notes]].",
    ].join("\n");

    const out = normalizeMd(fullNoteRoundTrip(md));
    expect(out).toContain("[Iris](https://example.com/docs)");
    expect(out).toContain("- [x] Done");
    expect(out).toContain("| A | B |");
    expect(out).toContain("[[Architecture Notes]]");
  });
});
