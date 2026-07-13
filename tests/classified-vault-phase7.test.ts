import { existsSync, readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import { buildCommandPaletteItems } from "@/lib/command-palette";
import {
  isClassifiedVaultPath,
  vaultRelativePath,
} from "@/lib/classified-path";
import {
  filterEditorActions,
  isEditorActionEnabled,
} from "@/lib/editor-actions";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("classified vault and Run boundary", () => {
  it("registers the classified panel shortcut outside the palette list", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const classified = items.find((item) => item.id === "classified-panel");

    expect(classified?.hiddenInPalette).toBe(true);
    expect(classified?.chord).toMatchObject({
      key: "L",
      mod: true,
      shift: true,
    });
  });

  it("detects and normalizes vault-relative classified paths", () => {
    expect(isClassifiedVaultPath(".classified/secret.md")).toBe(true);
    expect(isClassifiedVaultPath("notes/open.md")).toBe(false);
    expect(vaultRelativePath("/vault", "/vault/notes/a.md")).toBe("notes/a.md");
    expect(vaultRelativePath("/vault", "/other/a.md")).toBeNull();
  });

  it("keeps mutating editor actions disabled while a note is locked", () => {
    const context = {
      hasNote: true,
      hasSelection: true,
      streaming: false,
      isLocked: true,
    };
    const paste = filterEditorActions("context_menu", "editor", {
      ...context,
      isLocked: false,
    }).find((action) => action.id === "paste");
    const copy = filterEditorActions("context_menu", "editor", {
      ...context,
      isLocked: false,
    }).find((action) => action.id === "copy");

    expect(isEditorActionEnabled(paste!, context)).toBe(false);
    expect(isEditorActionEnabled(copy!, context)).toBe(true);
  });

  it("keeps classified file operations and lock UI available", () => {
    const list = read("src/components/classified/ClassifiedFileList.tsx");
    const editor = read("src/components/editor/TipTapEditor.tsx");

    expect(list).toContain("classifiedImport");
    expect(list).toContain("classifiedExport");
    expect(list).toContain("classifiedDelete");
    expect(editor).toContain("locked?: boolean");
    expect(editor).toContain('data-testid="editor-lock-toggle"');
  });

  it("routes classified assistant work through the same opaque Run protocol", () => {
    const panel = read("src/components/ai/UnifiedAssistantPanel.impl.tsx");
    const sender = read("src/components/ai/hooks/useUnifiedAssistantSend.ts");
    const inline = read("src/hooks/useInlineAi.ts");

    expect(panel).toContain("data-ai-domain={aiDomain}");
    expect(sender).toContain("securityDomain: aiDomain");
    expect(sender).toContain("explicitReferences");
    expect(inline).toContain("assistantRunStart");
    expect(inline).toContain("securityDomain: domain");
    expect(existsSync("src/components/ai/hooks/useAssistantTasks.ts")).toBe(
      false,
    );
  });
});
