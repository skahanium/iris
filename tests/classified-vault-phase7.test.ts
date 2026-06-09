import { readFileSync } from "node:fs";

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

describe("classified vault phase 7", () => {
  it("registers Cmd+Shift+L classified panel shortcut hidden from palette list", () => {
    const items = buildCommandPaletteItems({
      hasVault: true,
      hasActiveNote: false,
    });
    const classified = items.find((i) => i.id === "classified-panel");
    expect(classified).toBeDefined();
    expect(classified?.hiddenInPalette).toBe(true);
    expect(classified?.chord).toMatchObject({
      key: "L",
      mod: true,
      shift: true,
      requireVault: true,
    });
    expect(classified?.action).toEqual({ type: "openClassifiedPanel" });
  });

  it("disables classified panel shortcut without vault", () => {
    const items = buildCommandPaletteItems({
      hasVault: false,
      hasActiveNote: false,
    });
    expect(items.find((i) => i.id === "classified-panel")?.disabled).toBe(true);
  });

  it("detects classified vault-relative paths", () => {
    expect(isClassifiedVaultPath(".classified/secret.md")).toBe(true);
    expect(isClassifiedVaultPath(".classified/inbox/note.md")).toBe(true);
    expect(isClassifiedVaultPath("notes/open.md")).toBe(false);
    expect(isClassifiedVaultPath(".classified")).toBe(false);
  });

  it("converts absolute paths under vault to relative form", () => {
    expect(vaultRelativePath("/vault", "/vault/notes/a.md")).toBe("notes/a.md");
    expect(vaultRelativePath("/vault", "/other/a.md")).toBeNull();
  });

  it("disables editor edit actions when note is locked", () => {
    const lockedCtx = {
      hasNote: true,
      hasSelection: true,
      streaming: false,
      isLocked: true,
    };
    const paste = filterEditorActions("context_menu", "editor", {
      ...lockedCtx,
      isLocked: false,
    }).find((a) => a.id === "paste");
    expect(paste).toBeDefined();
    expect(isEditorActionEnabled(paste!, lockedCtx)).toBe(false);
    const rewrite = filterEditorActions("context_menu", "editor", {
      ...lockedCtx,
      isLocked: false,
    }).find((a) => a.id === "rewrite");
    expect(rewrite).toBeDefined();
    expect(isEditorActionEnabled(rewrite!, lockedCtx)).toBe(false);
    const copy = filterEditorActions("context_menu", "editor", {
      ...lockedCtx,
      isLocked: false,
    }).find((a) => a.id === "copy");
    expect(copy).toBeDefined();
    expect(isEditorActionEnabled(copy!, lockedCtx)).toBe(true);
  });

  it("TipTapEditor supports locked prop and lock toggle button", () => {
    const src = read("src/components/editor/TipTapEditor.tsx");
    expect(src).toContain("locked?: boolean");
    expect(src).toContain("setLocked?: (locked: boolean) => void");
    expect(src).toContain("editable: !locked");
    expect(src).toContain('data-testid="editor-lock-toggle"');
  });

  it("DocumentTitleField supports readOnly prop", () => {
    const src = read("src/components/editor/DocumentTitleField.tsx");
    expect(src).toContain("readOnly?: boolean");
    expect(src).toContain("readOnly={readOnly");
  });

  it("useEditorContextMenu skips menu when locked", () => {
    const src = read("src/hooks/useEditorContextMenu.ts");
    expect(src).toContain("locked = false");
    expect(src).toMatch(/if\s*\(\s*locked\s*\)\s*return/);
  });

  it("App wires file lock state and classified panel", () => {
    const src = read("src/App.tsx");
    const ipc = read("src/lib/ipc.ts");
    expect(src).toContain("fileSetLock");
    expect(src).toContain("ClassifiedPanel");
    expect(src).toContain("classifiedOpen");
    expect(src).toContain("listenClassifiedFileTaken");
    expect(ipc).toContain("classified:file_taken");
    expect(src).toContain("locked={");
    expect(src).toContain("setLocked={");
  });

  it("classified panel components exist with full file operations", () => {
    const list = read("src/components/classified/ClassifiedFileList.tsx");
    expect(list).toContain("classifiedImport");
    expect(list).toContain("classifiedExport");
    expect(list).toContain("classifiedDelete");
    expect(list).not.toContain("仅占位");
    const panel = read("src/components/classified/ClassifiedPanel.tsx");
    expect(panel).toContain("ClassifiedPasswordSetup");
    expect(panel).toContain("ClassifiedPasswordPrompt");
    expect(panel).toContain("waiting");
    expect(panel).toContain("idleDeadline");
    expect(list).toContain("classified-idle-countdown");
  });

  it("App uses global classified vault idle session hook", () => {
    const app = read("src/App.tsx");
    expect(app).toContain("useClassifiedVaultSession");
    expect(app).toContain("activeNoteIsClassified");
    expect(app).toContain("笔记已锁定，无法保存");
  });

  it("App never forwards classified note material into AI surfaces", () => {
    const app = read("src/App.tsx");
    expect(app).toContain(
      "const assistantNotePath = activeNoteIsClassified ? null : activePath;",
    );
    expect(app).toContain(
      'const assistantNoteContent = activeNoteIsClassified ? "" : markdown;',
    );
    expect(app).toContain("notePath={assistantNotePath}");
    expect(app).toContain("noteContent={assistantNoteContent}");
    expect(app).toContain("if (isClassifiedVaultPath(path)) return null;");
    expect(app).toContain("if (activeNoteIsClassified) {");
    expect(app).toContain("涉密笔记不能发送到 AI");
  });

  it("main note open paths cannot open classified notes", () => {
    const tabs = read("src/hooks/useTabManager.ts");
    expect(tabs).toContain("allowClassified?: boolean");
    expect(tabs).toContain("涉密笔记只能从涉密保险库打开");
    expect(tabs).toContain("fileRead(path, {");
    expect(tabs).toContain(
      "allowClassified: options?.allowClassified === true",
    );

    const app = read("src/App.tsx");
    expect(app).toContain("onOpenFile={(path) =>");
    expect(app).toContain(
      "openNoteLeavingHome(path, undefined, { allowClassified: true })",
    );
  });
});
