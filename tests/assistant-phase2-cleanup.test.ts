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
    const panelImpl = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const statusBadge = read("src/components/ai/AgentStatusBadge.tsx");
    const connectivity = read("src/hooks/useConnectivityStatus.ts");
    const header = read("src/components/ai/AssistantPanelHeader.tsx");
    const historyDropdown = read(
      "src/components/ai/SessionHistoryDropdown.tsx",
    );
    const skillsPanel = read("src/components/ai/SkillsPanel.tsx");

    expect(routing).toContain("legacySceneHintForAssistantIntent");
    expect(routing).not.toContain("resolveAiSceneForIntent");
    expect(panel).not.toContain("ContextStatusBar");
    expect(panel).toContain("onChromeChange");
    expect(read("src/components/ai/ContextPacketDrawer.tsx")).toContain("证据");
    expect(panel).not.toContain("SceneSelector");
    expect(panel).toContain("ResearchFocusView");
    expect(panel).not.toContain("ExecutionPlanPreview");
    expect(panel).toContain("assistantExecute(");
    expect(panelImpl).toContain("legacySceneHintForAssistantIntent");
    expect(header).toContain("legacySceneHint");
    expect(header).not.toContain("activeScene");
    expect(panel).not.toContain("chapterWritingExecute");
    expect(panel).not.toContain("documentCheckExecute");
    expect(statusBadge).not.toContain('case "exemplar_learning"');
    expect(connectivity).not.toContain('stored === "exemplar_learning"');
    expect(statusBadge).not.toContain("场景");
    expect(historyDropdown).not.toContain("场景");
    expect(skillsPanel).not.toContain("场景");
  });

  it("uses a single ai-stream suggestion node in the editor", () => {
    const editor = read("src/components/editor/TipTapEditor.tsx");
    expect(editor).toContain("AiStreamExtension");
    expect(editor).not.toContain("InlineAiExtension");
  });
});
