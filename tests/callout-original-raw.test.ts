import { describe, expect, it } from "vitest";

import {
  createProductionEditorFromIngestedBody,
  normalizeMd,
  pmSerializeBody,
} from "./helpers/tiptap-serialize-harness";

describe("callout originalRaw preservation", () => {
  it("exports untouched callout markdown verbatim on save", () => {
    const raw = "> [!note] Info\n> Callout body.";
    const editor = createProductionEditorFromIngestedBody(raw);
    try {
      expect(normalizeMd(pmSerializeBody(editor))).toBe(normalizeMd(raw));
    } finally {
      editor.destroy();
    }
  });

  it("clears originalRaw after editing callout text", () => {
    const raw = "> [!note] Info\n> Callout body.";
    const editor = createProductionEditorFromIngestedBody(raw);
    try {
      let calloutPos = -1;
      editor.state.doc.descendants((node, pos) => {
        if (
          node.type.name === "blockquote" &&
          node.attrs.calloutType === "note"
        ) {
          calloutPos = pos;
        }
      });
      expect(calloutPos).toBeGreaterThanOrEqual(0);
      editor.commands.focus();
      editor.commands.insertContentAt(calloutPos + 2, "X");
      const out = normalizeMd(pmSerializeBody(editor));
      expect(out).not.toBe(normalizeMd(raw));
      expect(out).toContain("[!note]");
    } finally {
      editor.destroy();
    }
  });
});
