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

import { markdownToEditorHtml } from "@/lib/markdown";
import { cn } from "@/lib/utils";

import { AiStreamExtension } from "./extensions/AiStreamExtension";
import { HeadingFoldExtension } from "./extensions/HeadingFoldExtension";
import { IrisDocument } from "./extensions/IrisDocument";
import { NoteTitleExtension } from "./extensions/NoteTitleExtension";
import { SlashCommandExtension } from "./extensions/SlashCommandExtension";
import { WikiLinkExtension } from "./extensions/WikiLinkExtension";

const lowlight = createLowlight(common);

/** 将像素值对齐到编辑器行高网格（与 --editor-line-height 一致） */
function snapToEditorLineGrid(px: number, pm: HTMLElement): number {
  const lh = parseFloat(getComputedStyle(pm).lineHeight);
  if (!Number.isFinite(lh) || lh <= 0) return px;
  return Math.round(px / lh) * lh;
}

/** 主标题区底边相对 ProseMirror 顶部的距离（对齐网格），正文行线从此开始 */
function measureBodyLinesStart(paper: HTMLElement): number {
  const pm = paper.querySelector(".ProseMirror");
  const wrap = paper.querySelector(".iris-doc-title-wrap");
  if (!(pm instanceof HTMLElement) || !wrap) return 0;
  const pmRect = pm.getBoundingClientRect();
  const wrapRect = wrap.getBoundingClientRect();
  const raw = Math.max(0, wrapRect.bottom - pmRect.top);
  return snapToEditorLineGrid(raw, pm);
}

interface TipTapEditorProps {
  initialMarkdown: string;
  titleFallback?: string;
  zen?: boolean;
  onDirty?: () => void;
  onSlashCommand?: (command: string) => void;
  onEditorReady?: (editor: Editor) => void;
  onInlineAiRetry?: (editor: Editor) => void;
  onOpenWikiLink?: (title: string) => void;
  /** Visual scale of the paper (0.75–1.5). */
  zoom?: number;
  className?: string;
}

export function TipTapEditor({
  initialMarkdown,
  titleFallback = "",
  zen = false,
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

  const firedInitialRef = useRef(false);
  const paperRef = useRef<HTMLDivElement>(null);

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
    },
    editorProps: {
      attributes: {
        class: "focus:outline-none",
      },
    },
  });

  useEffect(() => {
    if (!editor) return;
    onEditorReady?.(editor);
    const first = editor.state.doc.firstChild;
    if (first?.type.name === "noteTitle" && first.content.size === 0) {
      editor.commands.focus("start");
    }
  }, [editor, onEditorReady]);

  useEffect(() => {
    const paper = paperRef.current;
    if (!paper || !editor) return;

    let rafId = 0;
    const syncLineOffset = () => {
      cancelAnimationFrame(rafId);
      rafId = requestAnimationFrame(() => {
        const start = measureBodyLinesStart(paper);
        paper.style.setProperty("--editor-body-lines-start", `${start}px`);
      });
    };

    syncLineOffset();

    const ro = new ResizeObserver(syncLineOffset);
    const titleWrap = paper.querySelector(".iris-doc-title-wrap");
    const proseMirror = paper.querySelector(".ProseMirror");
    if (titleWrap) ro.observe(titleWrap);
    if (proseMirror) ro.observe(proseMirror);

    return () => {
      cancelAnimationFrame(rafId);
      ro.disconnect();
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
          ref={paperRef}
          className="iris-paper"
          style={{ zoom } as React.CSSProperties}
        >
          <div className="iris-paper-sheet">
            <div className="iris-paper-gutter" aria-hidden="true" />
            <div className="iris-paper-body">
              <EditorContent editor={editor} />
            </div>
          </div>
        </div>
      </div>
    </div>
  );
}

export type { Editor } from "@tiptap/react";
