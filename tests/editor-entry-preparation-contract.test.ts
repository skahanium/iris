import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("editor entry note preparation contract", () => {
  it("prepares wiki links and AI evidence sources before opening", () => {
    const wiki = read("src/components/editor/extensions/WikiLinkExtension.ts");
    const tiptap = read("src/components/editor/TipTapEditor.tsx");
    const workspace = read("src/components/layout/AppEditorWorkspace.tsx");
    const outline = read("src/components/editor/EditorOutline.tsx");
    const aiSlot = read("src/components/layout/AppAiPanelSlot.tsx");
    const app = read("src/App.impl.tsx");

    expect(wiki).toContain("onPrepareNote");
    expect(wiki).toContain("handleDOMEvents");
    expect(wiki).toContain("mouseover");
    expect(wiki).toContain("focusin");
    expect(tiptap).toContain("onPrepareWikiLink");
    expect(workspace).toContain("onPrepareWikiLink");
    expect(workspace).toContain(
      'onPrepareNotePath?.(title + ".md", title, "link")',
    );
    expect(outline).not.toContain("onPrepareNote");
    expect(outline).not.toContain("fileLinkSummary");
    expect(outline).toContain("outline-ghost-popover");
    expect(aiSlot).toContain(
      'onPrepareNotePath?.(packet.source_path, packet.title, "ai")',
    );
    expect(app).toContain("onPrepareNotePath={prepareNotePath}");
    expect(workspace).not.toContain('source: "outline"');
  });
});
