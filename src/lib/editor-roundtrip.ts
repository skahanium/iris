/**
 * Production Markdown round-trip helper.
 *
 * This is the single place that turns Markdown into a real TipTap editor and
 * back to Markdown using `editorDocToMarkdown`. Contract `editor_export` and
 * tests must use this path so they exercise the same serializer as the
 * production save hot path.
 */
import CodeBlock from "@tiptap/extension-code-block";
import Table from "@tiptap/extension-table";
import TableCell from "@tiptap/extension-table-cell";
import TableHeader from "@tiptap/extension-table-header";
import TableRow from "@tiptap/extension-table-row";
import TaskItem from "@tiptap/extension-task-item";
import TaskList from "@tiptap/extension-task-list";
import { Editor } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

import { CalloutBlockquoteExtension } from "@/components/editor/extensions/CalloutBlockquoteExtension";
import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { HeadingFoldExtension } from "@/components/editor/extensions/HeadingFoldExtension";
import { HeadingDomGuardExtension } from "@/components/editor/extensions/HeadingDomGuardExtension";
import { EmptyHeadingImeGuardExtension } from "@/components/editor/extensions/EmptyHeadingImeGuardExtension";
import { ImageExtension } from "@/components/editor/extensions/ImageExtension";
import { FindHighlightExtension } from "@/components/editor/extensions/FindHighlightExtension";
import {
  FootnoteDefExtension,
  FootnoteRefExtension,
} from "@/components/editor/extensions/FootnoteExtension";
import { ImeCompositionGuardExtension } from "@/components/editor/extensions/ImeCompositionGuardExtension";
import { IrisParagraphExtension } from "@/components/editor/extensions/IrisParagraphExtension";
import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { LinkExtension } from "@/components/editor/extensions/LinkExtension";
import { ListIndentKeymapExtension } from "@/components/editor/extensions/ListIndentKeymapExtension";
import { PreserveBlockExtension } from "@/components/editor/extensions/PreserveBlockExtension";
import { PreserveInlineExtension } from "@/components/editor/extensions/PreserveInlineExtension";
import { WikiLinkExtension } from "@/components/editor/extensions/WikiLinkExtension";
import { WikiMediaEmbedExtension } from "@/components/editor/extensions/WikiMediaEmbedExtension";
import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";
import { EDITOR_PARSE_OPTIONS } from "@/lib/editor-parse-options";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

/** Extensions used by the production editor and by Markdown round-trip checks. */
export function createProductionEditorExtensions(
  vaultPath: string | null = null,
) {
  return [
    IrisDocument,
    StarterKit.configure({
      document: false,
      paragraph: false,
      codeBlock: false,
      blockquote: false,
      heading: {
        levels: [1, 2, 3, 4, 5, 6],
        HTMLAttributes: { class: "iris-section-heading" },
      },
    }),
    ImeCompositionGuardExtension,
    HeadingDomGuardExtension,
    EmptyHeadingImeGuardExtension,
    IrisParagraphExtension,
    ListIndentKeymapExtension,
    FindHighlightExtension,
    LinkExtension,
    ImageExtension.configure({ vaultPath }),
    WikiMediaEmbedExtension.configure({ vaultPath, mediaLoading: "visible" }),
    TaskList,
    TaskItem.configure({ nested: true }),
    Table.configure({ resizable: true }),
    TableRow,
    TableHeader,
    TableCell,
    CodeBlock.configure({
      HTMLAttributes: { class: "iris-code-block" },
    }),
    CalloutBlockquoteExtension,
    HeadingFoldExtension,
    PreserveBlockExtension,
    PreserveInlineExtension,
    FootnoteRefExtension,
    FootnoteDefExtension,
    AiStreamExtension,
    WikiLinkExtension,
  ];
}

/** Create a real TipTap editor from Markdown using the production extension set. */
export function createProductionEditorFromMarkdown(
  bodyMd: string,
  vaultPath: string | null = null,
): Editor {
  const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: bodyMd });
  return new Editor({
    extensions: createProductionEditorExtensions(vaultPath),
    content: tipTapHtml,
    parseOptions: EDITOR_PARSE_OPTIONS,
  });
}

/** Markdown → TipTap → Markdown through the exact production save serializer. */
export function markdownToMarkdownViaProductionEditor(
  bodyMd: string,
  vaultPath: string | null = null,
): string {
  const editor = createProductionEditorFromMarkdown(bodyMd, vaultPath);
  try {
    return editorDocToMarkdown(editor);
  } finally {
    editor.destroy();
  }
}
