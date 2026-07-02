import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("lazy overlay loading fallback contract", () => {
  it("keeps lazy loading fallbacks inside real overlay shells", () => {
    const overlays = read("src/components/layout/AppOverlays.tsx");

    expect(overlays).toContain("function LazyOverlayFallback");
    expect(overlays).toContain("<IrisOverlay");
    expect(overlays).toContain('size="management"');
    expect(overlays).toContain('size="wide"');
    expect(overlays).toContain('size="graph"');
    expect(overlays).toContain('title="管理中心"');
    expect(overlays).toContain('label="管理中心加载中"');
    expect(overlays).toContain('overlays.closeOverlay("managementCenter")');
    expect(overlays).toContain('label="版本记录加载中"');
    expect(overlays).toContain('overlays.closeOverlay("version")');
    expect(overlays).toContain('label="知识图谱加载中"');
    expect(overlays).toContain('overlays.closeOverlay("graph")');
    expect(overlays).not.toContain("function OverlayLoadingSurface");
    expect(overlays).not.toContain("<Suspense fallback={null}>");
  });
});
