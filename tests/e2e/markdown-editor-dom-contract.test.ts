import { describe, expect, it } from "vitest";

import {
  createProductionEditorFromIngestedBody,
  fullNoteRoundTrip,
  normalizeMd,
} from "../helpers/tiptap-serialize-harness";

describe("markdown editor DOM contract", () => {
  it("renders callouts, plain blockquotes, footnotes, and inline preserves with distinct DOM semantics", () => {
    const body = [
      "> [!note] Note",
      "> Note body.",
      "",
      "> [!info] Info",
      "> Info body.",
      "",
      "> [!tip] Tip",
      "> Tip body.",
      "",
      "> [!warning] Warning",
      "> Warning body.",
      "",
      "> [!danger] Danger",
      "> Danger body.",
      "",
      "> [!example] Example",
      "> Example body.",
      "",
      "> Plain quote.",
      "",
      "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.[^copy]",
      "",
      "[^copy]: Copy shortcut.",
    ].join("\n");
    const editor = createProductionEditorFromIngestedBody(body);

    try {
      for (const type of [
        "note",
        "info",
        "tip",
        "warning",
        "danger",
        "example",
      ]) {
        expect(
          editor.view.dom.querySelector(
            `blockquote[data-callout-type="${type}"]`,
          ),
        ).toBeInstanceOf(HTMLElement);
      }
      expect(
        editor.view.dom.querySelector("blockquote:not([data-callout-type])"),
      ).toBeInstanceOf(HTMLElement);
      expect(
        editor.view.dom.querySelector("p [data-type='preserve-inline']"),
      ).toBeInstanceOf(HTMLElement);
      expect(
        editor.view.dom.querySelector(
          '[data-footnote-ref="copy"] a[href="#footnote-copy"]',
        ),
      ).toBeInstanceOf(HTMLElement);
      expect(
        editor.view.dom.querySelector('[data-footnote-def="copy"]'),
      ).toBeInstanceOf(HTMLElement);
    } finally {
      editor.destroy();
    }
  });

  it("saves and reopens a representative markdown document without losing core syntax", () => {
    const note = [
      "---",
      'title: "Markdown E2E"',
      "---",
      "",
      "> [!warning] Heads up",
      "> Read [[Target Note]] before continuing.[^warn]",
      "",
      "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.",
      "",
      '<div class="raw">Raw block</div>',
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "- [x] Done",
      "- [ ] Todo",
      "",
      "[^warn]: Warning footnote.",
    ].join("\n");

    const saved = fullNoteRoundTrip(note);
    const reopened = normalizeMd(fullNoteRoundTrip(saved));

    expect(reopened).toContain("[!warning]");
    expect(reopened).toContain("[[Target Note]]");
    expect(reopened).toContain("<kbd>Ctrl</kbd>");
    expect(reopened).toContain('<div class="raw">Raw block</div>');
    expect(reopened).toContain("| A | B |");
    expect(reopened).toContain("- [x] Done");
    expect(reopened).toContain("[^warn]: Warning footnote.");
  });
});
