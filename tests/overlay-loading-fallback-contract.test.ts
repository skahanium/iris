import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("lazy overlay loading fallback contract", () => {
  it("does not show blank overlays while lazy chunks load", () => {
    const overlays = read("src/components/layout/AppOverlays.tsx");

    expect(overlays).toContain("function OverlayLoadingSurface");
    expect(overlays).toContain(
      'fallback={<OverlayLoadingSurface label="管理中心加载中" />}>',
    );
    expect(overlays).toContain(
      'fallback={<OverlayLoadingSurface label="版本记录加载中" />}>',
    );
    expect(overlays).toContain(
      'fallback={<OverlayLoadingSurface label="知识图谱加载中" />}>',
    );
    expect(overlays).not.toContain("<Suspense fallback={null}>");
  });
});
