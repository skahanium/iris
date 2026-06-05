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
import { IrisParagraphExtension } from "@/components/editor/extensions/IrisParagraphExtension";
import { IrisDocument } from "@/components/editor/extensions/IrisDocument";
import { LinkExtension } from "@/components/editor/extensions/LinkExtension";
import { ListIndentKeymapExtension } from "@/components/editor/extensions/ListIndentKeymapExtension";
import { PreserveBlockExtension } from "@/components/editor/extensions/PreserveBlockExtension";
import { WikiLinkExtension } from "@/components/editor/extensions/WikiLinkExtension";
import { editorDocToMarkdown } from "@/lib/editor-pm-serialize";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";
import { markdownBodyToEditorHtml, parseNoteForEditor } from "@/lib/markdown";
import { serializeOpenNote } from "@/lib/serialize-open-note";

const lowlight = createLowlight(common);

const productionExtensions = [
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
  IrisParagraphExtension,
  ListIndentKeymapExtension,
  FindHighlightExtension,
  LinkExtension,
  ImageExtension,
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
  AiStreamExtension,
  WikiLinkExtension,
] as const;

/** Ingest via contract pipeline (preserve blocks, callouts, etc.). */
export function createProductionEditorFromIngestedBody(bodyMd: string): Editor {
  const { tipTapHtml } = ingestMarkdownForEditor({ bodyMarkdown: bodyMd });
  return new Editor({
    extensions: [...productionExtensions],
    content: tipTapHtml,
  });
}

export function createProductionEditorFromBody(bodyMd: string): Editor {
  return new Editor({
    extensions: [...productionExtensions],
    content: markdownBodyToEditorHtml(bodyMd),
  });
}

export function createProductionEditorFromNote(md: string): Editor {
  const { bodyMd } = parseNoteForEditor(md, "Fallback");
  return createProductionEditorFromIngestedBody(bodyMd);
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
