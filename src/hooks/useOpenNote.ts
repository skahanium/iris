import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useLayoutEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";

import type { DocumentPersistenceMoveResult } from "@/lib/document-persistence-coordinator";
import { ingestMarkdownForEditorAsync } from "@/lib/editor-ingest-async";
import { resetEditorContentBaseline } from "@/lib/editor-baseline";
import { EDITOR_PARSE_OPTIONS } from "@/lib/editor-parse-options";
import { documentRenameByTitle } from "@/lib/ipc";
import { extractFrontmatterYaml, parseNoteForEditor } from "@/lib/markdown";
import { pathStem } from "@/lib/note-display";
import {
  sanitizeDocumentTitleInput,
  NOTE_TITLE_HARD_LIMIT,
} from "@/lib/note-title-limits";
import {
  bodyMarkdownFromNoteRef,
  serializeOpenNote,
} from "@/lib/serialize-open-note";

function shouldSerializeEditorBody(
  editor: Editor | null,
  editorReady: boolean,
  dirty: boolean,
): boolean {
  if (!editor || editor.isDestroyed) return false;
  return editorReady || dirty;
}

interface UseOpenNoteOptions {
  activePath: string | null;
  /** Bumped when a note is read from disk into tab state (openFile); not bumped on save. */
  editorContentTick: number;
  activePathRef: RefObject<string | null>;
  markdownRef: RefObject<string>;
  frontmatterYamlRef: RefObject<string | null>;
  editorRef: RefObject<Editor | null>;
  editorReadyRef?: RefObject<boolean>;
  dirtyRef?: RefObject<boolean>;
  /** Flush layer-1 Markdown, then execute an atomic filesystem move. */
  renamePersistedPath?: (
    path: string,
    migrationPath: string,
    markdown: string,
    move: () => Promise<DocumentPersistenceMoveResult>,
  ) => Promise<string>;
  updateTabTitle: (path: string, title: string) => void;
  replaceOpenTabPath: (
    oldPath: string,
    newPath: string,
    title?: string,
    markdownOverride?: string,
  ) => void;
  onPathRenamed?: (oldPath: string, newPath: string) => void;
  onPathRenameError?: () => void;
}

/**
 * Owns the one editable title: the current Markdown filename. The title never
 * participates in Markdown serialization; blur/Enter serializes the latest
 * body, crosses the persistence barrier, and only then asks Rust to allocate
 * and move the filename.
 */
