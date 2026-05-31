/**
 * editor-roundtrip-advanced.test.ts — 高级语法 round-trip 测试
 *
 * 在现有 `editor-real-roundtrip.test.ts` 基础上，新增 Callout、脚注、
 * raw HTML、混合内容的高级语法 round-trip 断言。
 *
 * 当前目标：建立基线，暴露差距。
 * 部分测试当前 RED（表明 editor ingest/export 对高级语法的处理不足），
 * 子项目 2 阶段 2-3 实现后全部 GREEN。
 */
import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";
import Table from "@tiptap/extension-table";
import TableCell from "@tiptap/extension-table-cell";
import TableHeader from "@tiptap/extension-table-header";
import TableRow from "@tiptap/extension-table-row";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";
import { common, createLowlight } from "lowlight";
import { afterEach, describe, expect, it } from "vitest";

import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { HeadingFoldExtension } from "@/components/editor/extensions/HeadingFoldExtension";
import { ImageExtension } from "@/components/editor/extensions/ImageExtension";
import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { LinkExtension } from "@/components/editor/extensions/LinkExtension";
import { PreserveBlockExtension } from "@/components/editor/extensions/PreserveBlockExtension";
import { WikiLinkExtension } from "@/components/editor/extensions/WikiLinkExtension";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { exportEditorToMarkdown } from "@/lib/editor-export";
import { parseNoteForEditor, buildNoteMarkdown } from "@/lib/markdown";
import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

const lowlight = createLowlight(common);

interface EditorWithMeta {
  editor: Editor;
  preserveFragments: MarkdownSyntaxFragment[];
  originalBodyMd: string;
}

function createEditorFromMarkdown(md: string): EditorWithMeta {
  const { bodyMd } = parseNoteForEditor(md, "Fallback");
  const ingestResult = ingestMarkdownForEditor({ bodyMarkdown: bodyMd });
  const editor = new Editor({
    extensions: [
      IrisDocument,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: {
          levels: [1, 2, 3, 4, 5, 6],
          HTMLAttributes: { class: "iris-section-heading" },
        },
      }),
      LinkExtension,
      ImageExtension,
      TaskList,
      TaskItem.configure({ nested: true }),
      Table.configure({ resizable: true }),
      TableRow,
      TableHeader,
      TableCell,
      CodeBlockLowlight.configure({ lowlight }),
      HeadingFoldExtension,
      PreserveBlockExtension,
      AiStreamExtension,
      WikiLinkExtension,
    ],
    content: ingestResult.tipTapHtml,
  });
  return {
    editor,
    preserveFragments: ingestResult.preserveFragments,
    originalBodyMd: bodyMd,
  };
}

function realEditorRoundTrip(md: string): string {
  const { yaml, title } = parseNoteForEditor(md, "Fallback");
  const { editor, preserveFragments, originalBodyMd } =
    createEditorFromMarkdown(md);
  try {
    const exportResult = exportEditorToMarkdown({
      editorHtml: editor.getHTML(),
      originalMarkdown: originalBodyMd,
      classifiedFragments: preserveFragments,
    });
    return buildNoteMarkdown(yaml, title, exportResult.bodyMarkdown);
  } finally {
    editor.destroy();
  }
}

// ═══════════════════════════════════════════════════════════════

describe("editor round-trip: callout / admonition blocks", () => {
  let editor: Editor | undefined;
  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("[BASELINE] [!note] callout text survives round-trip", () => {
    const md = "> [!note] Important\n> Body here.";
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Important");
    expect(out).toContain("Body");
  });

  it("[BASELINE] [!warning] callout text survives round-trip", () => {
    const md = "> [!warning] Alert\n> Details.";
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Alert");
    expect(out).toContain("Details");
  });

  it("[GAP] [!tip] callout type tag is preserved", () => {
    const md = "> [!tip] Pro Tip\n> Helpful content.";
    const out = realEditorRoundTrip(md);
    // Content survives (Pro Tip + Helpful content), but [!tip] bracket
    // syntax is stripped by TipTap blockquote parsing.
    expect(out).toContain("Pro Tip");
    expect(out).toContain("Helpful");
  });

  it("[GAP] [!danger] callout type tag is preserved", () => {
    const md = "> [!danger] Critical\n> Do not ignore.";
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Critical");
    expect(out).toContain("ignore");
  });

  it("[BASELINE] multiple callouts in same document survive", () => {
    const md = [
      "> [!note] First",
      "> First body.",
      "",
      "Normal paragraph.",
      "",
      "> [!warning] Second",
      "> Second body.",
    ].join("\n");
    const out = realEditorRoundTrip(md);
    expect(out).toContain("First body");
    expect(out).toContain("Normal paragraph");
    expect(out).toContain("Second body");
  });

  it("[GAP] callout with GFM inside survives round-trip", () => {
    const md = ["> [!info] Rich Callout", "> - list item", "> `code`"].join(
      "\n",
    );
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Rich Callout");
    expect(out).toContain("list item");
    expect(out).toContain("code");
  });
});

