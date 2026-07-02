import CodeBlock from "@tiptap/extension-code-block";
import Placeholder from "@tiptap/extension-placeholder";

import Table from "@tiptap/extension-table";

import TableCell from "@tiptap/extension-table-cell";

import TableHeader from "@tiptap/extension-table-header";

import TableRow from "@tiptap/extension-table-row";

import TaskItem from "@tiptap/extension-task-item";

import TaskList from "@tiptap/extension-task-list";

import { EditorContent, useEditor, type Editor } from "@tiptap/react";

import StarterKit from "@tiptap/starter-kit";

import { Lock, LockOpen } from "lucide-react";

import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MouseEvent,
  type ReactNode,
} from "react";

import {
  clearCachedEditorHtml,
  editorHtmlDigest,
  getCachedEditorHtml,
  setCachedEditorHtml,
} from "@/lib/editor-html-cache";
import type { EditorHtmlCacheNamespace } from "@/lib/editor-html-cache";
import {
  ingestMarkdownForEditor,
  type EditorIngestResult,
} from "@/lib/editor-ingest";
import {
  EDITOR_INGEST_WORKER_THRESHOLD_BYTES,
  ingestMarkdownForEditorAsync,
} from "@/lib/editor-ingest-async";
import { EDITOR_PARSE_OPTIONS } from "@/lib/editor-parse-options";
import { normalizePastedEditorHtml } from "@/lib/iris-clipboard";

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
import { HeadingFoldOverlay } from "./HeadingFoldOverlay";

import { EditorImageDropExtension } from "./extensions/EditorImageDropExtension";
import { FindHighlightExtension } from "./extensions/FindHighlightExtension";
import {
  FootnoteDefExtension,
  FootnoteRefExtension,
} from "./extensions/FootnoteExtension";
import { ImageExtension } from "./extensions/ImageExtension";
import { ImeCompositionGuardExtension } from "./extensions/ImeCompositionGuardExtension";
import { IrisParagraphExtension } from "./extensions/IrisParagraphExtension";
import { ListIndentKeymapExtension } from "./extensions/ListIndentKeymapExtension";

import { IrisDocument } from "./extensions/IrisDocument";

import { isSafeHref, LinkExtension } from "./extensions/LinkExtension";

import { CalloutBlockquoteExtension } from "./extensions/CalloutBlockquoteExtension";
import { PreserveBlockExtension } from "./extensions/PreserveBlockExtension";
import { PreserveInlineExtension } from "./extensions/PreserveInlineExtension";

import { SlashCommandExtension } from "./extensions/SlashCommandExtension";

import { WikiLinkExtension } from "./extensions/WikiLinkExtension";
import { WikiMediaEmbedExtension } from "./extensions/WikiMediaEmbedExtension";

/** Status bar stats: avoid scanning the full doc on every keystroke. */

const BODY_STATS_DEBOUNCE_MS = 400;

const LIGHT_CODE_BLOCK_EXTENSION = CodeBlock.configure({
  HTMLAttributes: { class: "iris-code-block" },
});

interface TipTapEditorProps {
  /** Body markdown only (frontmatter / document title are separate). */

  initialBodyMarkdown: string;

  /** Already-ingested TipTap HTML prepared before this editor becomes visible. */
  initialEditorHtml?: string | null;

  /** When set, reuse cached TipTap HTML for this path (tab switch). */
  contentCacheKey?: string | null;

  /** Separates normal and classified prepared editor HTML. */
  contentCacheNamespace?: EditorHtmlCacheNamespace;

  /** Absolute vault path used only to render vault-relative asset URLs. */
  vaultPath?: string | null;

  /** Bumped when note content is loaded from disk (invalidates HTML cache). */
  reingestKey?: number;

  zen?: boolean;

  onDirty?: () => void;

  onSlashCommand?: (command: string) => void;

  onEditorReady?: (editor: Editor | null) => void;

