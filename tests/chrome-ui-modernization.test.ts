import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("v0.4.1-ui chrome modernization", () => {
  it("AiPanel uses shared scene data and subcomponents", () => {
    const source = read("src/components/ai/AiPanel.tsx");
    expect(source).toContain("AiPanelHeader");
    expect(source).toContain("AiComposer");
    expect(source).toContain("AiMessageList");
    expect(source).toContain("ContextPacketDrawer");
    expect(source).not.toContain("SCENE_OPTIONS");
    expect(source).not.toMatch(/const SCENE_OPTIONS/);
  });

  it("SlashCommandList uses Lucide via CommandListOption", () => {
    const source = read("src/components/editor/SlashCommandList.tsx");
    expect(source).toContain("CommandListOption");
    expect(source).toContain("resolveCommandIcon");
    expect(source).toContain("useListboxKeyboard");
    expect(source).not.toContain("📄");
    expect(source).not.toContain("💡");
  });

  it("ResearchPanel and WorkflowIndicator avoid high-saturation colors", () => {
    const research = read("src/components/ai/ResearchPanel.tsx");
    expect(research).not.toContain("emerald-");
    expect(research).not.toContain("purple-");
    expect(research).not.toContain("amber-");

    const workflow = read("src/components/ai/WorkflowIndicator.tsx");
    expect(workflow).not.toContain("emerald-");
    expect(workflow).toContain("bg-primary/80");
  });

  it("ContextPacketCard avoids high-saturation trust colors", () => {
    const source = read("src/components/ai/ContextPacketCard.tsx");
    expect(source).toContain("SurfaceCard");
    expect(source).not.toContain("emerald-500");
    expect(source).not.toContain("purple-500");
  });

  it("StatusBar uses EditorZoomControl popover", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    expect(source).toContain("EditorZoomControl");
    expect(source).not.toContain('aria-label="缩小"');
  });
});
