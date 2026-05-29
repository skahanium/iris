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
    expect(source).toContain("UnifiedAssistantPanel");
    expect(source).not.toContain("AiWorkflowPanel");
  });

  it("unified assistant panel removes workflow tabs and rules center from the main entry", () => {
    const source = read("src/components/ai/UnifiedAssistantPanel.tsx");

    expect(source).toContain("AssistantActionState");
    expect(source).toContain("AssistantIntent");
    expect(source).toContain("useAssistantIdentity");
    expect(source).toContain("AssistantAvatar");
    expect(source).not.toContain("WORKFLOW_TASK_DEFINITIONS");
    expect(source).not.toContain("未绑定文档");
    expect(source).not.toContain("AiRulesPanel");
    expect(source).not.toContain("SceneSelector");
    expect(source).not.toContain("AssistantIdentitySection");
    expect(source).not.toContain("setIdentity");
  });

  it("floating toolbar routes inline edits through the editor stream hook", () => {
    const source = read("src/components/editor/FloatingToolbar.tsx");

    expect(source).not.toContain("insertInlineAi");
    expect(source).toContain("onInlineAi(action)");
    expect(source).toContain("prefill");
  });

  it("settings panel hosts identity, rules, and model config", () => {
    const source = read("src/components/settings/SettingsPanel.tsx");

    expect(source).toContain("settings-section-ai-assistant");
    expect(source).toContain("AssistantIdentitySection");
    expect(source).toContain("AiRulesPanel");
    expect(source).toContain("MinimaxSearchSection");
    expect(source).not.toContain("UnifiedAssistantPanel");
  });
});
