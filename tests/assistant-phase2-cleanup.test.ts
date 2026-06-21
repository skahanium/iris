import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function readRequired(path: string): string {
  return readFileSync(path, "utf8");
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
      expect(existsSync(path)).toBe(false);
    }
  });

  it("routes scenes internally instead of exposing SceneSelector", () => {
    const policy = readRequired("src-tauri/src/ai_runtime/agent_task_policy.rs");
    const routing = readRequired("src/lib/assistant-routing.ts");
    const panel = readRequired(
      "src/components/ai/UnifiedAssistantPanel.impl.tsx",
    );
    const taskHook = readRequired(
      "src/components/ai/hooks/useAssistantTasks.ts",
    );
    const statusBadge = readRequired("src/components/ai/AgentStatusBadge.tsx");
    const connectivity = readRequired("src/hooks/useConnectivityStatus.ts");
    const historyDropdown = readRequired(
      "src/components/ai/SessionHistoryDropdown.tsx",
    );
    const skillsPanel = readRequired("src/components/ai/SkillsPanel.tsx");

    expect(policy).toContain("legacy_scene");
    expect(policy).toContain("compatibility");
    expect(routing).toContain("buildAssistantTaskPlan");
    expect(panel).not.toContain("ContextStatusBar");
    expect(panel).toContain("onChromeChange");
    expect(readRequired("src/components/ai/ContextPacketDrawer.tsx")).toContain(
      "证据",
    );
    expect(panel).not.toContain("SceneSelector");
    expect(panel).not.toContain("ExecutionPlanPreview");
    expect(taskHook).toContain("assistantExecute(");
    expect(taskHook).not.toContain("chapterWritingExecute");
    expect(taskHook).not.toContain("documentCheckExecute");
    expect(statusBadge).not.toContain('case "exemplar_learning"');
    expect(connectivity).not.toContain('stored === "exemplar_learning"');
    expect(statusBadge).not.toContain("场景");
    expect(historyDropdown).not.toContain("场景");
    expect(skillsPanel).not.toContain("场景");
  });

  it("uses a single ai-stream suggestion node in the editor", () => {
    const editor = readRequired("src/components/editor/TipTapEditor.tsx");
    expect(editor).toContain("AiStreamExtension");
    expect(editor).not.toContain("InlineAiExtension");
  });
});
