import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

function fileExists(path: string): boolean {
  try {
    readFileSync(path, "utf8");
    return true;
  } catch {
    return false;
  }
}

describe("document lifecycle source contracts", () => {
  it("VaultNavigator creates notes through createDefaultNote instead of raw fileCreate", () => {
    const source = read("src/components/file/VaultNavigator.tsx");
    expect(source).not.toContain("fileCreate(");
    expect(source).toContain("createDefaultNote({");
    expect(source).toContain("titleHint:");
    expect(source).toContain("folderPrefix: selectedFolder");
  });

  it("VaultNavigator keeps root folder creation distinct from dialog closed state", () => {
    const source = read("src/components/file/VaultNavigator.tsx");
    expect(source).toContain("folderCreateOpen");
    expect(source).toContain('setFolderCreateParent("")');
    expect(source).toContain("open={folderCreateOpen}");
  });

  it("StatusBar consumes live characterCount instead of persisted markdown wordCount", () => {
    const source = read("src/components/layout/StatusBar.tsx");
    expect(source).toContain("characterCount");
    expect(source).not.toContain("wordCount");
  });

  it("App passes live editor stats to StatusBar", () => {
    const source = read("src/App.tsx");
    expect(source).toContain("editorStats");
    expect(source).toContain("onBodyStatsChange");
    expect(source).not.toContain("splitFrontmatter(markdown).body.replace");
  });

  it("App reuses flushSave markdown when creating a manual version", () => {
    const source = read("src/App.tsx");
    expect(source).toContain("const md = await flushSave();");
    expect(source).toContain("versionSaveManual(path, md)");
    expect(source).not.toContain(
      "await versionSaveManual(path, getLiveMarkdown());",
    );
  });

  it("source contains no agent debug logging or note-content previews", () => {
    const sources = [
      "src/App.tsx",
      "src/hooks/useEditorSave.ts",
      "src/hooks/useOpenNote.ts",
      "src/hooks/useTabManager.ts",
      "src/components/editor/TipTapEditor.tsx",
      "src/lib/ipc.ts",
      "src/lib/serialize-open-note.ts",
      "src-tauri/src/commands/file.rs",
      "src-tauri/src/lib.rs",
    ]
      .map(read)
      .join("\n");

    expect(sources).not.toContain("debug_session_log");
    expect(sources).not.toContain("debugSessionLog");
    expect(sources).not.toContain("8589f0");
    expect(sources).not.toContain("/ingest/");
    expect(sources).not.toContain("mdPreview");
    expect(sources).not.toContain('"preview"');
    expect(sources).not.toContain('"tail"');
    expect(fileExists("src-tauri/src/debug_session_log.rs")).toBe(false);
  });

  it("serializeOpenNote does not use HTML fallback heuristics", () => {
    const source = read("src/lib/serialize-open-note.ts");
    expect(source).not.toContain("exportEditorToMarkdown");
    expect(source).not.toContain("spacerAwareHtmlBody");
    expect(source).not.toContain("baselineDocChars");
    expect(source).not.toContain("isDirty");
  });

  it("layer-1 save syncs markdown state so markdownRef is not stomped on re-render", () => {
    const app = read("src/App.tsx");
    expect(app).toContain("setMarkdown(md)");
    const openNote = read("src/hooks/useOpenNote.ts");
    expect(openNote).toContain("editorContentTick");
    expect(openNote).not.toMatch(
      /editorBodyMarkdown[\s\S]*?\[activePath, editorContentTick, markdown,/,
    );
  });

  it("active tab always flushes on leave (no dirty-only skip)", () => {
    const app = read("src/App.tsx");
    const persist = read("src/lib/persist-before-leave.ts");
    expect(app).toContain("persistActiveTabBeforeLeave");
    expect(app).toContain("flushSaveForPath");
    expect(persist).toContain("await flushSaveForPath(path, getMarkdown)");
    expect(app).not.toContain("skip fileWrite: not dirty");
  });

  it("app close blocks version idle enqueue via scheduler shutdown flag", () => {
    const app = read("src/App.tsx");
    expect(app).toContain('reason: "app_close"');
    expect(app).toContain("setAppClosing(true)");
    expect(app).toContain("clearVersionIdleTimer");
  });

  it("activateTab clears HTML cache before restoring session markdown", () => {
    const source = read("src/hooks/useTabManager.ts");
    expect(source).toContain("clearCachedEditorHtml(path)");
  });

  it("path sync forwards freshly serialized editor markdown before remounting by path", () => {
    const source = read("src/hooks/useOpenNote.ts");
    expect(source).toContain("const liveMarkdown = serializeOpenNote({");
    expect(source).toContain(
      "replaceOpenTabPath(path, entry.path, nextTitle, liveMarkdown)",
    );
  });
});
