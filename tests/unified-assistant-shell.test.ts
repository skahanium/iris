import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  try {
    return readFileSync(path, "utf8");
  } catch {
    return "";
  }
}

describe("unified assistant shell", () => {
  it("App uses a single window chrome and the unified assistant panel", () => {
    const source = read("src/App.tsx");

    const chromeMatches = source.match(/<MinimalWindowChrome\s*\/>/g) ?? [];
    expect(chromeMatches).toHaveLength(1);
    expect(read("src/components/layout/DesktopTitleBar.tsx")).toContain(
      'data-testid="desktop-title-bar"',
    );
    expect(source).toContain("UnifiedAssistantPanel");
    expect(source).not.toContain("AiWorkflowPanel");
  });

  it("unified assistant panel removes workflow tabs and rules center from the main entry", () => {
    const source = read("src/components/ai/UnifiedAssistantPanel.tsx");

    expect(source).toContain("AssistantActionState");
    expect(source).toContain("AssistantIntent");
    expect(source).toContain("usePromptProfile");
    expect(source).toContain("AssistantPersonaDisplay");
    expect(source).toContain("AgentStatusBadge");
    expect(source).not.toContain("AgentStatusStrip");
    expect(source).not.toContain("WORKFLOW_TASK_DEFINITIONS");
    expect(source).not.toContain("未绑定文档");
    expect(source).not.toContain("AiRulesPanel");
    expect(source).not.toContain("SceneSelector");
    expect(source).not.toContain("AssistantIdentitySection");
    expect(source).not.toContain("setIdentity");
  });

  it("editor context menu routes selection AI through runEditorAction", () => {
    expect(read("src/App.tsx")).not.toContain("FloatingToolbar");
    expect(read("src/lib/editor-actions.ts")).toContain("send_prefill");
    expect(read("src/lib/editor-action-executor.ts")).toContain("onInlineAi");
    expect(read("src/hooks/useEditorContextMenu.ts")).toContain("context_menu");
  });

  it("research results stay in the normal markdown message timeline", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const list = read("src/components/ai/AiMessageList.tsx");
    const tasks = read("src/components/ai/hooks/useAssistantTasks.ts");
    expect(panel).toContain("AssistantTaskSurfaces");
    expect(panel).toContain("onOpenArtifact");
    expect(list).toContain("AiMessageBubble");
    expect(list).not.toContain("ResearchResultMessage");
    expect(tasks).toContain("result.summary.trim()");
    expect(tasks).not.toContain('kind: "research"');
  });

  it("opens assistant artifacts in readonly workspace tabs instead of sidecar cards", () => {
    const app = read("src/App.impl.tsx");
    const workspace = read("src/components/layout/AppEditorWorkspace.tsx");
    const titleBar = read("src/components/layout/DesktopTitleBar.tsx");
    const aiSlot = read("src/components/layout/AppAiPanelSlot.tsx");
    const assistantTypes = read("src/components/ai/types.ts");

    expect(app).toContain("useArtifactTabs");
    expect(app).toContain("onOpenArtifact");
    expect(app).toContain("activeArtifactTab");
    expect(workspace).toContain("ArtifactWorkspaceView");
    expect(workspace).toContain("activeArtifactTab");
    expect(titleBar).toContain('kind === "artifact"');
    expect(titleBar).toContain("isArtifact");
    expect(aiSlot).toContain("onOpenArtifact");
    expect(assistantTypes).toContain("onOpenArtifact?");
    expect(workspace).not.toContain("onRegenerate={() => undefined}");
    expect(workspace).not.toContain('title="重新生成"');
  });

  it("does not expose the tool audit drawer as a normal user surface", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const processPanel = read("src/components/ai/AgentTaskStatusPanel.tsx");
    const statusBadge = read("src/components/ai/AgentStatusBadge.tsx");

    expect(panel).not.toContain("AuditTrailDrawer");
    expect(statusBadge).not.toContain("auditAvailable");
    expect(statusBadge).not.toContain("onOpenAudit");
    expect(processPanel).not.toContain("查看审计");
    expect(read("src/components/layout/StatusBar.tsx")).not.toContain(
      "工具审计",
    );
    expect(read("src/lib/audit-trail-events.ts")).toBe("");
    expect(processPanel).not.toContain("onOpenAudit");
  });

  it("Management Center hosts persona, rules, and model config", () => {
    const source = read("src/components/settings/ManagementCenterPanel.tsx");
    expect(source).toContain("PersonaSettingsBody");
    expect(source).toContain("AiRulesPanel");
    expect(source).not.toContain("MinimaxSearchSection");
    expect(source).toContain('id: "ai"');
  });
  it("contains local error boundaries around volatile AI panel surfaces", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");

    expect(panel).toContain('scope="AI任务区"');
    expect(panel).toContain('scope="AI对话区"');
    expect(panel).toContain('scope="AI任务状态"');
    expect(panel.indexOf('scope="AI任务区"')).toBeLessThan(
      panel.indexOf("<AssistantTaskSurfaces"),
    );
    expect(panel.indexOf('scope="AI对话区"')).toBeLessThan(
      panel.indexOf("<ConversationSurface"),
    );
  });
});
