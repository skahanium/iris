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

    it("useAppEditorActions does not blanket-block classified editor AI actions", () => {
      const src = read("src/hooks/useAppEditorActions.ts");
      expect(src).toContain("void inlineAi.run(ed, action)");
      expect(src).not.toContain("涉密笔记不能发送到 AI");
      expect(src).not.toContain("isClassifiedVaultPath(path)");
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
      // assistantNotePath is nulled for media/artifact surfaces and normal chat
      // only receives the active note when explicit note context exists.
      expect(src).toContain("activeMediaTab || activeArtifactTab");
      expect(src).toContain("hasExplicitNoteContext");
      expect(src).toMatch(
        /hasExplicitNoteContext[\s\S]*assistantNotePathWithoutMedia/,
      );
      // assistantSelectionQuote is nulled for any non-note surface
      expect(src).toContain(
        "assistantSelectionQuote: nonNoteSurfaceActive ? null : selectionQuote",
      );
    });

    it("insert-to-editor blocks artifact/media but allows classified editor domain", () => {
      const src = read("src/hooks/useWorkspaceAssistantRouting.ts");
      const artifactBlock = src.indexOf(
        "if (activeArtifactTab || activeMediaTab)",
      );
      expect(artifactBlock).toBeGreaterThan(0);
      const insertBlock =
        src
          .split("const handleAssistantInsertToEditor")[1]
          ?.split("return {")[0] ?? "";
      expect(insertBlock).toContain("handleInsertToEditor(content)");
      expect(insertBlock).not.toContain('domainState.domain === "classified"');
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

    it("assistant execution requests carry the active domain and strip normal context in classified mode", () => {
      const hook = read("src/components/ai/hooks/useAssistantTasks.ts");
      const aiTypes = read("src/types/ai.ts");
      const backend = read("src-tauri/src/commands/assistant_commands.rs");
      const harness = read("src-tauri/src/ai_harness/harness_task.rs");

      expect(aiTypes).toContain('aiDomain?: "normal" | "classified"');
      expect(hook).toContain("aiDomain,");
      expect(hook).toContain('aiDomain === "classified" ? []');
      expect(hook).toContain('if (aiDomain === "classified") return null');
      expect(backend).toContain("pub ai_domain: AssistantAiDomain");
      expect(backend).toContain("validate_assistant_domain_boundary");
      expect(harness).toContain("run_classified_chat_task");
      expect(harness).not.toContain(
        "validate_ai_note_path(request.note_path.as_deref())?;",
      );
    });

    it("classified assistant panel domain has visible chrome styles", () => {
      const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
      const css = read("src/styles/globals.css");

      expect(panel).toContain("data-ai-domain={aiDomain}");
      expect(css).toContain('[data-ai-domain="classified"]');
      expect(css).toContain("--classified-accent");
      expect(css).toContain("--ai-domain-accent");
      expect(css).toContain("--ai-domain-ring");
      expect(css).not.toContain("--ai-domain-accent: hsl(var(--warning))");
      expect(css).toMatch(
        /--ai-domain-ring:\s*hsl\(var\(--classified-accent\) \/ 0\.18\)/,
      );
    });
  });
});
