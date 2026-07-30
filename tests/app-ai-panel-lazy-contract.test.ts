import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("assistant panel lazy loading contract", () => {
  it("keeps the assistant panel out of the eager application chunk", () => {
    const slot = read("src/components/layout/AppAiPanelSlot.tsx");

    expect(slot).not.toContain(
      'import { UnifiedAssistantPanel } from "@/components/ai/UnifiedAssistantPanel"',
    );
    expect(slot).toContain("lazy(() =>");
    expect(slot).toContain('import("@/components/ai/UnifiedAssistantPanel")');
    expect(slot).toContain("<Suspense fallback={<AssistantPanelLoading />}");
  });

  it("preloads the lazy panel while the vault gate is visible", () => {
    const slot = read("src/components/layout/AppAiPanelSlot.tsx");
    const app = read("src/App.impl.tsx");
    const preload = read("src/lib/preload-assistant-panel.ts");

    expect(preload).toContain("export function preloadAssistantPanel");
    expect(preload).toContain(
      'import("@/components/ai/UnifiedAssistantPanel")',
    );
    expect(app).toContain("preloadAssistantPanel");
    expect(app).toContain("scheduleAssistantPanelPreload");
    expect(slot).toContain('import("@/components/ai/UnifiedAssistantPanel")');
  });
});
