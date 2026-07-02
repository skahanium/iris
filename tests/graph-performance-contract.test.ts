import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("graph performance and reduced-motion contract", () => {
  it("guards canvas animation for reduced motion and large graphs", () => {
    const graph = read("src/components/graph/GraphView.tsx");

    expect(graph).toContain("GRAPH_MAX_ANIMATED_NODES");
    expect(graph).toContain("prefers-reduced-motion: reduce");
    expect(graph).toContain("isGraphAnimationAllowed");
    expect(graph).toContain("drawGraphFrame");
    expect(graph).toContain("图谱暂无节点");
  });
});
