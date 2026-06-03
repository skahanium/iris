import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

describe("ingestMarkdownForEditor callouts", () => {
  it("does not nest blockquote for callout body lines", () => {
    const { tipTapHtml } = ingestMarkdownForEditor({
      bodyMarkdown: "> [!note] Info\n> Callout body.",
    });
    expect(tipTapHtml).toContain('data-callout-type="note"');
    expect(tipTapHtml).toContain("Callout body.");
    expect(tipTapHtml.match(/<blockquote/g)?.length).toBe(1);
  });
});
