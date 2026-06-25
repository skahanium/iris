import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified note-open preparation contract", () => {
  it("prepares classified notes only through explicit classified permission", () => {
    const opener = read("src/hooks/usePreparedNoteOpener.ts");
    const app = read("src/App.impl.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const panel = read("src/components/classified/ClassifiedPanel.tsx");
    const fileList = read("src/components/classified/ClassifiedFileList.tsx");

    expect(opener).toContain("prepareClassifiedNotePath");
    expect(opener).toContain("allowClassified: true");
    expect(app).toContain("prepareClassifiedNotePath");
    expect(app).toContain(
      "onPrepareClassifiedNotePath={prepareClassifiedNotePath}",
    );
    expect(overlays).toContain(
      "onPrepareClassifiedNotePath?.(path, titleHint,",
    );
    expect(overlays).toContain('source: "classified"');
    expect(panel).toContain("onPrepareFile");
    expect(panel).toMatch(/await onOpenFile\(path\)/);
    expect(fileList).toContain("onPrepareFile");
    expect(fileList).toMatch(
      /onMouseEnter=\{\(\) =>[\s\S]*!entry\.isDir &&[\s\S]*onPrepareFile\?\.\(entry\.path/,
    );
    expect(fileList).toMatch(
      /onFocus=\{\(\) =>[\s\S]*!entry\.isDir &&[\s\S]*onPrepareFile\?\.\(entry\.path/,
    );
  });
});
