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
import { useEffect, useRef } from "react";

import { markdownToHtml } from "@/lib/markdown";
import { cn } from "@/lib/utils";

import { AiStreamExtension } from "./extensions/AiStreamExtension";
import { SlashCommandExtension } from "./extensions/SlashCommandExtension";
import { WikiLinkExtension } from "./extensions/WikiLinkExtension";

const lowlight = createLowlight(common);

interface TipTapEditorProps {
  initialMarkdown: string;
  onDirty?: () => void;
  onSlashCommand?: (command: string) => void;
  onEditorReady?: (editor: Editor) => void;
  onInlineAiRetry?: (editor: Editor) => void;
  onOpenWikiLink?: (title: string) => void;
  className?: string;
}

export function TipTapEditor({
  initialMarkdown,
  onDirty,
  onSlashCommand,
  onEditorReady,
  onInlineAiRetry,
  onOpenWikiLink,
  className,
}: TipTapEditorProps) {
  const inlineAiRetryRef = useRef(onInlineAiRetry);
  inlineAiRetryRef.current = onInlineAiRetry;

  const onDirtyRef = useRef(onDirty);
  onDirtyRef.current = onDirty;

  const firedInitialRef = useRef(false);

  const editor = useEditor({
    extensions: [
      StarterKit.configure({ codeBlock: false }),
      TaskList,
      TaskItem.configure({ nested: true }),
      Table.configure({ resizable: true }),
      TableRow,
      TableHeader,
      TableCell,
      CodeBlockLowlight.configure({ lowlight }),
      Placeholder.configure({ placeholder: "开始写作，或输入 / 唤起 AI…" }),
      AiStreamExtension.configure({
        onRetry: (ed) => inlineAiRetryRef.current?.(ed),
      }),
      SlashCommandExtension.configure({ onCommand: onSlashCommand }),
      WikiLinkExtension.configure({ onOpenNote: onOpenWikiLink }),
    ],
    content: markdownToHtml(initialMarkdown),
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
    if (editor) onEditorReady?.(editor);
  }, [editor, onEditorReady]);

  return (
    <div
      className={cn(
        "iris-editor flex-1",
        className,
      )}
    >
      <EditorContent editor={editor} />
    </div>
  );
}

export type { Editor } from "@tiptap/react";
