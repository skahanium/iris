import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("management center contract", () => {
  it("uses top tabs for four management sections without the old sidebar", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const app = read("src/App.impl.tsx");

    expect(center).toContain('data-testid="management-center"');
    expect(center).toContain('data-testid="management-center-tabs"');
    expect(center).toContain("grid-cols-4");
    expect(center).toContain("w-full");
    expect(center).not.toContain('data-testid="management-center-nav"');
    for (const id of [
      'id: "overview"',
      'id: "notes"',
      'id: "knowledge"',
      'id: "ai"',
    ]) {
      expect(center).toContain(id);
    }
    expect(center).not.toContain('id: "workspace"');
    expect(center).not.toContain('id: "security"');
    expect(center).not.toContain('{ id: "about"');
    expect(center).toContain("ManagementCenterSection");
    expect(center).toContain("LlmRoutingSection");
    expect(center).toContain("MinimaxSearchSection");
    expect(center).toContain("PersonaSettingsBody");
    expect(center).toContain("SkillsPanelBody");
    expect(center).toContain("AiRulesPanel");
    expect(center).not.toContain('data-testid="ai-system-center-nav"');
    expect(overlays).toContain("ManagementCenterPanel");
    expect(overlays).not.toContain("SettingsPanel");
    expect(overlays).not.toContain("AiSystemCenterPanel");
    expect(app).toContain('openManagementCenter("ai")');
  });

  it("keeps knowledge management inline and moves graph to the status bar", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const statusBar = read("src/components/layout/StatusBar.tsx");
    const statusSlot = read("src/components/layout/AppStatusBarSlot.tsx");

    for (const prop of [
      "onOpenKnowledgeRelations",
      "onOpenVersion",
      "onRescanVault",
    ]) {
      expect(center, prop).toContain(prop);
      expect(overlays, prop).toContain(prop);
    }

    expect(center).toContain("VaultNavigatorBody");
    expect(center).toContain("RecycleBinBody");
    expect(center).not.toContain("renderTaskDetail");
    expect(center).not.toContain("openDetail");
    expect(statusBar).toContain('data-testid="status-bar-graph-button"');
    expect(statusSlot).toContain("onOpenGraph");
    expect(center).not.toContain("openClassified");
    expect(center).not.toContain("onOpenClassifiedPanel");
  });

  it("exposes automatic version tracking as real settings in the notes area", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const app = read("src/App.impl.tsx");
    const lifecycle = read("src/hooks/useAppPersistenceLifecycle.ts");
    const idle = read("src/hooks/useVersionIdle.ts");

    expect(center).toContain("autoVersionEnabled");
    expect(center).toContain("autoVersionIdleMinutes");
    expect(app).toContain("useAutoVersionSettings");
    expect(lifecycle).toContain("autoVersionEnabled");
    expect(lifecycle).toContain("autoVersionIdleMinutes");
    expect(idle).toContain("enabled");
    expect(idle).toContain("idleMs");
  });

  it("allows AI-only drilldown while keeping management sections scrollable", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(center).toContain('data-testid="management-section-ai"');
    expect(center).toContain('data-testid="management-ai-detail"');
    expect(center).toContain('data-testid="management-detail-back"');
    expect(center).toContain("overflow-y-auto");
    expect(center).toContain("LlmRoutingSection");
    expect(center).toContain("MinimaxSearchSection");
    expect(center).toContain("PersonaSettingsBody");
    expect(center).toContain("SkillsPanelBody");
    expect(center).toContain("AiRulesPanel");
    expect(center).toContain("renderAiDetail");
    expect(center).toContain("openAiDetail");
  });

  it("anchors management switches so the thumb stays inside the track", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(center).toContain('role="switch"');
    expect(center).toContain("left-1 top-1");
    expect(center).toContain("translate-x-5");
    expect(center).not.toContain("translate-x-6");
  });

  it("presents system and about information in overview without classified or fake button affordances", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");

    expect(center).toContain("GNU Affero General Public License v3.0");
    expect(center).not.toContain("openClassified");
    expect(center).not.toContain("LockKeyhole");
    expect(center).not.toContain("secret.read_plaintext");
  });

  it("prepares notes from the embedded file tree before opening", () => {
    const center = read("src/components/settings/ManagementCenterPanel.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");

    expect(center).toContain("onPrepareNote");
    expect(center).toContain("onPrepare={onPrepareNote}");
    expect(overlays).toContain("onPrepareNote={onPrepareNote}");
  });

  it("waits for restored notes to open before closing recycle views", () => {
    const recycle = read("src/components/file/RecycleBinSheet.tsx");
    const restoreIndex = recycle.indexOf("await onRestored(path)");
    const closeIndex = recycle.indexOf("onClose();", restoreIndex);

    expect(restoreIndex).toBeGreaterThanOrEqual(0);
    expect(closeIndex).toBeGreaterThan(restoreIndex);
  });
});