  onFirstFrameReady?: (editor: Editor) => void;

  onContentReady?: (editor: Editor) => void;

  onBodyStatsChange?: (stats: {
    characterCount: number;

    readingMinutes: number;
  }) => void;

  onInlineAiRetry?: (editor: Editor) => void;

  onInlineAiDismiss?: (editor: Editor) => void;

  onInlineAiAccept?: (editor: Editor) => void;

  onOpenWikiLink?: (title: string) => void;

  onPrepareWikiLink?: (title: string) => void;

  zoom?: number;

  className?: string;

  /** Document title field rendered above body inside the shared editor canvas. */

  titleSlot?: ReactNode;

  /** 屏蔽原生右键并打开 Iris 菜单 */

  onBodyContextMenu?: (event: MouseEvent) => void;

  onIngestComplete?: (
    preserveFragments: MarkdownSyntaxFragment[],

    originalBodyMd: string,
  ) => void;

  locked?: boolean;

  mediaLoading?: "deferred" | "visible";

  setLocked?: (locked: boolean) => void;
}

function TipTapEditorInner({
  initialBodyMarkdown,

  initialEditorHtml = null,

  contentCacheKey = null,

  contentCacheNamespace = "normal",

  vaultPath = null,

  reingestKey = 0,

  zen = false,

  onDirty,

  onSlashCommand,

  onEditorReady,

  onFirstFrameReady,

  onContentReady,

  onBodyStatsChange,

  onInlineAiRetry,

  onInlineAiDismiss,

  onInlineAiAccept,

  onOpenWikiLink,

  onPrepareWikiLink,

  onIngestComplete,

  zoom = 1,

  className,

  titleSlot,

  onBodyContextMenu,

  locked = false,

  mediaLoading = "visible",

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
  const [linkEditor, setLinkEditor] = useState<{
    href: string;
    error: string | null;
    from: number;
    to: number;
  } | null>(null);

  const onSlashCommandRef = useRef(onSlashCommand);

  onSlashCommandRef.current = onSlashCommand;

  const onOpenWikiLinkRef = useRef(onOpenWikiLink);

  onOpenWikiLinkRef.current = onOpenWikiLink;

  const onPrepareWikiLinkRef = useRef(onPrepareWikiLink);

  onPrepareWikiLinkRef.current = onPrepareWikiLink;

  const onIngestCompleteRef = useRef(onIngestComplete);

  onIngestCompleteRef.current = onIngestComplete;

  const onFirstFrameReadyRef = useRef(onFirstFrameReady);

  onFirstFrameReadyRef.current = onFirstFrameReady;

  const onContentReadyRef = useRef(onContentReady);

  onContentReadyRef.current = onContentReady;

  const contentCacheKeyRef = useRef(contentCacheKey);
  contentCacheKeyRef.current = contentCacheKey;
  const contentCacheNamespaceRef = useRef(contentCacheNamespace);
  contentCacheNamespaceRef.current = contentCacheNamespace;

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

      ImeCompositionGuardExtension,

      IrisParagraphExtension,

      ListIndentKeymapExtension,

      FindHighlightExtension,

      LinkExtension,

      ImageExtension.configure({
        mediaLoading,
        vaultPath,
      }),

      WikiMediaEmbedExtension.configure({
        mediaLoading,
        vaultPath,
      }),

      EditorImageDropExtension.configure({
        enabled: isTauriRuntime(),
      }),

      TaskList,

      TaskItem.configure({ nested: true }),

      Table.configure({ resizable: true }),

      TableRow,

      TableHeader,

      TableCell,

      LIGHT_CODE_BLOCK_EXTENSION,

      Placeholder.configure({
        placeholder: "开始写作，或输入 / 唤起 AI…",
      }),

      CalloutBlockquoteExtension,

      HeadingFoldExtension,

      PreserveBlockExtension,

      PreserveInlineExtension,

      FootnoteRefExtension,

      FootnoteDefExtension,

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
        onPrepareNote: (title) => onPrepareWikiLinkRef.current?.(title),
      }),
    ],

    [mediaLoading, vaultPath],
  );

  const ingestResultRef = useRef<{
    preserveFragments: MarkdownSyntaxFragment[];
    bodyMd: string;
  } | null>(null);

  const prevReingestKeyRef = useRef(reingestKey);
  const skipHtmlCache = prevReingestKeyRef.current !== reingestKey;
  prevReingestKeyRef.current = reingestKey;

  const parsedContentRef = useRef<string | null>(null);
  const parsedContentRevisionRef = useRef(0);
  const [parsedContentRevision, setParsedContentRevision] = useState(0);
  const contentReadyRef = useRef(false);
  const firstFrameGenerationRef = useRef(0);

  useEffect(() => {
    const bodyMd = initialBodyMarkdown.trim();
    const htmlDigest = editorHtmlDigest(initialBodyMarkdown);
    let cancelled = false;
    contentReadyRef.current = false;

    const setContent = (
      html: string,
      fragments?: MarkdownSyntaxFragment[],
      bodyMdParam?: string,
    ) => {
      if (cancelled) return;
      parsedContentRef.current = html;
      parsedContentRevisionRef.current += 1;
      setParsedContentRevision(parsedContentRevisionRef.current);
      if (fragments && bodyMdParam) {
        ingestResultRef.current = {
          preserveFragments: fragments,
          bodyMd: bodyMdParam,
        };
      }
    };

    if (initialEditorHtml) {
      if (contentCacheKey) {
        setCachedEditorHtml(
          contentCacheKey,
          initialEditorHtml,
          htmlDigest,
          contentCacheNamespace,
        );
      }
      setContent(initialEditorHtml);
      return;
    }

    if (!bodyMd) {
      setContent("<p></p>", [], bodyMd);
      return;
    }

    const rememberIngestResult = (result: EditorIngestResult) => {
      const html = result.tipTapHtml || "<p></p>";
      setContent(html, result.preserveFragments, bodyMd);
      if (contentCacheKey) {
        setCachedEditorHtml(
          contentCacheKey,
          html,
          htmlDigest,
          contentCacheNamespace,
        );
      }
    };

    if (contentCacheKey && bodyMd && !skipHtmlCache) {
      const cached = getCachedEditorHtml(
        contentCacheKey,
        htmlDigest,
        contentCacheNamespace,
      );
      if (cached) {
        setContent(cached);
        return;
      }
    }

    if (contentCacheKey && bodyMd && skipHtmlCache) {
      clearCachedEditorHtml(contentCacheKey, contentCacheNamespace);
    }

    if (bodyMd.length <= EDITOR_INGEST_WORKER_THRESHOLD_BYTES) {
      try {
        rememberIngestResult(ingestMarkdownForEditor({ bodyMarkdown: bodyMd }));
      } catch {
        contentReadyRef.current = false;
      }
      return;
    }

    ingestMarkdownForEditorAsync({ bodyMarkdown: bodyMd })
      .then((result) => {
        if (cancelled) return;
        rememberIngestResult(result);
      })
      .catch(() => {
        if (!cancelled) contentReadyRef.current = false;
      });

    return () => {
      cancelled = true;
    };
  }, [
    initialBodyMarkdown,
    initialEditorHtml,
    contentCacheKey,
    contentCacheNamespace,
    reingestKey,
    skipHtmlCache,
  ]);

  // Fire onIngestComplete after render (not inside async callback)
  useEffect(() => {
    const result = ingestResultRef.current;
    if (result) {
      ingestResultRef.current = null;
      onIngestCompleteRef.current?.(result.preserveFragments, result.bodyMd);
    }
  });

  const editor = useEditor(
    {
      extensions,

      content: "<p></p>",

      parseOptions: EDITOR_PARSE_OPTIONS,

      immediatelyRender: true,

      editable: !locked,

      shouldRerenderOnTransaction: false,

      onUpdate: ({ editor: updatedEditor }) => {
        onDirtyRef.current?.();

        scheduleBodyStats(updatedEditor);

        const key = contentCacheKeyRef.current;
        const namespace = contentCacheNamespaceRef.current;
        if (key) {
          const htmlDigest = editorHtmlDigest(initialBodyMarkdown);
          if (htmlCacheTimerRef.current)
            clearTimeout(htmlCacheTimerRef.current);
          htmlCacheTimerRef.current = setTimeout(() => {
            htmlCacheTimerRef.current = null;
            setCachedEditorHtml(
              key,
              updatedEditor.getHTML(),
              htmlDigest,
              namespace,
            );
          }, 2000);
        }
      },

      editorProps: {
        transformPastedHTML: normalizePastedEditorHtml,

        attributes: {
          class: "iris-markdown-content focus:outline-none",
          "data-prose-surface": "editor",
        },
      },
    },
    [extensions],
  );

  useEffect(() => {
    if (!editor) return;
    editor.setEditable(!locked);
  }, [editor, locked]);

  // Apply parsed content to editor when ready
  useEffect(() => {
    if (!editor || editor.isDestroyed) return;
    const content = parsedContentRef.current;
    if (!content) return undefined;

    let cancelled = false;
    let firstFrameId: number | null = null;
    let secondFrameId: number | null = null;
    const requestFrame =
      window.requestAnimationFrame ??
      ((cb: FrameRequestCallback) =>
        window.setTimeout(() => cb(performance.now()), 16));
    const cancelFrame =
      window.cancelAnimationFrame ?? ((id: number) => window.clearTimeout(id));
    const generation = ++firstFrameGenerationRef.current;

    editor.commands.setContent(content, false, EDITOR_PARSE_OPTIONS);
    contentReadyRef.current = true;
    onContentReadyRef.current?.(editor);
    flushBodyStats(editor);

    firstFrameId = requestFrame(() => {
      secondFrameId = requestFrame(() => {
        if (!cancelled && firstFrameGenerationRef.current === generation) {
          onFirstFrameReadyRef.current?.(editor);
        }
      });
    });

    return () => {
      cancelled = true;
      if (firstFrameId !== null) cancelFrame(firstFrameId);
      if (secondFrameId !== null) cancelFrame(secondFrameId);
    };
  }, [editor, flushBodyStats, parsedContentRevision]);

  const openLinkEditor = useCallback(
    (targetEditor: Editor) => {
      if (locked || !targetEditor.isEditable) return;
      const href = targetEditor.getAttributes("link").href;
      const { from, to } = targetEditor.state.selection;
      setLinkEditor({
        href: typeof href === "string" ? href : "",
        error: null,
        from,
        to,
      });
    },
    [locked],
  );

  useEffect(() => {
    if (!editor) return;
    const dom = editor.view.dom;
    const handleOpenLinkEditor = (event: Event) => {
      event.preventDefault();
      openLinkEditor(editor);
    };
    dom.addEventListener("iris-open-link-editor", handleOpenLinkEditor);
    return () => {
      dom.removeEventListener("iris-open-link-editor", handleOpenLinkEditor);
    };
  }, [editor, openLinkEditor]);

  const closeLinkEditor = useCallback(() => {
    setLinkEditor(null);
  }, []);

  const applyLinkEditor = useCallback(() => {
    if (!editor || !linkEditor) return;
    const href = linkEditor.href.trim();
    if (!href) {
      editor
        .chain()
        .focus()
        .setTextSelection({ from: linkEditor.from, to: linkEditor.to })
        .extendMarkRange("link")
        .unsetLink()
        .run();
      setLinkEditor(null);
      return;
    }
    if (!isSafeHref(href)) {
      setLinkEditor((state) =>
        state ? { ...state, error: "链接协议不安全" } : state,
      );
      return;
    }
    editor
      .chain()
      .focus()
      .setTextSelection({ from: linkEditor.from, to: linkEditor.to })
      .extendMarkRange("link")
      .setLink({ href })
      .run();
    setLinkEditor(null);
  }, [editor, linkEditor]);

  const removeLinkEditor = useCallback(() => {
    if (!editor || !linkEditor) return;
    editor
      .chain()
      .focus()
      .setTextSelection({ from: linkEditor.from, to: linkEditor.to })
      .extendMarkRange("link")
      .unsetLink()
      .run();
    setLinkEditor(null);
  }, [editor, linkEditor]);

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
      style={{ "--editor-zoom": String(zoom) } as React.CSSProperties}
    >
      {setLocked ? (
        <button
          type="button"
          data-testid="editor-lock-toggle"
          className="iris-focus-soft editor-edge-control editor-lock-btn absolute right-3 top-3 z-10 inline-flex h-8 w-8 items-center justify-center rounded-md border border-border/60 bg-surface-elevated/90 text-muted-foreground shadow-sm backdrop-blur-sm duration-fast ease-iris-out hover:bg-surface-inset hover:text-foreground focus:outline-none"
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
        <div className="iris-editor-canvas">
          {titleSlot ? (
            <div className="iris-editor-title-slot">{titleSlot}</div>
          ) : null}

          <div className="iris-editor-body" onContextMenu={onBodyContextMenu}>
            <EditorContent editor={editor} />
            <HeadingFoldOverlay editor={editor} />
            {linkEditor ? (
              <div
                data-testid="editor-link-popover"
                className="absolute left-1/2 top-24 z-20 w-[min(24rem,calc(100%-2rem))] -translate-x-1/2 rounded-md border border-border/70 bg-popover p-3 text-popover-foreground shadow-floating"
                role="dialog"
                aria-label="编辑链接"
                onKeyDown={(event) => {
                  if (event.key === "Escape") closeLinkEditor();
                  if (event.key === "Enter") applyLinkEditor();
                }}
              >
                <label className="block text-xs font-medium text-foreground">
                  链接 URL
                  <input
                    data-testid="editor-link-url-input"
                    className="mt-2 h-8 w-full rounded-sm border border-border/70 bg-background px-2 text-xs text-foreground outline-none transition-colors focus:border-primary"
                    value={linkEditor.href}
                    autoFocus
                    placeholder="https://example.com"
                    onChange={(event) => {
                      const nextHref = event.currentTarget.value;
                      setLinkEditor((state) => ({
                        href: nextHref,
                        error: null,
                        from: state?.from ?? 1,
                        to: state?.to ?? 1,
                      }));
                    }}
                  />
                </label>
                {linkEditor.error ? (
                  <p className="mt-2 text-xs text-destructive">
                    {linkEditor.error}
                  </p>
                ) : null}
                <div className="mt-3 flex items-center justify-end gap-2">
                  <button
                    type="button"
                    data-testid="editor-link-remove"
                    className="rounded-sm px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                    onClick={removeLinkEditor}
                  >
                    清除链接
                  </button>
                  <button
                    type="button"
                    className="rounded-sm px-2 py-1 text-xs text-muted-foreground hover:bg-muted hover:text-foreground"
                    onClick={closeLinkEditor}
                  >
                    取消
                  </button>
                  <button
                    type="button"
                    data-testid="editor-link-apply"
                    className="rounded-sm bg-primary px-2 py-1 text-xs font-medium text-primary-foreground hover:bg-primary/90"
                    onClick={applyLinkEditor}
                  >
                    应用
                  </button>
                </div>
              </div>
            ) : null}
          </div>
        </div>
      </div>
    </div>
  );
}

export const TipTapEditor = memo(TipTapEditorInner);

TipTapEditor.displayName = "TipTapEditor";

export type { Editor } from "@tiptap/react";
