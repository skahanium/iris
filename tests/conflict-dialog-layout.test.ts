import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("conflict dialog layout", () => {
  it("keeps the version conflict comparison compact and padded", () => {
    const source = read("src/components/file/ConflictDialog.tsx");

    expect(source).toContain("w-[min(1040px,calc(100vw-4rem))]");
    expect(source).toContain("grid-cols-1 gap-4 px-6 py-3 md:grid-cols-2");
    expect(source).toContain("h-[min(46vh,18rem)]");
    expect(source).toContain("border-t border-border/60 px-6 py-4");
    expect(source).not.toContain("max-w-3xl");
    expect(source).not.toContain("h-72");
  });
});
