import { describe, expect, it } from "vitest";

import {
  editorBodyHtmlToMarkdown,
  normalizeTurndownEscapes,
} from "@/lib/markdown";
import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "../helpers/tiptap-serialize-harness";

const corpus = [
  { name: "gfm", md: "**bold** and `code`" },
  { name: "callout", md: "> [!note] Info\n> Body" },
  { name: "footnote", md: "Text[^a]\n\n[^a]: Body" },
  { name: "wiki-link", md: "See [[Project Note]]" },
  { name: "block raw html", md: "<div>raw</div>" },
  { name: "inline raw html", md: "Press <kbd>Ctrl</kbd>" },
  { name: "table", md: "| A | B |\n| --- | --- |\n| 1 | 2 |" },
  { name: "task list", md: "- [x] Done\n- [ ] Todo" },
] as const;

describe("serialization path contracts", () => {
  it("normalizes turndown bracket escapes in one shared helper", () => {
    expect(normalizeTurndownEscapes("\\[\\[Note\\]\\]")).toBe("[[Note]]");
  });

  it.each(corpus)("keeps core semantics for $name", ({ md }) => {
    const editor = createProductionEditorFromIngestedBody(md);
    try {
      const hotPath = pmSerializeBody(editor);
      const fallbackPath = editorBodyHtmlToMarkdown(editor.getHTML());

      for (const needle of semanticNeedles(md)) {
        expect(hotPath).toContain(needle);
      }

      if (md.includes("<")) {
        expect(fallbackPath).toContain("<");
      }
    } finally {
      editor.destroy();
    }
  });
});

function semanticNeedles(md: string): string[] {
  if (md.includes("[!note]")) return ["[!note]", "Body"];
  if (md.includes("[^a]")) return ["[^a]", "Body"];
  if (md.includes("[[Project Note]]")) return ["[[Project Note]]"];
  if (md.includes("<div>")) return ["<div>raw</div>"];
  if (md.includes("<kbd>")) return ["<kbd>Ctrl</kbd>"];
  if (md.includes("| A | B |")) return ["| A", "1"];
  if (md.includes("[x]")) return ["[x]", "[ ]"];
  return ["bold", "`code`"];
}
