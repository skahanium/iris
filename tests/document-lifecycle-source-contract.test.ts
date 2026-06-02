import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
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
});
