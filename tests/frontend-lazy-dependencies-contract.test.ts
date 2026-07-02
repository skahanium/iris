import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

function read(path: string): string {
  return readFileSync(path, "utf8");
}

describe("frontend lazy dependency contract", () => {
  it("keeps markdown/highlight/tippy libraries out of eager editor and startup imports", () => {
    const main = read("src/main.tsx");
    const editor = read("src/components/editor/TipTapEditor.tsx");
    const slash = read(
      "src/components/editor/extensions/SlashCommandExtension.ts",
    );
    const wiki = read("src/components/editor/extensions/WikiLinkExtension.ts");
    const markdown = read("src/lib/markdown.ts");
    const sanitize = read("src/lib/sanitize.ts");
    const editorExport = read("src/lib/editor-export.ts");

    expect(main).not.toContain('import "tippy.js/dist/tippy.css"');
    expect(editor).not.toContain('from "lowlight"');
    expect(editor).not.toContain(
      'import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight"',
    );
    expect(editor).toContain("createLazyCodeBlockLowlightExtension");
    expect(slash).not.toContain('from "tippy.js"');
    expect(wiki).not.toContain('from "tippy.js"');
    expect(slash).toContain('import("tippy.js")');
    expect(wiki).toContain('import("tippy.js")');
    expect(markdown).not.toContain("import { Marked");
    expect(markdown).not.toContain("import TurndownService");
    expect(sanitize).not.toContain("import DOMPurify");
    expect(editorExport).not.toContain("import TurndownService");
  });
});
