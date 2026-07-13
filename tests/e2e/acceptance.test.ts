/** Source-level acceptance selectors for the Tauri/Playwright suite. */
import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("Iris core acceptance", () => {
  it("keeps primary editor, title bar and status bar selectors", () => {
    expect(read("src/components/editor/TipTapEditor.tsx")).toContain(
      'data-testid="editor"',
    );
    expect(read("src/components/layout/DesktopTitleBar.tsx")).toContain(
      'data-testid="desktop-title-bar"',
    );
    expect(read("src/components/layout/StatusBar.tsx")).toContain(
      'data-testid="status-bar"',
    );
  });

  it("exposes a single unified assistant panel backed by Run events", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const runHook = read("src/hooks/useAssistantRun.ts");

    expect(panel).toContain('data-testid="unified-assistant-panel"');
    expect(panel).toContain("useAssistantRun()");
    expect(panel).not.toContain('useAssistantRun("chat")');
    expect(runHook).toContain("assistantRunStart");
    expect(runHook).toContain("listenAssistantRunEvent");
  });

  it("keeps AI rules in Management Center, not the conversation panel", () => {
    const managementCenter = read(
      "src/components/settings/ManagementCenterPanel.tsx",
    );
    const panel = read("src/components/ai/UnifiedAssistantPanel.tsx");

    expect(managementCenter).toContain("AiRulesPanel");
    expect(managementCenter).toContain("memory:");
    expect(panel).not.toContain("AiRulesPanel");
  });
});
