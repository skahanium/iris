import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI dual-domain routing contract", () => {
  describe("normal document switching does not leak AI note content", () => {
    it("useWorkspaceAssistantRouting uses deriveAiDomainState for domain-based gating", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      // deriveAiDomainState must be imported and used
      expect(src).toContain(
        'import { deriveAiDomainState } from "@/lib/ai-domain"',
      );
      expect(src).toContain("deriveAiDomainState(");
      // domain-based gating replaces old nonNoteSurfaceActive ternary pattern
      expect(src).toMatch(/isNormalDomain\s*&&\s*!\s*nonNoteSurfaceActive/);
    });

    it("useAppEditorActions blocks classified paths from reaching AI", () => {
      const src = read("src/hooks/useAppEditorActions.ts");
      expect(src).toContain("涉密笔记不能发送到 AI");
      expect(src).toContain("if (activeNoteIsClassified)");
      expect(src).toContain("isClassifiedVaultPath(path)");
    });

    it("App impl nulls out note path and content when classified note is active", () => {
      const app = read("src/App.impl.tsx");
      expect(app).toContain(
        "activeArtifactTab || activeNoteIsClassified ? null : activePath",
      );
      expect(app).toContain(
        'activeArtifactTab || activeNoteIsClassified ? "" : getLiveMarkdown()',
      );
    });
  });

  describe("active classified note with unlocked vault yields classified domain", () => {
    it("useWorkspaceAssistantRouting computes domain via deriveAiDomainState", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      expect(src).toContain("domainState.domain");
      expect(src).toContain("classifiedActivePath");
      // domain state is derived from the full set of inputs
      expect(src).toMatch(/deriveAiDomainState\(\{/);
      expect(src).toContain("activeNoteIsClassified");
      expect(src).toContain("classifiedUnlocked");
    });

    it("type definitions contain AiDomain union", () => {
      const aiTypes = read("src/types/ai.ts");
      expect(aiTypes).toContain("AiDomain");
      expect(aiTypes).toMatch(
        /type\s+AiDomain\s*=\s*["']normal["']\s*\|\s*["']classified["']/,
      );
    });
  });

  describe("switching from classified to normal clears classified runtime state", () => {
    it("App impl tracks activeNoteIsClassified as a reactive boolean", () => {
      const app = read("src/App.impl.tsx");
      expect(app).toContain("activeNoteIsClassified");
    });

    it("IPC types carry AiDomain and classified thread types", () => {
      const ipcTypes = read("src/types/ipc.ts");
      expect(ipcTypes).toContain("ClassifiedAiThread");
      expect(ipcTypes).toContain("ClassifiedAiMessage");
    });
  });

  describe("media/artifact tabs never inherit classified permissions", () => {
    it("useWorkspaceAssistantRouting gates note content for non-note surfaces via domain model", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      // assistantNotePath is nulled when activeMediaTab is truthy
      expect(src).toContain(
        "assistantNotePath: activeMediaTab ? null : assistantNotePathWithoutMedia",
      );
      // assistantSelectionQuote is nulled for any non-note surface
      expect(src).toContain(
        "assistantSelectionQuote: nonNoteSurfaceActive ? null : selectionQuote",
      );
    });

    it("insert-to-editor blocks artifact/media and defers classified to domain model", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      // artifact/media check comes before domain check
      const artifactBlock = src.indexOf(
        "if (activeArtifactTab || activeMediaTab)",
      );
      const classifiedDomainBlock = src.indexOf(
        'if (domainState.domain === "classified")',
      );
      expect(artifactBlock).toBeGreaterThan(0);
      expect(classifiedDomainBlock).toBeGreaterThan(artifactBlock);
    });

    it("useWorkspaceAssistantRouting returns aiDomain and classifiedPath", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      expect(src).toContain("aiDomain: domainState.domain");
      expect(src).toContain("classifiedPath: domainState.classifiedActivePath");
    });
  });

  describe("AppAiPanelSlot passes domain to UnifiedAssistantPanel", () => {
    it("AppAiPanelSlot accepts and forwards aiDomain and classifiedPath", () => {
      const slot = read("src/components/layout/AppAiPanelSlot.tsx");
      expect(slot).toContain("aiDomain: AiDomain");
      expect(slot).toContain("classifiedPath: string | null");
      expect(slot).toContain("aiDomain={aiDomain}");
      expect(slot).toContain("classifiedPath={classifiedPath}");
    });

    it("UnifiedAssistantPanelProps includes domain fields", () => {
      const types = read("src/components/ai/types.ts");
      expect(types).toContain("aiDomain?: AiDomain");
      expect(types).toContain("classifiedPath?: string | null");
    });
  });
});
