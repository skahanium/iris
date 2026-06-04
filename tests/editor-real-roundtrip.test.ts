import { Editor } from "@tiptap/core";
import { afterEach, describe, expect, it } from "vitest";

import { markdownToHtmlPage } from "@/lib/markdown";
import { serializeOpenNote } from "@/lib/serialize-open-note";

import {
  createProductionEditorFromNote,
  fullNoteRoundTrip,
  normalizeMd,
} from "./helpers/tiptap-serialize-harness";

function createEditorFromMarkdown(md: string): Editor {
  return createProductionEditorFromNote(md);
}

function realEditorRoundTrip(md: string): string {
  return fullNoteRoundTrip(md);
}

function normalize(md: string): string {
  return normalizeMd(md);
}

function selectParagraphText(editor: Editor, text: string): void {
  let from: number | null = null;
  editor.state.doc.descendants((node, pos) => {
    if (node.type.name === "paragraph" && node.textContent === text) {
      from = pos + 1;
      return false;
    }
  });
  if (from === null) {
    throw new Error(`Paragraph not found: ${text}`);
  }
  editor.commands.setTextSelection({ from, to: from + text.length });
}

describe("real TipTap editor markdown round-trip", () => {
  let editor: Editor | undefined;

  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("preserves links, tasks, tables, headings, images, and wiki-links through the real editor", () => {
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
      "#### Deep Heading",
      "",
      "![diagram](https://example.com/x.png)",
      "",
      "See [[Architecture Notes]].",
      "",
      "> [!tip] Hint",
      "> Optional detail.",
    ].join("\n");

    const out = normalize(realEditorRoundTrip(md));

    expect(out).toContain("[Iris](https://example.com/docs)");
    expect(out).toContain("- [x] Done");
    expect(out).toContain("- [ ] Todo");
    expect(out).toContain("| A | B |");
    expect(out).toContain("| 1 | 2 |");
    expect(out).toContain("#### Deep Heading");
    expect(out).toContain("![diagram](https://example.com/x.png)");
    expect(out).toContain("[[Architecture Notes]]");
    expect(out).toContain("[!tip]");
    expect(out).toContain("Optional detail");
  });

  it("preserves blank lines between paragraphs after save round-trip", () => {
    const md = [
      "---",
      'title: "Spacing"',
      "---",
      "",
      "First paragraph.",
      "",
      "Second paragraph.",
    ].join("\n");

    const out = normalize(realEditorRoundTrip(md));
    expect(out).toMatch(/First paragraph\.[\s\S]*\n\n[\s\S]*Second paragraph\./);
  });

  it("does not remove a later body heading just because it matches the document title", () => {
    const md = [
      "---",
      'title: "Design"',
      "---",
      "",
      "Intro paragraph.",
      "",
      "# Design",
      "",
      "This is a legitimate section heading.",
    ].join("\n");

    const out = normalize(realEditorRoundTrip(md));

    expect(out).toContain("Intro paragraph.");
    expect(out).toContain("# Design");
    expect(out).toContain("This is a legitimate section heading.");
  });

  it("omits transient AI stream nodes from persisted markdown", () => {
    editor = createEditorFromMarkdown('---\ntitle: "AI"\n---\n\nStart.');
    editor.commands.insertAiStreamAtCursor({
      originalText: "Start.",
      action: "summarize",
    });
    editor.commands.updateAiStream("Temporary suggestion");

    const out = normalize(
      serializeOpenNote({
        yaml: 'title: "AI"',
        title: "AI",
        editor,
        bodyFallbackMd: "",
      }),
    );

    expect(out).toContain("Start.");
    expect(out).not.toContain("Temporary suggestion");
    expect(out).not.toContain("data-type");
    expect(out).not.toContain("ai-stream");
  });

  it("restores selected original text when autosave sees an unresolved AI stream", () => {
    editor = createEditorFromMarkdown(
      '---\ntitle: "AI"\n---\n\nReplace this sentence.',
    );
    selectParagraphText(editor, "Replace this sentence.");
    editor.commands.insertAiStreamAtCursor({
      originalText: "Replace this sentence.",
      action: "rewrite",
    });
    editor.commands.updateAiStream("Temporary rewrite");

    const out = normalize(
      serializeOpenNote({
        yaml: 'title: "AI"',
        title: "AI",
        editor,
        bodyFallbackMd: "",
      }),
    );

    expect(out).toContain("Replace this sentence.");
    expect(out).not.toContain("Temporary rewrite");
    expect(out).not.toContain("ai-stream");
  });
});

describe("markdown HTML export safety", () => {
  it("sanitizes dangerous raw HTML and URLs from exported pages", () => {
    const page = markdownToHtmlPage(
      [
        "# Export",
        "",
        '<img src="x" onerror="alert(1)">',
        "",
        '<script>alert("xss")</script>',
        "",
        "[bad](javascript:alert(1))",
      ].join("\n"),
      "Export",
    );

    expect(page).not.toContain("<script");
    expect(page).not.toContain("onerror");
    expect(page).not.toContain("javascript:");
    expect(page).toContain("<h1>Export</h1>");
  });
});
