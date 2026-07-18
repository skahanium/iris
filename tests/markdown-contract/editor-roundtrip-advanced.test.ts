/**
 * editor-roundtrip-advanced.test.ts — advanced-syntax coverage
 *
 * AGENTS.md §4.1 production gate: ingest → TipTap → PM serialize
 * (`createProductionEditorFromIngestedBody` / `pmSerializeBody`).
 */
import { afterEach, describe, expect, it } from "vitest";
import type { Editor } from "@tiptap/core";

import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "../helpers/tiptap-serialize-harness";

function bodyRoundTrip(bodyMd: string): string {
  const editor = createProductionEditorFromIngestedBody(bodyMd);
  try {
    return pmSerializeBody(editor);
  } finally {
    editor.destroy();
  }
}

// ═══════════════════════════════════════════════════════════════

describe("editor round-trip: callout / admonition blocks", () => {
  let editor: Editor | undefined;
  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("[BASELINE] [!note] callout text survives round-trip", () => {
    const md = "> [!note] Important\n> Body here.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("Important");
    expect(out).toContain("Body");
  });

  it("[BASELINE] [!warning] callout text survives round-trip", () => {
    const md = "> [!warning] Alert\n> Details.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("Alert");
    expect(out).toContain("Details");
  });

  it("[GAP] [!tip] callout type tag is preserved", () => {
    const md = "> [!tip] Pro Tip\n> Helpful content.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("Pro Tip");
    expect(out).toContain("Helpful");
    expect(out).toContain("[!tip]");
  });

  it("[GAP] [!danger] callout type tag is preserved", () => {
    const md = "> [!danger] Critical\n> Do not ignore.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("Critical");
    expect(out).toContain("ignore");
    expect(out).toContain("[!danger]");
  });

  it("[BASELINE] multiple callouts in same document survive", () => {
    const md = [
      "> [!note] First",
      "> First body.",
      "",
      "Normal paragraph.",
      "",
      "> [!warning] Second",
      "> Second body.",
    ].join("\n");
    const out = bodyRoundTrip(md);
    expect(out).toContain("First body");
    expect(out).toContain("Normal paragraph");
    expect(out).toContain("Second body");
  });

  it("[GAP] callout body with nested GFM loses list and code on round-trip", () => {
    const md = ["> [!info] Rich Callout", "> - list item", "> `code`"].join(
      "\n",
    );
    const out = bodyRoundTrip(md);
    expect(out).toContain("Rich Callout");
    expect(out).toContain("[!info]");
    expect(out).not.toContain("list item");
    expect(out).not.toContain("`code`");
  });
});

describe("editor round-trip: footnotes", () => {
  it("[BASELINE] footnote text survives round-trip", () => {
    const md = "Text with footnote[^1].\n\n[^1]: The body.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("footnote");
  });

  it("[GAP] footnote reference [^1] is not lost", () => {
    const md = "See [^note] for more.\n\n[^note]: The detail.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("note");
    expect(out).toContain("detail");
  });

  it("[GAP] multiple footnotes survive round-trip", () => {
    const md = [
      "See [^a] and [^b].",
      "",
      "[^a]: Note A.",
      "[^b]: Note B.",
    ].join("\n");
    const out = bodyRoundTrip(md);
    expect(out).toContain("Note A");
    expect(out).toContain("Note B");
  });

  it("[BASELINE] footnote with inline formatting survives", () => {
    const md = "Text[^fmt]\n\n[^fmt]: Content with **bold** and `code`.";
    const out = bodyRoundTrip(md);
    expect(out).toContain("bold");
    expect(out).toContain("code");
  });
});

describe("editor round-trip: raw HTML / preserve-only blocks", () => {
  it("[BASELINE] raw <div> text content survives round-trip", () => {
    const md = '<div class="note">content</div>';
    const out = bodyRoundTrip(md);
    expect(out).toContain("content");
  });

  it("[GAP] raw <kbd> preserved in output", () => {
    const md = "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>";
    const out = bodyRoundTrip(md);
    expect(out).toContain("Ctrl");
    expect(out).toContain("Press");
    expect(out).toContain("<kbd>Ctrl</kbd>");
    expect(out).toContain("<kbd>C</kbd>");
  });

  it("[BASELINE] HTML comments do not crash round-trip", () => {
    const md = "Text <!-- note --> more.";
    expect(() => bodyRoundTrip(md)).not.toThrow();
  });
});

describe("editor round-trip: mixed advanced + native GFM", () => {
  it("[BASELINE] callout + native GFM in same document", () => {
    const md = [
      "## Section",
      "",
      "**bold** and *italic*.",
      "",
      "> [!note] Info",
      "> With content.",
      "",
      "- list item",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
    ].join("\n");
    const out = bodyRoundTrip(md);
    expect(out).toContain("Section");
    expect(out).toContain("bold");
    expect(out).toContain("list item");
    expect(out).toContain("| A | B |");
  });

  it("[GAP] callout + footnote + table in same document", () => {
    const md = [
      "> [!info] Mixed",
      "> With footnote[^m] and table below.",
      "",
      "| Key | Value |",
      "| --- | --- |",
      "| A | 1 |",
      "",
      "[^m]: Mixed footnote.",
    ].join("\n");
    const out = bodyRoundTrip(md);
    expect(out).toContain("Mixed");
    expect(out).toContain("Mixed footnote");
    expect(out).toContain("| Key | Value |");
    expect(out).toContain("| A | 1 |");
  });

  it("[GAP] raw HTML beside native GFM", () => {
    const md = [
      "# Title",
      "",
      '<div class="box">HTML block</div>',
      "",
      "**Native** paragraph.",
      "",
      "- native list",
    ].join("\n");
    const out = bodyRoundTrip(md);
    expect(out).toContain("Title");
    expect(out).toContain("HTML block");
    expect(out).toContain("Native");
    expect(out).toContain("native list");
  });
});

describe("editor round-trip: full mixed stress test", () => {
  it("[BASELINE] all major syntax types survive round-trip", () => {
    const md = [
      "# Document Title",
      "",
      "**Bold** and *italic* and `code` and ~~strike~~.",
      "",
      "- [x] Done task",
      "- [ ] Pending task",
      "",
      "> blockquote here",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "```ts",
      "const x = 1;",
      "```",
      "",
      "[Link](https://example.com)",
      "",
      "[[WikiLink]]",
    ].join("\n");
    const out = bodyRoundTrip(md);
    expect(out).toContain("Document Title");
    expect(out).toContain("Bold");
    expect(out).toContain("[x]");
    expect(out).toContain("[ ]");
    expect(out).toContain("blockquote");
    expect(out).toContain("| A | B |");
    expect(out).toContain("```");
    expect(out).toContain("[Link](https://example.com)");
    expect(out).toContain("[[WikiLink]]");
  });
});
