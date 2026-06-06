import CodeBlock from "@tiptap/extension-code-block";

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
import { Lock, LockOpen } from "lucide-react";

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type MouseEvent,
  type ReactNode,
} from "react";

import {
  clearCachedEditorHtml,
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

import {
  characterCountExcludingWhitespace,
  readingMinutes,
} from "@/lib/reading-time";

import { isTauriRuntime } from "@/lib/tauri-runtime";
import { cn } from "@/lib/utils";

import { AiSourceHighlightExtension } from "./extensions/AiSourceHighlightExtension";
import { AiStreamExtension } from "./extensions/AiStreamExtension";

import { HeadingFoldExtension } from "./extensions/HeadingFoldExtension";

import { EditorImageDropExtension } from "./extensions/EditorImageDropExtension";
import { FindHighlightExtension } from "./extensions/FindHighlightExtension";
import { ImageExtension } from "./extensions/ImageExtension";
import { IrisParagraphExtension } from "./extensions/IrisParagraphExtension";
import { ListIndentKeymapExtension } from "./extensions/ListIndentKeymapExtension";

import { IrisDocument } from "./extensions/IrisDocument";

import { LinkExtension } from "./extensions/LinkExtension";

import { CalloutBlockquoteExtension } from "./extensions/CalloutBlockquoteExtension";
import { PreserveBlockExtension } from "./extensions/PreserveBlockExtension";

import { SlashCommandExtension } from "./extensions/SlashCommandExtension";

import { WikiLinkExtension } from "./extensions/WikiLinkExtension";

const lowlight = createLowlight(common);

/** Status bar stats: avoid scanning the full doc on every keystroke. */

const BODY_STATS_DEBOUNCE_MS = 400;

/** Use lighter code blocks + fewer fold widgets above this body size. */

const LARGE_DOC_BODY_THRESHOLD = 12_000;

interface TipTapEditorProps {
  /** Body markdown only (frontmatter / document title are separate). */

  initialBodyMarkdown: string;

  /** When set, reuse cached TipTap HTML for this path (tab switch). */
  contentCacheKey?: string | null;

  /** Bumped when note content is loaded from disk (invalidates HTML cache). */
  reingestKey?: number;

  zen?: boolean;

  onDirty?: () => void;

  onSlashCommand?: (command: string) => void;

  onEditorReady?: (editor: Editor | null) => void;

  onBodyStatsChange?: (stats: {
    characterCount: number;

    readingMinutes: number;
  }) => void;

  onInlineAiRetry?: (editor: Editor) => void;

  onInlineAiDismiss?: (editor: Editor) => void;

  onInlineAiAccept?: (editor: Editor) => void;

  onOpenWikiLink?: (title: string) => void;

  zoom?: number;

  className?: string;

  /** Document title field rendered above body inside the shared editor canvas. */

  titleSlot?: ReactNode;

  /** 屏蔽原生右键并打开 Iris 菜单 */

  onBodyContextMenu?: (event: MouseEvent) => void;

  /** 编辑器 ingest 完成时回调，传递 preserve 片段信息供 export 使用 */

  onIngestComplete?: (
    preserveFragments: MarkdownSyntaxFragment[],

    originalBodyMd: string,
  ) => void;

  locked?: boolean;

  setLocked?: (locked: boolean) => void;
}

function TipTapEditorInner({
  initialBodyMarkdown,

  contentCacheKey = null,

  reingestKey = 0,

  zen = false,

  onDirty,

  onSlashCommand,

  onEditorReady,

  onBodyStatsChange,

  onInlineAiRetry,

  onInlineAiDismiss,

  onInlineAiAccept,

  onOpenWikiLink,

  onIngestComplete,

  zoom = 1,

  className,

  titleSlot,

  onBodyContextMenu,

  locked = false,

  setLocked,
}: TipTapEditorProps) {
  const inlineAiRetryRef = useRef(onInlineAiRetry);
  inlineAiRetryRef.current = onInlineAiRetry;
  const inlineAiDismissRef = useRef(onInlineAiDismiss);
  inlineAiDismissRef.current = onInlineAiDismiss;
  const inlineAiAcceptRef = useRef(onInlineAiAccept);
  inlineAiAcceptRef.current = onInlineAiAccept;

  const onDirtyRef = useRef(onDirty);

  onDirtyRef.current = onDirty;

  const onBodyStatsChangeRef = useRef(onBodyStatsChange);

  onBodyStatsChangeRef.current = onBodyStatsChange;

  const editorRef = useRef<Editor | null>(null);

  const onSlashCommandRef = useRef(onSlashCommand);

  onSlashCommandRef.current = onSlashCommand;

  const onOpenWikiLinkRef = useRef(onOpenWikiLink);

  onOpenWikiLinkRef.current = onOpenWikiLink;

  const onIngestCompleteRef = useRef(onIngestComplete);

  onIngestCompleteRef.current = onIngestComplete;

  const contentCacheKeyRef = useRef(contentCacheKey);
  contentCacheKeyRef.current = contentCacheKey;

  const bodyStatsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const htmlCacheTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flushBodyStats = useCallback((editor: Editor) => {
    if (bodyStatsTimerRef.current) {
      clearTimeout(bodyStatsTimerRef.current);

      bodyStatsTimerRef.current = null;
    }

    const text = editor.state.doc.textContent;

    onBodyStatsChangeRef.current?.({
      characterCount: characterCountExcludingWhitespace(text),

      readingMinutes: readingMinutes(text),
    });
  }, []);

  const scheduleBodyStats = useCallback((editor: Editor) => {
    if (bodyStatsTimerRef.current) clearTimeout(bodyStatsTimerRef.current);

    bodyStatsTimerRef.current = setTimeout(() => {
      bodyStatsTimerRef.current = null;

      const text = editor.state.doc.textContent;

      onBodyStatsChangeRef.current?.({
        characterCount: characterCountExcludingWhitespace(text),

        readingMinutes: readingMinutes(text),
      });
    }, BODY_STATS_DEBOUNCE_MS);
  }, []);

  const isLargeDoc =
    (initialBodyMarkdown?.length ?? 0) > LARGE_DOC_BODY_THRESHOLD;

  const extensions = useMemo(
    () => [
      IrisDocument,

      StarterKit.configure({
        document: false,

        paragraph: false,

        codeBlock: false,

        blockquote: false,

        history: {
          /** Smaller undo stack for very large notes (default depth is heavy). */

          depth: 80,
        },

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

      EditorImageDropExtension.configure({
        enabled: isTauriRuntime(),
      }),

      TaskList,

      TaskItem.configure({ nested: true }),

      Table.configure({ resizable: true }),

      TableRow,

      TableHeader,

      TableCell,

      isLargeDoc
        ? CodeBlock.configure({
            HTMLAttributes: { class: "iris-code-block" },
          })
        : CodeBlockLowlight.configure({ lowlight }),

      Placeholder.configure({
        placeholder: "开始写作，或输入 / 唤起 AI…",
      }),

      CalloutBlockquoteExtension,

      HeadingFoldExtension,

      PreserveBlockExtension,

      AiSourceHighlightExtension,

      AiStreamExtension.configure({
        onRetry: (ed) => inlineAiRetryRef.current?.(ed),
        onDismiss: (ed) => inlineAiDismissRef.current?.(ed),
        onAccept: (ed) => inlineAiAcceptRef.current?.(ed),
      }),

      SlashCommandExtension.configure({
        onCommand: (command) => onSlashCommandRef.current?.(command),
      }),

      WikiLinkExtension.configure({
        onOpenNote: (title) => onOpenWikiLinkRef.current?.(title),
      }),
    ],

    [isLargeDoc],
  );

  const ingestResultRef = useRef<{
    preserveFragments: MarkdownSyntaxFragment[];
    bodyMd: string;
  } | null>(null);

  const prevReingestKeyRef = useRef(reingestKey);
  const skipHtmlCache = prevReingestKeyRef.current !== reingestKey;
  prevReingestKeyRef.current = reingestKey;

  const initialContent = useMemo(() => {
    const bodyMd = initialBodyMarkdown.trim();
    if (contentCacheKey && bodyMd && !skipHtmlCache) {
      const cached = getCachedEditorHtml(contentCacheKey);
      if (cached) {
        return cached;
      }
    }

    const { tipTapHtml, preserveFragments } = ingestMarkdownForEditor({
      bodyMarkdown: bodyMd,
    });

    // Store for useEffect callback (avoid side effect in useMemo)
    ingestResultRef.current = { preserveFragments, bodyMd };

    if (contentCacheKey && bodyMd) {
      setCachedEditorHtml(contentCacheKey, tipTapHtml);
    }

    return tipTapHtml;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reingestKey busts HTML cache on disk reload
  }, [initialBodyMarkdown, contentCacheKey, reingestKey, skipHtmlCache]);

  // Fire onIngestComplete after render (not inside useMemo)
  useEffect(() => {
    const result = ingestResultRef.current;
    if (result) {
      ingestResultRef.current = null;
      onIngestCompleteRef.current?.(result.preserveFragments, result.bodyMd);
    }
  });

  // Clear HTML cache on disk reload so reingest uses fresh markdown
  useEffect(() => {
    if (reingestKey > 0 && contentCacheKey) {
      clearCachedEditorHtml(contentCacheKey);
    }
  }, [reingestKey, contentCacheKey]);

  const editor = useEditor({
    extensions,

    content: initialContent,

    immediatelyRender: true,

    editable: !locked,

    /** Avoid re-rendering this React tree on every keystroke (major lag on large docs). */

    shouldRerenderOnTransaction: false,

    onUpdate: ({ editor: updatedEditor }) => {
      onDirtyRef.current?.();

      scheduleBodyStats(updatedEditor);

      // Throttled HTML cache update (2s) so tab-switch shows latest content
      const key = contentCacheKeyRef.current;
      if (key) {
        if (htmlCacheTimerRef.current) clearTimeout(htmlCacheTimerRef.current);
        htmlCacheTimerRef.current = setTimeout(() => {
          htmlCacheTimerRef.current = null;
          setCachedEditorHtml(key, updatedEditor.getHTML());
        }, 2000);
      }
    },

    editorProps: {
      attributes: {
        class: "iris-markdown-content focus:outline-none",
        "data-prose-surface": "editor",
      },
    },
  });

  useEffect(() => {
    if (!editor) return;
    editor.setEditable(!locked);
  }, [editor, locked]);

  useEffect(() => {
    if (!editor) return;

    editorRef.current = editor;

    onEditorReady?.(editor);

    flushBodyStats(editor);

    return () => {
      flushBodyStats(editor);

      editorRef.current = null;

      onEditorReady?.(null);
    };
  }, [editor, onEditorReady, flushBodyStats]);

  useEffect(() => {
    return () => {
      if (bodyStatsTimerRef.current) {
        clearTimeout(bodyStatsTimerRef.current);

        bodyStatsTimerRef.current = null;
      }
      if (htmlCacheTimerRef.current) {
        clearTimeout(htmlCacheTimerRef.current);
        htmlCacheTimerRef.current = null;
      }
    };
  }, []);

  return (
    <div
      data-testid="editor"
      className={cn(
        "iris-editor relative flex min-h-0 flex-1 flex-col",
        className,
      )}
      data-zen={zen ? "true" : undefined}
      data-editor-zoom={zoom}
      data-locked={locked ? "true" : undefined}
    >
      {setLocked ? (
        <button
          type="button"
          data-testid="editor-lock-toggle"
          className="editor-lock-btn absolute right-3 top-3 z-10 inline-flex h-8 w-8 items-center justify-center rounded-md border border-border/60 bg-surface-elevated/90 text-muted-foreground shadow-sm backdrop-blur-sm duration-fast ease-iris-out hover:bg-surface-inset hover:text-foreground focus:outline-none focus:ring-2 focus:ring-primary focus:ring-offset-1"
          onClick={() => setLocked(!locked)}
          title={locked ? "解锁编辑" : "锁定编辑"}
          aria-label={locked ? "解锁编辑" : "锁定编辑"}
          aria-pressed={locked}
        >
          {locked ? (
            <Lock className="h-4 w-4" aria-hidden />
          ) : (
            <LockOpen className="h-4 w-4" aria-hidden />
          )}
        </button>
      ) : null}
      <div className="iris-editor-zoom-scroll min-h-0 flex-1 overflow-y-auto overflow-x-hidden">
        <div
          className="iris-editor-canvas"
          style={
            zoom !== 1
              ? ({ fontSize: `${zoom}rem` } as React.CSSProperties)
              : undefined
          }
        >
          {titleSlot ? (
            <div className="iris-editor-title-slot">{titleSlot}</div>
          ) : null}

          <div className="iris-editor-body" onContextMenu={onBodyContextMenu}>
            <EditorContent editor={editor} />
          </div>
        </div>
      </div>
    </div>
  );
}

export const TipTapEditor = memo(TipTapEditorInner);

TipTapEditor.displayName = "TipTapEditor";

export type { Editor } from "@tiptap/react";
