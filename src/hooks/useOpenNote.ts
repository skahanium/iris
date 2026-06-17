import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type RefObject,
} from "react";

import { pathStem } from "@/lib/note-display";
import { ingestMarkdownForEditorAsync } from "@/lib/editor-ingest-async";
import { extractFrontmatterYaml, parseNoteForEditor } from "@/lib/markdown";
import { isPlaceholderTitle } from "@/lib/path-sync";
import { fileRename, pathSyncSuggest } from "@/lib/ipc";
import {
  sanitizeDocumentTitleInput,
  NOTE_TITLE_HARD_LIMIT,
} from "@/lib/note-title-limits";
import {
  bodyMarkdownFromNoteRef,
  serializeOpenNote,
} from "@/lib/serialize-open-note";

const PATH_SYNC_DEBOUNCE_MS = 800;

interface UseOpenNoteOptions {
  activePath: string | null;
  /** Bumped when a note is read from disk into tab state (openFile); not bumped on save. */
  editorContentTick: number;
  activePathRef: RefObject<string | null>;
  markdownRef: RefObject<string>;
  frontmatterYamlRef: RefObject<string | null>;
  editorRef: RefObject<Editor | null>;
  dirtyRef?: RefObject<boolean>;
  updateTabTitle: (path: string, title: string) => void;
  replaceOpenTabPath: (
    oldPath: string,
    newPath: string,
    title?: string,
    markdownOverride?: string,
  ) => void;
}

export function useOpenNote({
  activePath,
  editorContentTick,
  activePathRef,
  markdownRef,
  frontmatterYamlRef,
  editorRef,
  dirtyRef,
  updateTabTitle,
  replaceOpenTabPath,
}: UseOpenNoteOptions) {
  const [noteTitle, setNoteTitle] = useState("");
  const [bodyMarkdown, setBodyMarkdown] = useState("");

  /** Parsed body for TipTap on disk/tab load only — not on layer-1 save (`setMarkdown` must not remount editor). */
  const editorBodyMarkdown = useMemo(() => {
    if (!activePath) return "";
    return parseNoteForEditor(markdownRef.current, pathStem(activePath)).bodyMd;
    // eslint-disable-next-line react-hooks/exhaustive-deps -- editorContentTick = disk load; omit `markdown` so save does not call setContent
  }, [activePath, editorContentTick, markdownRef]);

  const pathSyncTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pathSyncGenRef = useRef(0);
  const editorIngestGenerationRef = useRef(0);
  const syncFromMarkdown = useCallback(
    (md: string, _path: string) => {
      const parsed = parseNoteForEditor(md, pathStem(_path));
      frontmatterYamlRef.current = parsed.yaml;
      setNoteTitle(parsed.title);
      setBodyMarkdown(parsed.bodyMd);
    },
    [frontmatterYamlRef],
  );

  useEffect(() => {
    if (!activePath) {
      setNoteTitle("");
      setBodyMarkdown("");
      return;
    }
    syncFromMarkdown(markdownRef.current, activePath);
    // `markdown` state intentionally omitted: layer-1 save only updates `markdownRef`.
  }, [activePath, editorContentTick, syncFromMarkdown, markdownRef]);

  useEffect(() => {
    return () => {
      if (pathSyncTimerRef.current) {
        clearTimeout(pathSyncTimerRef.current);
      }
    };
  }, []);

  const getLiveMarkdown = useCallback(() => {
    return serializeOpenNote({
      yaml: frontmatterYamlRef.current,
      title: noteTitle,
      editor: editorRef.current,
      bodyFallbackMd: bodyMarkdownFromNoteRef(markdownRef.current),
    });
  }, [noteTitle, frontmatterYamlRef, editorRef, markdownRef]);

  const applySavedMarkdown = useCallback(
    (md: string) => {
      markdownRef.current = md;
      frontmatterYamlRef.current = extractFrontmatterYaml(md);
      const path = activePathRef.current;
      if (path) {
        const parsed = parseNoteForEditor(md, pathStem(path));
        setNoteTitle(parsed.title);
      }
    },
    [activePathRef, frontmatterYamlRef, markdownRef],
  );

  const onTitleChange = useCallback(
    (raw: string) => {
      const path = activePathRef.current;
      if (!path) return;

      const next = sanitizeDocumentTitleInput(raw).slice(
        0,
        NOTE_TITLE_HARD_LIMIT,
      );
      setNoteTitle(next);
      updateTabTitle(path, next);
    },
    [activePathRef, updateTabTitle],
  );

  const schedulePathSync = useCallback(
    (path: string, title: string) => {
      if (pathSyncTimerRef.current) {
        clearTimeout(pathSyncTimerRef.current);
      }
      if (isPlaceholderTitle(title)) {
        return;
      }

      const generation = ++pathSyncGenRef.current;
      pathSyncTimerRef.current = setTimeout(() => {
        pathSyncTimerRef.current = null;
        void pathSyncSuggest(path, title)
          .then((suggest) => {
            if (generation !== pathSyncGenRef.current) return;
            if (activePathRef.current !== path) return;
            if (!suggest.needs_sync || suggest.suggested_path === path) {
              return;
            }
            const msg = suggest.conflict_resolved
              ? `路径「${suggest.suggested_path}」已避开同名冲突。是否同步？`
              : `是否将文件路径同步为「${suggest.suggested_path}」？`;
            if (!window.confirm(msg)) return;
            return fileRename(path, suggest.suggested_path).then((entry) => {
              const allocatedTitle = pathStem(entry.path);
              const nextTitle =
                title.trim() === "" ? allocatedTitle : title.trim();
              const liveMarkdown = serializeOpenNote({
                yaml: frontmatterYamlRef.current,
                title: nextTitle,
                editor: editorRef.current,
                bodyFallbackMd: bodyMarkdownFromNoteRef(markdownRef.current),
              });
              replaceOpenTabPath(path, entry.path, nextTitle, liveMarkdown);
              if (title.trim() === "") {
                setNoteTitle(allocatedTitle);
              }
            });
          })
          .catch(() => {
            /* 路径同步为可选增强 */
          });
      }, PATH_SYNC_DEBOUNCE_MS);
    },
    [
      activePathRef,
      editorRef,
      frontmatterYamlRef,
      markdownRef,
      replaceOpenTabPath,
    ],
  );

  const onTitleBlur = useCallback(() => {
    const path = activePathRef.current;
    if (!path) return;
    schedulePathSync(path, noteTitle);
  }, [activePathRef, noteTitle, schedulePathSync]);

  const loadBodyIntoEditor = useCallback(
    (content: string) => {
      const path = activePathRef.current;
      if (!path) return;
      syncFromMarkdown(content, path);
      const parsed = parseNoteForEditor(content, pathStem(path));
      if (editorRef.current) {
        const generation = ++editorIngestGenerationRef.current;
        void ingestMarkdownForEditorAsync({ bodyMarkdown: parsed.bodyMd })
          .then(({ tipTapHtml }) => {
            if (generation !== editorIngestGenerationRef.current) return;
            if (activePathRef.current !== path) return;
            if (dirtyRef?.current) return;
            editorRef.current?.commands.setContent(tipTapHtml, false);
          })
          .catch(() => {
            if (generation !== editorIngestGenerationRef.current) return;
            if (activePathRef.current !== path) return;
            if (dirtyRef?.current) return;
            editorRef.current?.commands.setContent("<p></p>", false);
          });
      }
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
    schedulePathSync,
    syncFromMarkdown,
    loadBodyIntoEditor,
  };
}
