import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

const REMOVED_ENTRY_PANELS = [
  "AiWorkflowPanel.tsx",
  "AiPanel.tsx",
  "AiPanelHeader.tsx",
  "SceneSelector.tsx",
  "WritingTaskPanel.tsx",
  "CitationTaskPanel.tsx",
  "ChapterDocumentPanel.tsx",
  "OrganizePanel.tsx",
  "ResearchPanel.tsx",
  "InlineAiExtension.ts",
  "InlineAiNodeView.tsx",
] as const;

describe("assistant phase 2 cleanup", () => {
  it("removes legacy workflow entry panels from the tree", () => {
    for (const file of REMOVED_ENTRY_PANELS) {
      const path = file.includes("InlineAiExtension")
        ? `src/components/editor/extensions/${file}`
        : file.includes("InlineAiNodeView")
          ? `src/components/editor/${file}`
          : `src/components/ai/${file}`;
      expect(read(path)).toBe("");
    }
  });

  it("routes scenes internally instead of exposing SceneSelector", () => {
    const routing = read("src/lib/assistant-scene.ts");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");

    expect(routing).toContain("resolveAiSceneForIntent");
    expect(panel).toContain("ContextStatusBar");
    expect(panel).not.toContain("SceneSelector");
    expect(panel).toContain("ResearchFocusView");
    expect(panel).toContain("ExecutionPlanPreview");
    expect(panel).toContain("assistantExecute(");
    expect(panel).not.toContain("chapterWritingExecute");
    expect(panel).not.toContain("documentCheckExecute");
  });

  it("uses a single ai-stream suggestion node in the editor", () => {
    const editor = read("src/components/editor/TipTapEditor.tsx");
    expect(editor).toContain("AiStreamExtension");
    expect(editor).not.toContain("InlineAiExtension");
  });
});
