import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { E2E_SELECTORS, intentForFlow } from "./helpers";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("unified assistant E2E contract", () => {
  it("exposes stable data-testid hooks for future Tauri/Playwright drivers", () => {
    const app = read("src/App.tsx");
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");
    const conversation = read("src/components/ai/ConversationSurface.tsx");
    const shell = read("src/components/layout/AppShell.tsx");

    expect(app).toContain('data-testid="editor-shell"');
    expect(shell).toContain('data-testid="unified-assistant-dock"');
    expect(panel).toContain('data-testid="unified-assistant-panel"');
    expect(panel).toContain('data-testid="ai-input"');
    expect(conversation).toContain('data-testid="ai-message-list"');
    expect(panel).toContain("AssistantTaskSurfaces");
    expect(panel).not.toContain('data-testid="research-focus"');
    expect(panel).not.toContain("ExecutionPlanPreview");
    expect(panel).toContain("usePromptProfile");
    expect(panel).toContain("AssistantPersonaDisplay");
    expect(panel).toContain("AgentStatusBadge");
    expect(read("src/components/ai/AgentStatusBadge.tsx")).toContain(
      'data-testid="agent-status-trigger"',
    );
    expect(read("src/components/settings/ManagementCenterPanel.tsx")).toContain(
      'data-testid="management-center"',
    );
    expect(panel).not.toContain("AssistantIdentitySection");
    expect(panel).not.toContain("AgentStatusStrip");
  });

  it("maps acceptance flows to assistant intents without SceneSelector", () => {
    expect(intentForFlow("selection_rewrite")).toBe("writing");
    expect(intentForFlow("mention_scope_lookup")).toBe("knowledge");
    expect(intentForFlow("citation_check")).toBe("citation");
    expect(intentForFlow("research_focus")).toBe("research");
    expect(intentForFlow("web_knowledge_chat")).toBe("knowledge");
  });

  it("does not reference removed workflow entry UI", () => {
    const helpers = read("tests/e2e/helpers.ts");
    expect(helpers).not.toContain("scene-selector");
    expect(helpers).not.toContain("knowledge-lookup");
    expect(E2E_SELECTORS.assistantPanel).toBe(
      '[data-testid="unified-assistant-panel"]',
    );
  });

  it("wires Ctrl+Shift+A toggle to the unified assistant dock", () => {
    const keyboard = read("src/hooks/useAppKeyboard.ts");
    expect(keyboard).toContain("matchesKeyChord");
    const items = read("src/lib/command-palette.ts");
    expect(items).toContain('key: "A"');
    expect(items).toContain("toggleAiPanel");
    expect(read("src/App.tsx")).toContain("aiPanelOpen");
  });
});
