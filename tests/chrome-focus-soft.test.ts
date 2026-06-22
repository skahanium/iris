import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

const chromeFiles = [
  "src/components/ui/button.tsx",
  "src/components/layout/DesktopTitleBar.tsx",
  "src/components/layout/WindowControls.tsx",
  "src/components/layout/StatusBar.tsx",
  "src/components/layout/ConnectivityIndicators.tsx",
  "src/components/layout/StatusBarTokenUsage.tsx",
  "src/components/layout/EditorZoomControl.tsx",
  "src/components/editor/TipTapEditor.tsx",
  "src/components/ui/iris-overlay.tsx",
  "src/components/ui/dialog.tsx",
];

describe("chrome soft focus contract", () => {
  it("defines soft focus utilities for non-form chrome controls", () => {
    const css = read("src/styles/globals.css");

    expect(css).toContain(".iris-focus-soft:focus-visible");
    expect(css).toContain(".iris-focus-soft-within:focus-within");
    expect(css).toContain("--iris-focus-soft-bg");
    expect(css).toContain("--iris-focus-soft-halo");
    expect(css).toContain("--iris-focus-soft-line");
  });

  it("uses the soft focus class instead of coarse primary rings on chrome controls", () => {
    for (const file of chromeFiles) {
      const source = read(file);

      expect(source, file).toContain("iris-focus-soft");
      expect(source, file).not.toMatch(
        /focus(?:-visible|-within)?:ring-2[^"'\n]*ring-primary/,
      );
      expect(source, file).not.toMatch(
        /focus(?:-visible|-within)?:ring-primary[^"'\n]*ring-2/,
      );
    }
  });

  it("keeps form fields on a stronger visible focus treatment", () => {
    expect(read("src/components/ui/input.tsx")).toContain(
      "focus-visible:ring-2 focus-visible:ring-primary",
    );
    expect(read("src/components/ui/textarea.tsx")).toContain(
      "focus-visible:ring-2 focus-visible:ring-ring",
    );
    expect(read("src/components/ui/select.tsx")).toContain(
      "focus:ring-2 focus:ring-ring",
    );
  });
});
