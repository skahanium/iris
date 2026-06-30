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

  it("locking a note flushes pending editor content before changing the lock state", () => {
    const app = read("src/App.impl.tsx");
    expect(app).toMatch(
      /if \(locked\) \{[\s\S]*?await flushSave\(\);[\s\S]*?\}[\s\S]*?setFileLocked\(path, locked\);/,
    );
  });

  it("tab switching relies on digest-guarded editor HTML cache instead of clearing every open", () => {
    const tabManager = read("src/hooks/useTabManager.ts");
    const htmlCache = read("src/lib/editor-html-cache.ts");
    expect(tabManager).not.toContain("clearCachedEditorHtml(path)");
    expect(htmlCache).toContain("expectedDigest");
    expect(htmlCache).toContain("entry.digest !== expectedDigest");
  });

  it("syncs committed note title and body before the first painted editor frame", () => {
    const source = read("src/hooks/useOpenNote.ts");
    expect(source).toContain("useLayoutEffect");
    expect(source).toMatch(
      /useLayoutEffect\(\(\) => \{[\s\S]*?syncFromMarkdown\(markdownRef\.current, activePath\)/,
    );
  });
  it("path sync forwards freshly serialized editor markdown before remounting by path", () => {
    const source = read("src/hooks/useOpenNote.ts");
    expect(source).toContain("const liveMarkdown = serializeOpenNote({");
    expect(source).toContain(
      "replaceOpenTabPath(path, entry.path, nextTitle, liveMarkdown)",
    );
  });

  it("finalizing a note flushes layer-1 save and refreshes the vault index", () => {
    const app = read("src/App.impl.tsx");
    const overlays = read("src/components/layout/AppOverlays.tsx");
    const timeline = read("src/components/version/VersionTimeline.tsx");

    expect(app).toContain("handleBeforeFinalizeCurrent");
    expect(app).toMatch(
      /const md = await flushSave\(\);[\s\S]*?if \(md\) \{[\s\S]*?bumpVaultIndex\(\);[\s\S]*?return md;/,
    );
    expect(overlays).toContain("onBeforeFinalizeCurrent");
    expect(timeline).toContain(
      "onBeforeFinalizeCurrent?: () => Promise<string | null>",
    );
  });

  it("large editor ingest is worker-backed and guarded against stale results", () => {
    const source = read("src/hooks/useOpenNote.ts");
    expect(source).toContain("ingestMarkdownForEditorAsync");
    expect(source).toContain("editorIngestGenerationRef");
    expect(source).toContain(
      "generation !== editorIngestGenerationRef.current",
    );
    expect(source).toContain("activePathRef.current !== path");
  });

  it("accepting an external change for the active note applies external markdown directly", () => {
    const source = read("src/hooks/useFileConflictResolution.ts");

    expect(source).toContain(
      "const { externalContent, filePath } = conflictState",
    );
    expect(source).toContain("filePath === activePathRef.current");
    expect(source).toContain("applyMarkdownToEditor(externalContent)");
    expect(source).toContain("syncTabMarkdownCache(filePath, externalContent)");
    expect(source).toContain("markClean(");
    expect(source).toContain("openNoteLeavingHome(filePath)");
  });

  it("active classified tab leave snapshots write editor HTML to the classified cache namespace", () => {
    const source = read("src/hooks/useAppPersistenceLifecycle.ts");

    expect(source).toContain('from "@/lib/classified-path"');
    expect(source).toContain(
      'isClassifiedVaultPath(path) ? "classified" : "normal"',
    );
  });

  it("assistant stream listens for retry status without prompt content", () => {
    const ipc = read("src/lib/ipc.ts");
    const hook = read("src/hooks/useAssistantLlmStream.ts");
    const backend = read("src-tauri/src/ai_harness/harness/run.rs");
    expect(ipc).toContain("listenAiRetryStatus");
    expect(hook).toContain("listenAiRetryStatus");
    expect(hook).toContain("setActivityHint");
    expect(hook).not.toContain('role: "system",\n          content: `重试中');
    expect(backend).toContain('"ai:retry_status"');
    expect(backend).toContain('"delay_ms": delay_ms');
    expect(backend).toContain('"reason_kind": retry_reason.reason_kind');
    expect(backend).toContain('"status_code": retry_reason.status_code');
    expect(backend).not.toContain('"message": request');
    expect(backend).not.toContain('"prompt"');
  });
});