export function useOpenNote({
  activePath,
  editorContentTick,
  activePathRef,
  markdownRef,
  frontmatterYamlRef,
  editorRef,
  editorReadyRef,
  dirtyRef,
  renamePersistedPath,
  updateTabTitle,
  replaceOpenTabPath,
  onPathRenamed,
  onPathRenameError,
}: UseOpenNoteOptions) {
  const [noteTitle, setNoteTitle] = useState("");
  const [bodyMarkdown, setBodyMarkdown] = useState("");
  const noteTitleRef = useRef("");
  /** True while DocumentTitleField is focused — blocks disk-sync from stomping live edits. */
  const titleFocusedRef = useRef(false);
  const titleRenameGenerationRef = useRef(0);
  const titleRenameQueueRef = useRef<Promise<void>>(Promise.resolve());
  const editorIngestGenerationRef = useRef(0);

  /** Parsed body for TipTap on disk/tab load only; a save must not remount it. */
  const editorBodyMarkdown = useMemo(() => {
    if (!activePath) return "";
    return parseNoteForEditor(markdownRef.current, pathStem(activePath)).bodyMd;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- disk load tick is authoritative
  }, [activePath, editorContentTick, markdownRef]);

  const syncFromMarkdown = useCallback(
    (markdown: string, path: string) => {
      const parsed = parseNoteForEditor(markdown, pathStem(path));
      frontmatterYamlRef.current = parsed.yaml;
      // The path is the title authority, including when legacy frontmatter has one.
      const stem = pathStem(path);
      noteTitleRef.current = stem;
      setNoteTitle(stem);
      setBodyMarkdown(parsed.bodyMd);
    },
    [frontmatterYamlRef],
  );

  useLayoutEffect(() => {
    if (!activePath) {
      noteTitleRef.current = "";
      setNoteTitle("");
      setBodyMarkdown("");
      return;
    }
    // Do not overwrite a title the user is actively editing.
    if (titleFocusedRef.current) return;
    syncFromMarkdown(markdownRef.current, activePath);
  }, [activePath, editorContentTick, markdownRef, syncFromMarkdown]);

  const setTitleFocused = useCallback((focused: boolean) => {
    titleFocusedRef.current = focused;
  }, []);

  const getLiveMarkdown = useCallback(() => {
    const editor = editorRef.current;
    return serializeOpenNote({
      yaml: frontmatterYamlRef.current,
      editor,
      editorReady: shouldSerializeEditorBody(
        editor,
        editorReadyRef?.current ?? true,
        dirtyRef?.current ?? false,
      ),
      bodyFallbackMd: bodyMarkdownFromNoteRef(markdownRef.current),
    });
  }, [dirtyRef, editorReadyRef, editorRef, frontmatterYamlRef, markdownRef]);

  const applySavedMarkdown = useCallback(
    (markdown: string) => {
      markdownRef.current = markdown;
      frontmatterYamlRef.current = extractFrontmatterYaml(markdown);
    },
    [frontmatterYamlRef, markdownRef],
  );

  const onTitleChange = useCallback(
    (raw: string) => {
      if (!activePathRef.current) return;
      const next = sanitizeDocumentTitleInput(raw).slice(
        0,
        NOTE_TITLE_HARD_LIMIT,
      );
      noteTitleRef.current = next;
      // Do not call setNoteTitle while typing: the field is uncontrolled when
      // focused, and parent re-renders remount the editor surface / jump caret.
    },
    [activePathRef],
  );

  const commitTitleRename = useCallback(
    (title: string) => {
      const generation = ++titleRenameGenerationRef.current;
      const run = async () => {
        if (generation !== titleRenameGenerationRef.current) return;
        const oldPath = activePathRef.current;
        if (!oldPath) return;
        if (!title.trim()) {
          const restored = pathStem(oldPath);
          noteTitleRef.current = restored;
          setNoteTitle(restored);
          return;
        }

        const markdownSnapshot = getLiveMarkdown();
        let renamedPath = oldPath;
        const move = async (): Promise<DocumentPersistenceMoveResult> => {
          const receipt = await documentRenameByTitle(oldPath, title);
          renamedPath = receipt.entry.path;
          return {
            path: receipt.entry.path,
            indexDegraded: receipt.indexStatus === "degraded",
          };
        };

        try {
          // Keep the old path as the temporary migration identity. The Rust
          // command chooses the collision suffix only after the save barrier.
          const persistedMarkdown = renamePersistedPath
            ? await renamePersistedPath(
                oldPath,
                oldPath,
                markdownSnapshot,
                move,
              )
            : (await move(), markdownSnapshot);
          const committedTitle = pathStem(renamedPath);
          noteTitleRef.current = committedTitle;
          setNoteTitle(committedTitle);
          if (renamedPath === oldPath) {
            updateTabTitle(oldPath, committedTitle);
          } else {
            replaceOpenTabPath(
              oldPath,
              renamedPath,
              committedTitle,
              persistedMarkdown,
            );
            onPathRenamed?.(oldPath, renamedPath);
          }
        } catch {
          if (generation === titleRenameGenerationRef.current) {
            const restored = pathStem(oldPath);
            noteTitleRef.current = restored;
            setNoteTitle(restored);
            onPathRenameError?.();
          }
        } finally {
          // When an older rename fails after a newer one was queued, the older
          // catch skips restore. The latest generation must still align the
          // visible title with the authoritative path stem once it settles.
          if (
            generation === titleRenameGenerationRef.current &&
            !titleFocusedRef.current
          ) {
            const path = activePathRef.current;
            if (path) {
              const stem = pathStem(path);
              if (noteTitleRef.current !== stem) {
                noteTitleRef.current = stem;
                setNoteTitle(stem);
              }
            }
          }
        }
      };
      titleRenameQueueRef.current = titleRenameQueueRef.current.then(run, run);
    },
    [
      activePathRef,
      getLiveMarkdown,
      onPathRenamed,
      onPathRenameError,
      renamePersistedPath,
      replaceOpenTabPath,
      updateTabTitle,
    ],
  );

  const onTitleBlur = useCallback(
    (titleOverride?: string) => {
      const title = titleOverride ?? noteTitleRef.current;
      if (titleOverride !== undefined) {
        noteTitleRef.current = titleOverride;
      }
      setNoteTitle(title);
      const path = activePathRef.current;
      if (path && pathStem(path) === title.trim()) {
        return;
      }
      commitTitleRename(title);
    },
    [activePathRef, commitTitleRename],
  );

  const onTitleCancel = useCallback(() => {
    const path = activePathRef.current;
    if (!path) return;
    const restored = pathStem(path);
    noteTitleRef.current = restored;
    setNoteTitle(restored);
  }, [activePathRef]);

  const loadBodyIntoEditor = useCallback(
    (content: string) => {
      const path = activePathRef.current;
      if (!path) return;
      syncFromMarkdown(content, path);
      const parsed = parseNoteForEditor(content, pathStem(path));
      const editor = editorRef.current;
      if (!editor) return;
      const generation = ++editorIngestGenerationRef.current;
      void ingestMarkdownForEditorAsync({ bodyMarkdown: parsed.bodyMd })
        .then(({ tipTapHtml }) => {
          if (generation !== editorIngestGenerationRef.current) return;
          if (activePathRef.current !== path || dirtyRef?.current) return;
          const current = editorRef.current;
          if (!current) return;
          resetEditorContentBaseline(current, tipTapHtml, {
            parseOptions: EDITOR_PARSE_OPTIONS,
            selection: "preserve",
          });
        })
        .catch(() => {
          if (generation !== editorIngestGenerationRef.current) return;
          if (activePathRef.current !== path || dirtyRef?.current) return;
          const current = editorRef.current;
          if (!current) return;
          resetEditorContentBaseline(current, "<p></p>", {
            parseOptions: EDITOR_PARSE_OPTIONS,
            selection: "preserve",
          });
        });
    },
    [activePathRef, dirtyRef, editorRef, syncFromMarkdown],
  );

  return {
    noteTitle,
    bodyMarkdown,
    editorBodyMarkdown,
    getLiveMarkdown,
    applySavedMarkdown,
    onTitleChange,
    onTitleBlur,
    onTitleCancel,
    setTitleFocused,
    commitTitleRename,
    syncFromMarkdown,
    loadBodyIntoEditor,
  };
}
