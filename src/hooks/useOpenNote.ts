import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useRef,
  useState,
  type RefObject,
} from "react";

import { resolveNoteDisplayTitle } from "@/lib/note-display";
import { displayTitleFromMarkdown } from "@/lib/document-title";
import {
  extractFrontmatterYaml,
  markdownBodyToEditorHtml,
  parseNoteForEditor,
} from "@/lib/markdown";
import { isPlaceholderTitle } from "@/lib/path-sync";
import { fileRename, pathSyncSuggest } from "@/lib/ipc";
import {
  bodyMarkdownFromNoteRef,
  serializeOpenNote,
} from "@/lib/serialize-open-note";

const PATH_SYNC_DEBOUNCE_MS = 800;

function pathStem(path: string): string {
  return path.replace(/\.md$/i, "").split("/").pop() ?? path;
}

interface UseOpenNoteOptions {
  activePath: string | null;
  markdown: string;
  activePathRef: RefObject<string | null>;
  markdownRef: RefObject<string>;
  frontmatterYamlRef: RefObject<string | null>;
  editorRef: RefObject<Editor | null>;
  updateTabTitle: (path: string, title: string) => void;
  replaceOpenTabPath: (
    oldPath: string,
    newPath: string,
    title?: string,
  ) => void;
  onMarkdownSynced?: (md: string) => void;
}

export function useOpenNote({
  activePath,
  markdown,
  activePathRef,
  markdownRef,
  frontmatterYamlRef,
  editorRef,
  updateTabTitle,
  replaceOpenTabPath,
  onMarkdownSynced,
}: UseOpenNoteOptions) {
  const [noteTitle, setNoteTitle] = useState("");
  const [bodyMarkdown, setBodyMarkdown] = useState("");

  const pathSyncTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const pathSyncGenRef = useRef(0);

  const syncFromMarkdown = useCallback(
    (md: string, path: string) => {
      const parsed = parseNoteForEditor(md, pathStem(path));
      frontmatterYamlRef.current = parsed.yaml;
      const displayTitle = resolveNoteDisplayTitle({
        path,
        title: parsed.title,
        markdown: md,
      });
      setNoteTitle(displayTitle);
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
    syncFromMarkdown(markdown, activePath);
  }, [activePath, markdown, syncFromMarkdown]);

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
      onMarkdownSynced?.(md);
      const path = activePathRef.current;
      if (path) {
        const savedTitle = displayTitleFromMarkdown(md, "");
        setNoteTitle(resolveNoteDisplayTitle({ path, title: savedTitle }));
      }
    },
    [activePathRef, frontmatterYamlRef, markdownRef, onMarkdownSynced],
  );

  const onTitleChange = useCallback(
    (raw: string) => {
      const path = activePathRef.current;
      if (!path) return;

      const title = resolveNoteDisplayTitle({
        path,
        title: raw.trim(),
      });

      setNoteTitle(title);
      updateTabTitle(path, title);
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
              replaceOpenTabPath(path, entry.path, title);
            });
          })
          .catch(() => {
            /* 路径同步为可选增强 */
          });
      }, PATH_SYNC_DEBOUNCE_MS);
    },
    [activePathRef, replaceOpenTabPath],
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
        editorRef.current.commands.setContent(
          markdownBodyToEditorHtml(parsed.bodyMd),
          false,
        );
      }
    },
    [activePathRef, editorRef, syncFromMarkdown],
  );

  return {
    noteTitle,
    bodyMarkdown,
    getLiveMarkdown,
    applySavedMarkdown,
    onTitleChange,
    onTitleBlur,
    loadBodyIntoEditor,
    syncFromMarkdown,
  };
}
