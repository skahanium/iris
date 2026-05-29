import CodeBlockLowlight from "@tiptap/extension-code-block-lowlight";

import Placeholder from "@tiptap/extension-placeholder";

import Table from "@tiptap/extension-table";

import TableCell from "@tiptap/extension-table-cell";

import TableHeader from "@tiptap/extension-table-header";

import TableRow from "@tiptap/extension-table-row";

import TaskItem from "@tiptap/extension-task-item";

import TaskList from "@tiptap/extension-task-list";

import { EditorContent, useEditor, type Editor } from "@tiptap/react";

import StarterKit from "@tiptap/starter-kit";

import { common, createLowlight } from "lowlight";

import { useEffect, useMemo, useRef } from "react";

import { markdownToEditorHtml } from "@/lib/markdown";

import { noteTitleFromEditor } from "@/lib/note-title";

import { cn } from "@/lib/utils";

import { AiStreamExtension } from "./extensions/AiStreamExtension";
import { HeadingFoldExtension } from "./extensions/HeadingFoldExtension";
import { IrisDocument } from "./extensions/IrisDocument";
import { NoteTitleExtension } from "./extensions/NoteTitleExtension";
import { SlashCommandExtension } from "./extensions/SlashCommandExtension";
import { WikiLinkExtension } from "./extensions/WikiLinkExtension";

const lowlight = createLowlight(common);

interface TipTapEditorProps {
  initialMarkdown: string;

  titleFallback?: string;

  zen?: boolean;

  /** Fired when the `noteTitle` block changes (including initial load). */

  onTitleChange?: (title: string) => void;

  onDirty?: () => void;

  onSlashCommand?: (command: string) => void;

  onEditorReady?: (editor: Editor) => void;

  onInlineAiRetry?: (editor: Editor) => void;

  onOpenWikiLink?: (title: string) => void;

  /** Visual scale of the editor canvas (0.75–1.5). */

  zoom?: number;

  className?: string;
}

export function TipTapEditor({
  initialMarkdown,

  titleFallback = "",

  zen = false,

  onTitleChange,

  onDirty,

  onSlashCommand,

  onEditorReady,

  onInlineAiRetry,

  onOpenWikiLink,

  zoom = 1,

  className,
}: TipTapEditorProps) {
  const inlineAiRetryRef = useRef(onInlineAiRetry);

  inlineAiRetryRef.current = onInlineAiRetry;

  const onDirtyRef = useRef(onDirty);

  onDirtyRef.current = onDirty;

  const onTitleChangeRef = useRef(onTitleChange);

  onTitleChangeRef.current = onTitleChange;

  const lastEmittedTitleRef = useRef<string | null>(null);

  const firedInitialRef = useRef(false);

  const editorRef = useRef<Editor | null>(null);

  const onSlashCommandRef = useRef(onSlashCommand);
  onSlashCommandRef.current = onSlashCommand;

  const onOpenWikiLinkRef = useRef(onOpenWikiLink);
  onOpenWikiLinkRef.current = onOpenWikiLink;

  /** 稳定引用，避免父组件重渲染时销毁并重建编辑器（会从陈旧 markdown 恢复标题） */
  const extensions = useMemo(
    () => [
      IrisDocument,
      NoteTitleExtension,
      StarterKit.configure({
        document: false,
        codeBlock: false,
        heading: {
          levels: [1, 2, 3],
          HTMLAttributes: { class: "iris-section-heading" },
        },
      }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Table.configure({ resizable: true }),
      TableRow,
      TableHeader,
      TableCell,
      CodeBlockLowlight.configure({ lowlight }),
      Placeholder.configure({
        placeholder: ({ node }) =>
          node.type.name === "noteTitle"
            ? "无标题"
            : "开始写作，或输入 / 唤起 AI…",
      }),
      HeadingFoldExtension,
      AiStreamExtension.configure({
        onRetry: (ed) => inlineAiRetryRef.current?.(ed),
      }),
      SlashCommandExtension.configure({
        onCommand: (command) => onSlashCommandRef.current?.(command),
      }),
      WikiLinkExtension.configure({
        onOpenNote: (title) => onOpenWikiLinkRef.current?.(title),
      }),
    ],
    [],
  );

  const editor = useEditor({
    extensions,

    content: markdownToEditorHtml(initialMarkdown, titleFallback),

    onUpdate: () => {
      if (!firedInitialRef.current) {
        firedInitialRef.current = true;

        return;
      }

      onDirtyRef.current?.();
    },

    editorProps: {
      attributes: {
        class: "focus:outline-none",
      },
    },
  });

  useEffect(() => {
    if (!editor) return;

    editorRef.current = editor;
    onEditorReady?.(editor);

    const first = editor.state.doc.firstChild;

    if (first?.type.name === "noteTitle" && first.content.size === 0) {
      editor.commands.focus("start");
    }
  }, [editor, onEditorReady]);

  useEffect(() => {
    if (!editor) return;

    let skipInitial = true;

    const emitTitle = () => {
      if (skipInitial) return;
      const next = noteTitleFromEditor(editor);

      if (next === lastEmittedTitleRef.current) return;

      lastEmittedTitleRef.current = next;

      onTitleChangeRef.current?.(next);
    };

    const onTransaction = ({
      transaction,
    }: {
      transaction: { docChanged: boolean };
    }) => {
      if (transaction.docChanged) emitTitle();
    };

    editor.on("transaction", onTransaction);

    const frame = requestAnimationFrame(() => {
      skipInitial = false;
    });

    return () => {
      editor.off("transaction", onTransaction);
      cancelAnimationFrame(frame);
    };
  }, [editor]);

  return (
    <div
      data-testid="editor"
      className={cn("iris-editor flex min-h-0 flex-1 flex-col", className)}
      data-zen={zen ? "true" : undefined}
      data-editor-zoom={zoom}
    >
      <div className="iris-editor-zoom-scroll min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
        <div
          className="iris-editor-canvas"
          style={{ zoom } as React.CSSProperties}
        >
          <div className="iris-editor-body">
            <EditorContent editor={editor} />
          </div>
        </div>
      </div>
    </div>
  );
}

export type { Editor } from "@tiptap/react";
