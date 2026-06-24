import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("graph note-open preparation contract", () => {
  it("prepares hovered graph nodes and atomically closes after open", () => {
    const graph = read("src/components/graph/GraphView.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");

    expect(graph).toContain("onPrepareNotePath");
    expect(graph).toContain("findNodeAtCanvasEvent");
    expect(graph).toContain("onMouseMove={handleMouseMove}");
    expect(graph).toMatch(/onPrepareNotePath\?\.\(node\.path,\s*node\.title\)/);
    expect(graph).toMatch(/await onOpenNote\(node\.path\)/);
    expect(graph).toMatch(/await onOpenNote\(node\.path\)[\s\S]*onClose\(\)/);
    expect(overlays).toContain("onPrepareNotePath={onPrepareNotePath}");
  });
});
