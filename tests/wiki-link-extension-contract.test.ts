import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("wiki link extension contract", () => {
  it("registers a suggestion popup for ASCII and full-width wiki-link triggers", () => {
    const source = readFileSync(
      "src/components/editor/extensions/WikiLinkExtension.ts",
      "utf8",
    );
    const matcher = readFileSync("src/lib/wiki-link-suggestions.ts", "utf8");

    expect(source).toContain("Suggestion<WikiLinkSuggestionItem");
    expect(source).toContain("WikiLinkSuggestionList");
    expect(source).toContain("findSuggestionMatch");
    expect(matcher).toContain('"[["');
    expect(matcher).toContain('"【【"');
  });
});
