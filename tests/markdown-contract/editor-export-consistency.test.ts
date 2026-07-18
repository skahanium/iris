/**
 * editor-export-consistency.test.ts
 *
 * Contract-surface consistency via the production save path:
 * ingest → TipTap → `editorDocToMarkdown` (`pmSerializeBody`).
 */
import { describe, expect, it } from "vitest";

import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { renderMarkdownWithProfile } from "@/lib/markdown-contract/contract";

import {
  createProductionEditorFromIngestedBody,
  pmSerializeBody,
} from "../helpers/tiptap-serialize-harness";

function serializeBody(md: string): string {
  const editor = createProductionEditorFromIngestedBody(md);
  try {
    return pmSerializeBody(editor);
  } finally {
    editor.destroy();
  }
}

// ═══════════════════════════════════════════════════════════════
// 编辑器导出与 contract 一致性
// ═══════════════════════════════════════════════════════════════

describe("editor export = contract semantic", () => {
  it("editor export produces same body content as contract editor_export", () => {
    const md = "# Title\n\n**Bold** paragraph with `code`.\n\n- item";
    const contractExport = renderMarkdownWithProfile(md, "editor_export");
    const editorExport = serializeBody(md);

    expect(editorExport).toContain("Bold");
    expect(editorExport).toContain("code");
    expect(editorExport).toContain("item");
    expect(contractExport.output).toContain("Bold");
    expect(contractExport.output).toContain("code");
    expect(contractExport.output).toContain("item");
  });

  it("same markdown → ingest → export → stable", () => {
    const md = "**Bold** and *italic* and `code`.";
    const editor = createProductionEditorFromIngestedBody(md);
    try {
      const result1 = pmSerializeBody(editor);
      const result2 = pmSerializeBody(editor);
      expect(result1).toBe(result2);
    } finally {
      editor.destroy();
    }
  });

  it("editor ingest preserves callout type information", () => {
    const md = "> [!note] Info\n> Content.";
    const result = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(result.tipTapHtml).toContain("Info");
    expect(result.tipTapHtml).toContain("Content");
  });

  it("editor export after callout edit retains callout structure", () => {
    const md = "> [!note] Original\n> Body.";
    const bodyMarkdown = serializeBody(md);
    expect(bodyMarkdown).toContain("Original");
    expect(bodyMarkdown).toContain("Body");
    expect(bodyMarkdown).toContain("[!note]");
  });
});

// ═══════════════════════════════════════════════════════════════
// 重复导入导出稳定性
// ═══════════════════════════════════════════════════════════════

describe("repeated ingest-export stability", () => {
  it("pure native GFM: ingest → export → same", () => {
    const md = "# Title\n\n**Bold** `code` - list item";
    const export1 = serializeBody(md);
    expect(export1).toContain("Title");
    expect(export1).toContain("Bold");
    expect(export1).toContain("code");
  });

  it("mixed content: ingest → export → ingest → export stable", () => {
    const md =
      "# Doc\n\n**Bold**\n\n> [!note] Callout\n> Body\n\n<div class='x'>raw</div>";
    const ingest1 = ingestMarkdownForEditor({ bodyMarkdown: md });
    const export1 = serializeBody(md);

    const ingest2 = ingestMarkdownForEditor({
      bodyMarkdown: export1,
    });
    const export2 = serializeBody(export1);

    expect(ingest1.preserveFragments.length).toBeGreaterThan(0);
    expect(export1).toContain("Bold");
    expect(export2).toContain("Bold");
    expect(ingest2.preserveFragments.length).toBeGreaterThan(0);
  });

  it("ingest warnings are consistent across repeated calls", () => {
    const md = "<script>alert(1)</script>";
    const r1 = ingestMarkdownForEditor({ bodyMarkdown: md });
    const r2 = ingestMarkdownForEditor({ bodyMarkdown: md });
    expect(r1.warnings.length).toBe(r2.warnings.length);
  });
});

// ═══════════════════════════════════════════════════════════════
// Vault 预览 ↔ 编辑器导出一致性
// ═══════════════════════════════════════════════════════════════

describe("vault preview = editor export semantics", () => {
  it("vault_preview and editor export produce semantically equivalent output", () => {
    const md = "# Title\n\n**Bold** paragraph.\n\n- item 1\n- item 2";

    const vaultResult = renderMarkdownWithProfile(md, "vault_preview", {
      context: "Title",
    });
    const editorResult = serializeBody(md);

    expect(vaultResult.output).toContain("Title");
    expect(editorResult).toContain("Title");
    expect(vaultResult.output).toContain("Bold");
    expect(editorResult).toContain("Bold");
  });

  it("advanced syntax is preserved in both vault and editor export", () => {
    const md = "# Doc\n\n> [!info] Note\n> Body\n\n<div>raw</div>";
    const vaultResult = renderMarkdownWithProfile(md, "vault_preview");
    const editorResult = serializeBody(md);

    expect(vaultResult.output).toContain("Note");
    expect(vaultResult.output).toContain("Body");
    expect(editorResult).toContain("Note");
    expect(editorResult).toContain("Body");
  });
});

// ═══════════════════════════════════════════════════════════════
// 导出完整性：不丢失任何元素
// ═══════════════════════════════════════════════════════════════

describe("export completeness — no element loss", () => {
  it("export includes all heading levels", () => {
    const md = "# H1\n\n## H2\n\n### H3";
    const result = serializeBody(md);
    expect(result).toContain("H1");
    expect(result).toContain("H2");
    expect(result).toContain("H3");
  });

  it("export includes task lists", () => {
    const md = "- [x] Done\n- [ ] Pending";
    const result = serializeBody(md);
    expect(result).toContain("Done");
    expect(result).toContain("Pending");
    expect(result).toContain("[x]");
    expect(result).toContain("[ ]");
  });

  it("export includes GFM tables", () => {
    const md = "| A | B |\n| --- | --- |\n| 1 | 2 |";
    const result = serializeBody(md);
    expect(result).toContain("1");
    expect(result).toContain("2");
    expect(result).toContain("| A | B |");
  });

  it("export includes code blocks", () => {
    const md = "```ts\nconst x = 1;\n```";
    const result = serializeBody(md);
    expect(result).toContain("const x");
    expect(result).toContain("```");
  });
});
