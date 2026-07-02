import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function lineCount(path: string): number {
  return read(path).split("\n").length;
}

describe("App shell refactor contract", () => {
  it("keeps App.impl.tsx below the current shell split checkpoint", () => {
    // 990: current post-split shell checkpoint after moving AI sidecar and
    // persistence lifecycle out of App.impl while keeping overlay wiring local.
    expect(lineCount("src/App.impl.tsx")).toBeLessThanOrEqual(990);
  });

  it("moves AI sidecar bridge state behind a dedicated hook", () => {
    const hook = read("src/hooks/useAiSidecarBridge.ts");
    const app = read("src/App.impl.tsx");

    expect(hook).toContain("selectionQuote");
    expect(hook).toContain("prefillMessage");
    expect(hook).toContain("assistantChrome");
    expect(hook).toContain("webSearchEnabled");
    expect(app).toContain("useAiSidecarBridge");
    expect(app).not.toContain("const [selectionQuote");
    expect(app).not.toContain("const [assistantPrefill");
    expect(app).not.toContain("const [assistantChrome");
  });

  it("moves persistence lifecycle behind a dedicated hook", () => {
    const hook = read("src/hooks/useAppPersistenceLifecycle.ts");
    const app = read("src/App.impl.tsx");

    expect(hook).toContain("useEditorSave");
    expect(hook).toContain("persistActiveTabBeforeLeave");
    expect(hook).toContain("useTauriCloseSave");
    expect(hook).toContain("handleSaveVersion");
    expect(app).toContain("useAppPersistenceLifecycle");
    expect(app).not.toContain("const versionSnapshotScheduler = useMemo");
    expect(app).not.toContain("const flushAllOpenTabs = useCallback");
  });

  it("moves overlay composition behind a layout component", () => {
    const component = read("src/components/layout/AppOverlays.tsx");
    const app = read("src/App.impl.tsx");

    expect(component).not.toContain("CommandPalette");
    expect(component).toContain("QuickOpen");
    expect(component).toContain("ManagementCenterPanel");
    expect(component).toContain("VersionTimeline");
    expect(component).toContain("ClassifiedPanel");
    expect(app).toContain("<AppOverlays");
    expect(app).not.toContain("<CommandPalette");
    expect(app).not.toContain("<VersionTimeline");
  });

  it("does not mount closed lazy overlays and shows non-blank suspense fallbacks", () => {
    const component = read("src/components/layout/AppOverlays.tsx");

    expect(component).toContain("overlays.managementCenterOpen ? (");
    expect(component).toContain("overlays.versionOpen ? (");
    expect(component).toContain("overlays.graphOpen ? (");
    expect(component).toContain("OverlayLoadingSurface");
    expect(component).toContain("fallback={<OverlayLoadingSurface");
    expect(component).not.toContain("fallback={null}");
    expect(component).not.toContain("LazyFallback");
  });
  it("moves editor workspace composition behind a layout component", () => {
    const component = read("src/components/layout/AppEditorWorkspace.tsx");
    const app = read("src/App.impl.tsx");

    expect(component).toContain("TipTapEditor");
    expect(component).toContain("EditorOutline");
    expect(component).toContain("EditorFindReplaceBar");
    expect(component).toContain("WelcomeEmpty");
    expect(app).toContain("<AppEditorWorkspace");
    expect(app).not.toContain("<TipTapEditor");
  });

  it("moves AI panel composition behind a layout component", () => {
    const component = read("src/components/layout/AppAiPanelSlot.tsx");
    const app = read("src/App.impl.tsx");

    expect(component).toContain("UnifiedAssistantPanel");
    expect(component).toContain("onPatchApplied");
    expect(app).toContain("<AppAiPanelSlot");
    expect(app).not.toContain("<UnifiedAssistantPanel");
  });
});
