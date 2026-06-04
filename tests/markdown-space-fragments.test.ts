import { describe, expect, it } from "vitest";

import { classifyMarkdownCapabilities } from "@/lib/markdown-contract/contract";

describe("space fragment raw from marked", () => {
  it("documents how marked tokenizes multiple blank lines", () => {
    const fragments = classifyMarkdownCapabilities("A\n\n\n\nB");
    const spaces = fragments.filter((f) => f.syntaxKind === "space");
    expect(spaces.length).toBeGreaterThanOrEqual(1);
    // Used by ingest to emit one spacer per \n\n in the space raw
    expect(spaces.map((s) => JSON.stringify(s.raw))).toMatchInlineSnapshot(`
      [
        ""\\n\\n\\n\\n"",
      ]
    `);
  });
});
