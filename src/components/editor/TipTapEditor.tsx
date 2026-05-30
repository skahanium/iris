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
import {
  useEffect,
  useMemo,
  useRef,
  type MouseEvent,
  type ReactNode,
} from "react";

import { markdownBodyToEditorHtml } from "@/lib/markdown";
import { cn } from "@/lib/utils";

import { AiStreamExtension } from "./extensions/AiStreamExtension";
import { HeadingFoldExtension } from "./extensions/HeadingFoldExtension";
import { ImageExtension } from "./extensions/ImageExtension";
import { IrisDocument } from "./extensions/IrisDocument";
import { LinkExtension } from "./extensions/LinkExtension";
import { SlashCommandExtension } from "./extensions/SlashCommandExtension";
import { WikiLinkExtension } from "./extensions/WikiLinkExtension";

const lowlight = createLowlight(common);

interface TipTapEditorProps {
  /** Body markdown only (frontmatter / document title are separate). */
  initialBodyMarkdown: string;
  zen?: boolean;
  onDirty?: () => void;
  onSlashCommand?: (command: string) => void;
  onEditorReady?: (editor: Editor) => void;
  onInlineAiRetry?: (editor: Editor) => void;
  onOpenWikiLink?: (title: string) => void;
  zoom?: number;
  className?: string;
  /** Document title field rendered above body inside the shared editor canvas. */
  titleSlot?: ReactNode;
  /** 屏蔽原生右键并打开 Iris 菜单 */
  onBodyContextMenu?: (event: MouseEvent) => void;
}

export function TipTapEditor({
  initialBodyMarkdown,
  zen = false,
  onDirty,
  onSlashCommand,
  onEditorReady,
  onInlineAiRetry,
  onOpenWikiLink,
  zoom = 1,
  className,
  titleSlot,
  onBodyContextMenu,
}: TipTapEditorProps) {
  const inlineAiRetryRef = useRef(onInlineAiRetry);
  inlineAiRetryRef.current = onInlineAiRetry;

  const onDirtyRef = useRef(onDirty);
  onDirtyRef.current = onDirty;

  const editorRef = useRef<Editor | null>(null);

  const onSlashCommandRef = useRef(onSlashCommand);
  onSlashCommandRef.current = onSlashCommand;

  const onOpenWikiLinkRef = useRef(onOpenWikiLink);
  onOpenWikiLinkRef.current = onOpenWikiLink;

  const extensions = useMemo(
    () => [
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
      Placeholder.configure({
        placeholder: "开始写作，或输入 / 唤起 AI…",
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
    content: markdownBodyToEditorHtml(initialBodyMarkdown),
    onUpdate: () => {
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
  }, [editor, onEditorReady]);

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
          {titleSlot ? (
            <div className="iris-editor-title-slot">{titleSlot}</div>
          ) : null}
          <div
            className="iris-editor-body"
            onContextMenu={onBodyContextMenu}
          >
            <EditorContent editor={editor} />
          </div>
        </div>
      </div>
    </div>
  );
}

export type { Editor } from "@tiptap/react";
