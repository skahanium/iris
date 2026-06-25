import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("zen mode regression contracts", () => {
  it("does not reserve outline space while zen mode is active", () => {
    const workspace = read("src/components/layout/AppEditorWorkspace.tsx");

    expect(workspace).toContain(
      'outlineOpen && !zen && effectiveNotePath && "iris-editor-outline-open"',
    );
  });

  it("wires the global Escape handler into the app shell", () => {
    const app = read("src/App.impl.tsx");

    expect(app).toContain("useZenExitKeyboard");
    expect(app).toContain("useZenExitKeyboard({ zen, setZen })");
  });
});
