import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "../helpers/tiptap-serialize-harness";

describe("inline raw HTML preservation", () => {
  it("ingests safe inline raw HTML as inline preserve atoms inside one paragraph", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.",
    });

    expect(result.tipTapHtml).toContain('data-type="preserve-inline"');
    expect(result.tipTapHtml).not.toContain('data-type="preserve-block"');
    expect(result.tipTapHtml.trim()).toMatch(/^<p>Press .* \+ .*<\/p>$/);
  });

  it("serializes preserveInline nodes back to the original raw HTML", () => {
    const editor = createProductionEditorFromIngestedBody(
      "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>.",
    );

    try {
      const out = pmSerializeBody(editor);
      expect(out).toContain("Press");
      expect(out).toContain("<kbd>Ctrl</kbd>");
      expect(out).toContain("<kbd>C</kbd>");
    } finally {
      editor.destroy();
    }
  });

  it("keeps block raw HTML as preserveBlock", () => {
    const result = ingestMarkdownForEditor({
      bodyMarkdown: "<div>raw</div>",
    });

    expect(result.tipTapHtml).toContain('data-type="preserve-block"');
    expect(result.tipTapHtml).not.toContain('data-type="preserve-inline"');
  });
});
