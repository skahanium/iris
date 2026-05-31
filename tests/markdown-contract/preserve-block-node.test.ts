/**
 * PreserveBlock node unit tests.
 *
 * Tests the PreserveBlock TipTap node in isolation (parseHTML, renderHTML,
 * atom behavior, attribute preservation).
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

const lowlight = createLowlight(common);

function createEditor(content: string): Editor {
  return new Editor({
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
    content,
  });
}

// ═══════════════════════════════════════════════════════════════

describe("PreserveBlock node: parseHTML / renderHTML", () => {
  it("parses preserve-block HTML tag into a preserveBlock node", () => {
    const html =
      '<div data-type="preserve-block" data-original-raw="<div class=\'x\'>raw</div>" data-syntax-kind="raw_html"></div>';
    const editor = createEditor(html);
    let found = false;
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") found = true;
    });
    expect(found).toBe(true);
    editor.destroy();
  });

  it("parses originalRaw attribute from HTML", () => {
    const html =
      '<div data-type="preserve-block" data-original-raw="<div class=\'x\'>content</div>" data-syntax-kind="raw_html"></div>';
    const editor = createEditor(html);
    let raw = "";
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") {
        raw = (node.attrs.originalRaw as string) || "";
      }
    });
    expect(raw).toBe("<div class='x'>content</div>");
    editor.destroy();
  });

  it("parses syntaxKind attribute from HTML", () => {
    const html =
      '<div data-type="preserve-block" data-original-raw="<!-- note -->" data-syntax-kind="html_comment"></div>';
    const editor = createEditor(html);
    let kind = "";
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") {
        kind = (node.attrs.syntaxKind as string) || "";
      }
    });
    expect(kind).toBe("html_comment");
    editor.destroy();
  });

  it("renderHTML produces data-type='preserve-block'", () => {
    const editor = createEditor(
      '<div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div>',
    );
    const html = editor.getHTML();
    expect(html).toContain('data-type="preserve-block"');
    expect(html).toContain("data-original-raw");
    expect(html).toContain("data-syntax-kind");
    editor.destroy();
  });
});

describe("PreserveBlock node: atom behavior", () => {
  let editor: Editor | undefined;
  afterEach(() => {
    editor?.destroy();
    editor = undefined;
  });

  it("is atom — cursor cannot be placed inside", () => {
    editor = createEditor(
      '<div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div>',
    );
    // atom nodes have no content, so there's nothing to select inside
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") {
        expect(node.isAtom).toBe(true);
      }
    });
  });

  it("can be deleted as a whole unit", () => {
    editor = createEditor(
      '<p>before</p><div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div><p>after</p>',
    );
    // Select the preserve block and delete it
    let preservePos = -1;
    let preserveSize = 0;
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === "preserveBlock") {
        preservePos = pos;
        preserveSize = node.nodeSize;
      }
    });
    expect(preservePos).toBeGreaterThan(0);

    // Delete the node via transaction
    const tr = editor.state.tr.delete(preservePos, preservePos + preserveSize);
    editor.view.dispatch(tr);

    // Verify it's gone
    let found = false;
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") found = true;
    });
    expect(found).toBe(false);

    // Remaining content should be intact
    expect(editor.state.doc.textContent).toContain("before");
    expect(editor.state.doc.textContent).toContain("after");
  });

  it("typing before preserveBlock inserts paragraph before it", () => {
    editor = createEditor(
      '<p>initial</p><div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div><p>after</p>',
    );
    // Insert a new paragraph before the preserve block
    let preservePos = -1;
    editor.state.doc.descendants((node, pos) => {
      if (node.type.name === "preserveBlock" && preservePos === -1) {
        preservePos = pos;
      }
    });
    const tr = editor.state.tr.insert(
      preservePos,
      editor.state.schema.nodes.paragraph!.create(
        {},
        editor.state.schema.text("inserted"),
      ),
    );
    editor.view.dispatch(tr);

    const text = editor.state.doc.textContent;
    expect(text).toContain("inserted");
    expect(text).toContain("initial");
    expect(text).toContain("after");
  });
});

describe("PreserveBlock node: attribute preservation", () => {
  it("originalRaw survives round-trip through getHTML", () => {
    const editor = createEditor(
      '<div data-type="preserve-block" data-original-raw="<kbd>Ctrl</kbd>" data-syntax-kind="raw_html"></div>',
    );
    const html = editor.getHTML();
    expect(html).toContain('data-type="preserve-block"');
    expect(html).toContain("data-original-raw");
    // The originalRaw may be HTML-escaped in the output, but the value should survive
    expect(html).toContain("kbd");
    editor.destroy();
  });

  it("multiple preserve blocks each retain their own attributes", () => {
    const editor = createEditor(
      [
        '<div data-type="preserve-block" data-original-raw="<div class=\'a\'>A</div>" data-syntax-kind="raw_html"></div>',
        '<div data-type="preserve-block" data-original-raw="<!-- B -->" data-syntax-kind="html_comment"></div>',
        '<div data-type="preserve-block" data-original-raw="<kbd>C</kbd>" data-syntax-kind="raw_html"></div>',
      ].join(""),
    );
    const raws: string[] = [];
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") {
        raws.push((node.attrs.originalRaw as string) || "");
      }
    });
    expect(raws.length).toBe(3);
    expect(raws[0]).toContain("A");
    expect(raws[1]).toContain("B");
    expect(raws[2]).toContain("C");
    editor.destroy();
  });

  it("preserve block attribute defaults work when missing", () => {
    const editor = createEditor('<div data-type="preserve-block"></div>');
    let originalRaw = "SHOULD_NOT_BE";
    let syntaxKind = "SHOULD_NOT_BE";
    editor.state.doc.descendants((node) => {
      if (node.type.name === "preserveBlock") {
        originalRaw = (node.attrs.originalRaw as string) || "";
        syntaxKind = (node.attrs.syntaxKind as string) || "";
      }
    });
    expect(originalRaw).toBe("");
    expect(syntaxKind).toBe("raw_html");
    editor.destroy();
  });
});

describe("PreserveBlock node: coexistence with native nodes", () => {
  it("preserveBlock can be placed between native paragraphs", () => {
    const editor = createEditor(
      [
        "<p>Paragraph A</p>",
        '<div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div>',
        "<p>Paragraph B</p>",
      ].join(""),
    );
    const text = editor.state.doc.textContent;
    expect(text).toContain("Paragraph A");
    expect(text).toContain("Paragraph B");
    editor.destroy();
  });

  it("preserveBlock can follow heading", () => {
    const editor = createEditor(
      [
        "<h1>Title</h1>",
        '<div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div>',
        "<p>Content.</p>",
      ].join(""),
    );
    const text = editor.state.doc.textContent;
    expect(text).toContain("Title");
    expect(text).toContain("Content");
    editor.destroy();
  });

  it("preserveBlock can be inside blockquote via document structure", () => {
    // Blockquote contains the preserve block
    const editor = createEditor(
      [
        "<blockquote>",
        '<div data-type="preserve-block" data-original-raw="<div>x</div>" data-syntax-kind="raw_html"></div>',
        "</blockquote>",
      ].join(""),
    );
    // The document should parse without error
    expect(editor.state.doc.content.size).toBeGreaterThan(0);
    editor.destroy();
  });
});
