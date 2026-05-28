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

import { useCallback, useEffect, useRef, useState } from "react";

import { useInlineSuggestion } from "@/hooks/useInlineSuggestion";
import { markdownToEditorHtml } from "@/lib/markdown";

import { noteTitleFromEditor } from "@/lib/note-title";

import { cn } from "@/lib/utils";

import { AiStreamExtension } from "./extensions/AiStreamExtension";
import { HeadingFoldExtension } from "./extensions/HeadingFoldExtension";
import { InlineAiExtension } from "./extensions/InlineAiExtension";
import { InlineSuggestion } from "./InlineSuggestion";
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

  /** Enable inline AI suggestions (GitHub Copilot style autocomplete). */

  enableInlineSuggestion?: boolean;

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

  enableInlineSuggestion = false,

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

  const {
    suggestion,
    isLoading,
    fetchSuggestion,
    acceptSuggestion,
    dismissSuggestion,
  } = useInlineSuggestion();

  const [suggestionPos, setSuggestionPos] = useState<{
    top: number;
    left: number;
  } | null>(null);

  const editorRef = useRef<Editor | null>(null);

  const enableSuggestionRef = useRef(enableInlineSuggestion);
  enableSuggestionRef.current = enableInlineSuggestion;

  const handleEditorUpdate = useCallback(() => {
    const ed = editorRef.current;
    if (!ed || !enableSuggestionRef.current) return;

    const { from } = ed.state.selection;
    const textBefore = ed.state.doc.textBetween(
      Math.max(0, from - 200),
      from,
      "\n",
    );

    if (textBefore.trim().length > 10) {
      fetchSuggestion(textBefore, from);

      const coords = ed.view.coordsAtPos(from);
      setSuggestionPos({
        top: coords.bottom,
        left: coords.left,
      });
    }
  }, [fetchSuggestion]);

  const handleAcceptSuggestion = useCallback(() => {
    const ed = editorRef.current;
    if (ed && suggestion) {
      ed.commands.insertContent(suggestion.text);
    }
    acceptSuggestion();
    setSuggestionPos(null);
  }, [suggestion, acceptSuggestion]);

  const handleDismissSuggestion = useCallback(() => {
    dismissSuggestion();
    setSuggestionPos(null);
  }, [dismissSuggestion]);

  const handleEditorUpdateRef = useRef(handleEditorUpdate);
  handleEditorUpdateRef.current = handleEditorUpdate;

  const editor = useEditor({
    extensions: [
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
      InlineAiExtension,
      SlashCommandExtension.configure({ onCommand: onSlashCommand }),
      WikiLinkExtension.configure({ onOpenNote: onOpenWikiLink }),
    ],

    content: markdownToEditorHtml(initialMarkdown, titleFallback),

    onUpdate: () => {
      if (!firedInitialRef.current) {
        firedInitialRef.current = true;

        return;
      }

      onDirtyRef.current?.();
      handleEditorUpdateRef.current();
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

    const emitTitle = () => {
      const next = noteTitleFromEditor(editor);

      if (next === lastEmittedTitleRef.current) return;

      lastEmittedTitleRef.current = next;

      onTitleChangeRef.current?.(next);
    };

    emitTitle();

    const onTransaction = ({
      transaction,
    }: {
      transaction: { docChanged: boolean };
    }) => {
      if (transaction.docChanged) emitTitle();
    };

    editor.on("transaction", onTransaction);

    return () => {
      editor.off("transaction", onTransaction);
    };
  }, [editor]);

  return (
    <div
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

      {suggestion && suggestionPos && (
        <div
          className="pointer-events-auto"
          style={{
            position: "fixed",
            top: suggestionPos.top,
            left: suggestionPos.left,
            zIndex: 50,
          }}
        >
          <InlineSuggestion
            suggestion={suggestion}
            onAccept={handleAcceptSuggestion}
            onDismiss={handleDismissSuggestion}
          />
        </div>
      )}

      {isLoading && enableInlineSuggestion && (
        <div className="fixed bottom-4 right-4 z-50">
          <div className="rounded-full bg-primary/10 px-3 py-1.5 text-xs text-primary">
            AI 思考中…
          </div>
        </div>
      )}
    </div>
  );
}

export type { Editor } from "@tiptap/react";
