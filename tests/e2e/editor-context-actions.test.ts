import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor slash and context menu acceptance", () => {
  it("wires registry-driven slash and Iris-only context menu", () => {
    const app = read("src/App.tsx");
    const palette = read("src/lib/command-palette.ts");

    expect(app).toContain("runEditorAction");
    expect(app).not.toContain("FloatingToolbar");
    expect(app).toContain("IrisContextMenu");
    expect(app).toContain("useEditorContextMenu");
    expect(palette).not.toContain("slashPaletteItems");
    expect(read("src/lib/editor-actions.ts")).toContain("EDITOR_ACTIONS");
    expect(read("src/components/ui/iris-surface-menu.tsx")).toContain(
      "IrisSurfaceMenuItem",
    );
  });

  it("executes slash and selection actions through the same Run protocol", () => {
    const inline = read("src/hooks/useInlineAi.ts");

    expect(inline).toContain("assistantRunStart");
    expect(inline).toContain("assistantRunControl");
    expect(inline).toContain("listenAssistantRunEvent");
    expect(inline).toContain("buildInlineSelectionReference");
    expect(inline).toContain("explicitReferences");
    expect(inline).toContain("explicitAction");
    expect(inline).not.toContain("assistantExecute");
    expect(inline).not.toContain("aiSendMessage");
  });
});
