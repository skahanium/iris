import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor slash and context menu E2E contract", () => {
  it("wires registry-driven slash and iris-only context menu", () => {
    const app = read("src/App.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const conversation = read("src/components/ai/ConversationSurface.tsx");
    const palette = read("src/lib/command-palette.ts");

    expect(app).toContain("runEditorAction");
    expect(app).not.toContain("FloatingToolbar");
    expect(app).toContain("IrisContextMenu");
    expect(app).toContain("onBodyContextMenu");
    expect(app).toContain("useEditorContextMenu");
    expect(palette).not.toContain("slashPaletteItems");
    expect(conversation).toContain("AiMessageSelectionUi");
    expect(panel).toContain("AiComposerContextMenu");
    expect(read("src/lib/editor-actions.ts")).not.toContain(
      "selection_toolbar",
    );
    expect(read("src/lib/editor-actions.ts")).toContain("EDITOR_ACTIONS");
    expect(read("src/components/ui/iris-surface-menu.tsx")).toContain(
      "IrisSurfaceMenuItem",
    );
    expect(read("src/components/editor/SlashCommandList.tsx")).not.toContain(
      "CommandListOption",
    );
    expect(read("src/lib/iris-clipboard.ts")).toContain("pasteIntoEditor");
    expect(
      read("src/components/editor/DocumentTitleContextMenu.tsx"),
    ).toContain("IrisContextMenu");
  });
});
