import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("AI dual-domain routing contract", () => {
  describe("normal document switching does not leak AI note content", () => {
    it("useWorkspaceAssistantRouting does not pass note content when classified", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      // nonNoteSurfaceActive must gate all note-derived context
      expect(src).toContain("nonNoteSurfaceActive");
      expect(src).toMatch(
        /nonNoteSurfaceActive\s*\?\s*""\s*:\s*getLiveMarkdown\(\)/,
      );
      expect(src).toMatch(
        /nonNoteSurfaceActive\s*\?\s*null\s*:\s*getWritingContext\(\)/,
      );
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
    it("useWorkspaceAssistantRouting identifies classified surface as non-note", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      expect(src).toContain("activeNoteIsClassified");
      // classified is grouped with artifact/media as non-note surfaces
      expect(src).toMatch(
        /activeArtifactTab\s*\|\|\s*activeMediaTab\s*\|\|\s*activeNoteIsClassified/,
      );
    });

    it("type definitions do not yet contain AiDomain union", () => {
      const aiTypes = read("src/types/ai.ts");
      // This assertion LOCKS that AiDomain does not exist yet.
      // When the feature is implemented, this test must be updated to assert
      // the opposite: that `type AiDomain = "normal" | "classified"` exists.
      expect(aiTypes).not.toContain("AiDomain");
    });
  });

  describe("switching from classified to normal clears classified runtime state", () => {
    it("App impl tracks activeNoteIsClassified as a reactive boolean", () => {
      const app = read("src/App.impl.tsx");
      expect(app).toContain("activeNoteIsClassified");
    });

    it("IPC types do not yet carry a domain field on AI messages", () => {
      const ipcTypes = read("src/types/ipc.ts");
      // domain currently only means web URL domain in IPC types
      // This locks that there is no AI domain routing in IPC yet.
      const aiSections = ipcTypes.split("domain");
      // The word "domain" appears only in non-AI contexts currently
      for (const section of aiSections) {
        expect(section).not.toMatch(/classified/);
      }
    });
  });

  describe("media/artifact tabs never inherit classified permissions", () => {
    it("useWorkspaceAssistantRouting treats media tabs as non-note regardless of classified state", () => {
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

    it("insert-to-editor blocks classified but also blocks artifact/media independently", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      // artifact/media check comes before classified check
      const artifactBlock = src.indexOf(
        "if (activeArtifactTab || activeMediaTab)",
      );
      const classifiedBlock = src.indexOf("if (activeNoteIsClassified)");
      expect(artifactBlock).toBeGreaterThan(0);
      expect(classifiedBlock).toBeGreaterThan(artifactBlock);
    });
  });
});
