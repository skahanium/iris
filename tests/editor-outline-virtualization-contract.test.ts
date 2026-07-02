import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor outline virtualization contract", () => {
  it("uses the shared virtualizer instead of rendering every heading", () => {
    const outline = read("src/components/editor/EditorOutline.tsx");

    expect(outline).toContain("@tanstack/react-virtual");
    expect(outline).toContain("useVirtualizer");
    expect(outline).toContain("outlineVirtualizer.getVirtualItems()");
    expect(outline).not.toContain(
      "entries.map((entry, index) => renderItem(entry, index))",
    );
  });
});
