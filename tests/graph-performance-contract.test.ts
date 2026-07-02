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

  it("keeps force simulation work off the React main-thread component", () => {
    const graph = read("src/components/graph/GraphView.tsx");
    const worker = read("src/workers/graph-layout.worker.ts");

    expect(graph).toContain("new Worker");
    expect(graph).toContain("graph-layout.worker");
    expect(graph).not.toContain("function forceSimulate");
    expect(worker).toContain("function forceSimulate");
    expect(worker).toContain("GraphLayoutRequest");
    expect(worker).toContain("GraphLayoutResponse");
  });
});
