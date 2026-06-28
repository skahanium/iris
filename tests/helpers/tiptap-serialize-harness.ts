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

import { CalloutBlockquoteExtension } from "@/components/editor/extensions/CalloutBlockquoteExtension";
import { AiStreamExtension } from "@/components/editor/extensions/AiStreamExtension";
import { HeadingFoldExtension } from "@/components/editor/extensions/HeadingFoldExtension";
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
import { markdownBodyToEditorHtml, parseNoteForEditor } from "@/lib/markdown";
import { serializeOpenNote } from "@/lib/serialize-open-note";

const lowlight = createLowlight(common);

function productionExtensions(vaultPath: string | null = null) {
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
    CodeBlockLowlight.configure({ lowlight }),
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

/** Ingest via contract pipeline (preserve blocks, callouts, etc.). */
export function createProductionEditorFromIngestedBody(
  bodyMd: string,
  vaultPath: string | null = null,
): Editor {
  const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: bodyMd });
  return new Editor({
    extensions: productionExtensions(vaultPath),
    content: tipTapHtml,
    parseOptions: EDITOR_PARSE_OPTIONS,
  });
}

export function createProductionEditorFromBody(
  bodyMd: string,
  vaultPath: string | null = null,
): Editor {
  return new Editor({
    extensions: productionExtensions(vaultPath),
    content: markdownBodyToEditorHtml(bodyMd),
    parseOptions: EDITOR_PARSE_OPTIONS,
  });
}

export function createProductionEditorFromNote(
  md: string,
  vaultPath: string | null = null,
): Editor {
  const { bodyMd } = parseNoteForEditor(md, "Fallback");
  return createProductionEditorFromIngestedBody(bodyMd, vaultPath);
}

export function pmSerializeBody(editor: Editor): string {
  return editorDocToMarkdown(editor);
}

export function fullNoteRoundTrip(md: string): string {
  const { yaml, title, bodyMd } = parseNoteForEditor(md, "Fallback");
  const editor = createProductionEditorFromNote(md);
  try {
    return serializeOpenNote({ yaml, title, editor, bodyFallbackMd: bodyMd });
  } finally {
    editor.destroy();
  }
}

export function normalizeMd(md: string): string {
  return md.replace(/\r\n/g, "\n").trim();
}
