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
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";
import { ingestMarkdownForEditor } from "@/lib/editor-ingest";

import type { MarkdownSyntaxFragment } from "@/lib/markdown-contract/types";

import {

  characterCountExcludingWhitespace,

  readingMinutes,

} from "@/lib/reading-time";

import { cn } from "@/lib/utils";



import { AiStreamExtension } from "./extensions/AiStreamExtension";

import { HeadingFoldExtension } from "./extensions/HeadingFoldExtension";

import { ImageExtension } from "./extensions/ImageExtension";

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

  onOpenWikiLink,

  onIngestComplete,

  zoom = 1,

  className,

  titleSlot,

  onBodyContextMenu,

}: TipTapEditorProps) {

  const inlineAiRetryRef = useRef(onInlineAiRetry);

  inlineAiRetryRef.current = onInlineAiRetry;



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



  const bodyStatsTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);



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

      LinkExtension,

      ImageExtension,

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

    [isLargeDoc],

  );



  const initialContent = useMemo(() => {
    if (contentCacheKey) {
      const cached = getCachedEditorHtml(contentCacheKey);
      if (cached) return cached;
    }

    const { tipTapHtml, preserveFragments } = ingestMarkdownForEditor({
      bodyMarkdown: initialBodyMarkdown || "",
    });

    onIngestCompleteRef.current?.(preserveFragments, initialBodyMarkdown || "");

    if (contentCacheKey) {
      setCachedEditorHtml(contentCacheKey, tipTapHtml);
    }

    return tipTapHtml;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- reingestKey busts HTML cache on disk reload
  }, [initialBodyMarkdown, contentCacheKey, reingestKey]);



  const editor = useEditor({

    extensions,

    content: initialContent,

    immediatelyRender: true,

    /** Avoid re-rendering this React tree on every keystroke (major lag on large docs). */

    shouldRerenderOnTransaction: false,

    onUpdate: ({ editor: updatedEditor }) => {

      onDirtyRef.current?.();

      scheduleBodyStats(updatedEditor);

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

    flushBodyStats(editor);

    return () => {

      flushBodyStats(editor);

      editorRef.current = null;

      onEditorReady?.(null);

    };

  }, [editor, onEditorReady, flushBodyStats]);



  const lastSyncedContentRef = useRef(initialContent);



  useEffect(() => {

    if (!editor) return;

    if (lastSyncedContentRef.current === initialContent) return;

    lastSyncedContentRef.current = initialContent;

    editor.commands.setContent(initialContent, false);

    flushBodyStats(editor);

  }, [editor, initialContent, flushBodyStats]);



  useEffect(() => {

    return () => {

      if (bodyStatsTimerRef.current) {

        clearTimeout(bodyStatsTimerRef.current);

        bodyStatsTimerRef.current = null;

      }

    };

  }, []);



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

