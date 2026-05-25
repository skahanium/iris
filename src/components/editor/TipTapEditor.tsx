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
/**
 * TipTap 扩展 = v0.1「核心 GFM」schema（非完整 GFM）。
 * @see ./gfm-schema.ts — SUPPORTED_CORE_GFM / UNSUPPORTED_OR_BEST_EFFORT_GFM
 */

const lowlight = createLowlight(common);

interface TipTapEditorProps {
  initialMarkdown: string;
  onUpdateHtml: (html: string) => void;
  onSlashCommand?: (command: string) => void;
  onEditorReady?: (editor: Editor) => void;
  onInlineAiRetry?: (editor: Editor) => void;
  onOpenWikiLink?: (title: string) => void;
  className?: string;
}

export function TipTapEditor({
  initialMarkdown,
  onUpdateHtml,
  onSlashCommand,
  onEditorReady,
  onInlineAiRetry,
  onOpenWikiLink,
  className,
}: TipTapEditorProps) {
  const inlineAiRetryRef = useRef(onInlineAiRetry);
  inlineAiRetryRef.current = onInlineAiRetry;

  const editor = useEditor({
    extensions: [
      // StarterKit: 标题、段落、粗体/斜体/删除线、列表、引用、水平线、行内 code 等
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
    onUpdate: ({ editor: ed }) => {
      onUpdateHtml(ed.getHTML());
    },
    editorProps: {
      attributes: {
        class:
          "prose prose-invert max-w-none min-h-[60vh] font-mono text-sm focus:outline-none px-6 py-4",
      },
    },
  });

  useEffect(() => {
    if (editor) onEditorReady?.(editor);
  }, [editor, onEditorReady]);

  useEffect(() => {
    if (!editor) return;
    const html = markdownToHtml(initialMarkdown);
    if (editor.getHTML() !== html) {
      editor.commands.setContent(html, false);
    }
  }, [initialMarkdown, editor]);

  return (
    <div className={cn("iris-editor flex-1 overflow-auto bg-editor-paper", className)}>
      <EditorContent editor={editor} />
    </div>
  );
}

export type { Editor } from "@tiptap/react";