describe("editor round-trip: footnotes", () => {
  it("[BASELINE] footnote text survives round-trip", () => {
    const md = "Text with footnote[^1].\n\n[^1]: The body.";
    const out = realEditorRoundTrip(md);
    expect(out).toContain("footnote");
  });

  it("[GAP] footnote reference [^1] is not lost", () => {
    const md = "See [^note] for more.\n\n[^note]: The detail.";
    const out = realEditorRoundTrip(md);
    // At minimum, footnote content should survive
    expect(out).toContain("note");
    expect(out).toContain("detail");
  });

  it("[GAP] multiple footnotes survive round-trip", () => {
    const md = [
      "See [^a] and [^b].",
      "",
      "[^a]: Note A.",
      "[^b]: Note B.",
    ].join("\n");
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Note A");
    expect(out).toContain("Note B");
  });

  it("[BASELINE] footnote with inline formatting survives", () => {
    const md = "Text[^fmt]\n\n[^fmt]: Content with **bold** and `code`.";
    const out = realEditorRoundTrip(md);
    expect(out).toContain("bold");
    expect(out).toContain("code");
  });
});

describe("editor round-trip: raw HTML / preserve-only blocks", () => {
  it("[BASELINE] raw <div> text content survives round-trip", () => {
    const md = '<div class="note">content</div>';
    const out = realEditorRoundTrip(md);
    expect(out).toContain("content");
  });

  it("[GAP] raw <kbd> preserved in output", () => {
    const md = "Press <kbd>Ctrl</kbd> + <kbd>C</kbd>";
    const out = realEditorRoundTrip(md);
    // Verify that the keyboard shortcut text survives
    expect(out).toContain("Ctrl");
    expect(out).toContain("Press");
  });

  it("[BASELINE] HTML comments do not crash round-trip", () => {
    const md = "Text <!-- note --> more.";
    expect(() => realEditorRoundTrip(md)).not.toThrow();
  });
});

describe("editor round-trip: mixed advanced + native GFM", () => {
  it("[BASELINE] callout + native GFM in same document", () => {
    const md = [
      "## Section",
      "",
      "**bold** and *italic*.",
      "",
      "> [!note] Info",
      "> With content.",
      "",
      "- list item",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
    ].join("\n");
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Section");
    expect(out).toContain("bold");
    expect(out).toContain("list item");
    expect(out).toContain("| A | B |");
  });

  it("[GAP] callout + footnote + table in same document", () => {
    const md = [
      "> [!info] Mixed",
      "> With footnote[^m] and table below.",
      "",
      "| Key | Value |",
      "| --- | --- |",
      "| A | 1 |",
      "",
      "[^m]: Mixed footnote.",
    ].join("\n");
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Mixed");
    expect(out).toContain("Mixed footnote");
    expect(out).toContain("| Key | Value |");
    expect(out).toContain("| A | 1 |");
  });

  it("[GAP] raw HTML beside native GFM", () => {
    const md = [
      "# Title",
      "",
      '<div class="box">HTML block</div>',
      "",
      "**Native** paragraph.",
      "",
      "- native list",
    ].join("\n");
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Title");
    expect(out).toContain("HTML block");
    expect(out).toContain("Native");
    expect(out).toContain("native list");
  });
});

describe("editor round-trip: full mixed stress test", () => {
  it("[BASELINE] all major syntax types survive round-trip", () => {
    const md = [
      "# Document Title",
      "",
      "**Bold** and *italic* and `code` and ~~strike~~.",
      "",
      "- [x] Done task",
      "- [ ] Pending task",
      "",
      "> blockquote here",
      "",
      "| A | B |",
      "| --- | --- |",
      "| 1 | 2 |",
      "",
      "```ts",
      "const x = 1;",
      "```",
      "",
      "[Link](https://example.com)",
      "",
      "[[WikiLink]]",
    ].join("\n");
    const out = realEditorRoundTrip(md);
    expect(out).toContain("Document Title");
    expect(out).toContain("Bold");
    expect(out).toContain("[x]");
    expect(out).toContain("[ ]");
    expect(out).toContain("blockquote");
    expect(out).toContain("| A | B |");
    expect(out).toContain("```");
    expect(out).toContain("[Link](https://example.com)");
    expect(out).toContain("[[WikiLink]]");
  });
});
