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

  it("research results appear in message timeline", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    expect(panel).toContain('kind: "research"');
    expect(panel).toContain("onExpandResearch");
    const list = read("src/components/ai/AiMessageList.tsx");
    expect(list).toContain("ResearchResultMessage");
    expect(read("src/components/ai/ResearchResultMessage.tsx")).toContain(
      'data-testid="research-result-message"',
    );
  });

  it("AI System Center hosts persona, rules, and model config", () => {
    const source = read("src/components/settings/AiSystemCenterPanel.tsx");
    expect(source).toContain("PersonaSettingsPanel");
    expect(source).toContain("AiRulesPanel");
    expect(source).toContain("MinimaxSearchSection");
    expect(read("src/components/settings/SettingsPanel.tsx")).not.toContain(
      "MinimaxSearchSection",
    );
  });
});
