import { readFileSync } from "node:fs";
import { describe, expect, it, afterEach } from "vitest";
import { marked } from "marked";

import { markdownBodyToEditorHtml } from "@/lib/markdown";
import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";

afterEach(() => {
  marked.setOptions({ gfm: true, breaks: false });
});

describe("marked instance isolation", () => {
  it("editor ingest keeps project breaks behavior when global marked options change", () => {
    marked.setOptions({ gfm: true, breaks: false });

    const html = markdownBodyToEditorHtml("first line\nsecond line");

    expect(html).toContain("<br>");
    expect(html).toContain("second line");
  });

  it("contract classification keeps GFM table support when global marked options change", () => {
    marked.setOptions({ gfm: false });

    const fragments = classifyMarkdownCapabilities(
      "| A | B |\n| --- | --- |\n| 1 | 2 |",
    );

    expect(
      fragments.some(
        (fragment) =>
          fragment.syntaxKind === "table" && fragment.capability === "native",
      ),
    ).toBe(true);
  });

  it("source modules do not import the marked singleton outside the markdown factory", () => {
    const files = [
      "src/lib/editor-ingest.ts",
      "src/lib/markdown-contract/contract.ts",
      "src/lib/markdown-render.ts",
    ];

    for (const file of files) {
      const source = readFileSync(file, "utf8");
      expect(source).not.toContain('import { marked } from "marked"');
    }
  });
});
