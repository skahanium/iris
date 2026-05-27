import { useCallback, useRef, useState } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import {
  displayTitleFromMarkdown,
  resolveDocumentTitle,
} from "@/lib/document-title";
import { fileDiscard, fileRead } from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import { extractFrontmatterYaml } from "@/lib/markdown";

interface UseTabManagerOptions {
  onStatusChange?: (status: string) => void;
  onVaultIndexBump?: () => void;
}

export function useTabManager(options: UseTabManagerOptions = {}) {
  const { onStatusChange, onVaultIndexBump } = options;

  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [markdown, setMarkdown] = useState("");
  const activePathRef = useRef<string | null>(null);
  const markdownRef = useRef("");
  const frontmatterYamlRef = useRef<string | null>(null);

  activePathRef.current = activePath;
  markdownRef.current = markdown;

  const getEditorMarkdown = useCallback(() => markdownRef.current, []);

  const maybeDiscardEmptyNote = useCallback(
    async (path: string): Promise<boolean> => {
      const md = markdownRef.current;
      if (!isNoteSubstantivelyEmpty(md)) {
        return false;
      }
      await fileDiscard(path);
      onVaultIndexBump?.();
      return true;
    },
    [onVaultIndexBump],
  );

  const openFile = useCallback(
    async (path: string, titleHint?: string) => {
      const current = activePathRef.current;
      if (current && current !== path && (await maybeDiscardEmptyNote(current))) {
        setTabs((prev) => prev.filter((t) => t.path !== current));
      }
      try {
        const content = await fileRead(path);
        frontmatterYamlRef.current = extractFrontmatterYaml(content);
        const fromMarkdown = displayTitleFromMarkdown(content, "");
        const fallbackDb = await resolveDocumentTitle(path, titleHint);
        setMarkdown(content);
        markdownRef.current = content;
        setActivePath(path);
        setTabs((prev) => {
          const existing = prev.find((t) => t.path === path);
          const title =
            fromMarkdown ||
            (titleHint?.trim() ?? "") ||
            existing?.title ||
            fallbackDb;
          if (prev.some((t) => t.path === path)) {
            return prev.map((t) => (t.path === path ? { ...t, title } : t));
          }
          return [...prev, { path, title, dirty: false }];
        });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`无法打开笔记：${msg}`);
        onVaultIndexBump?.();
      }
    },
    [maybeDiscardEmptyNote, onStatusChange, onVaultIndexBump],
  );

  const closeTab = useCallback(
    async (path: string) => {
      if (activePathRef.current === path) {
        if (await maybeDiscardEmptyNote(path)) {
          setTabs((prev) => {
            const idx = prev.findIndex((t) => t.path === path);
            const next = prev.filter((t) => t.path !== path);
            if (next.length === 0) {
              setActivePath(null);
              setMarkdown("");
              markdownRef.current = "";
            } else {
              const newIdx = Math.min(Math.max(0, idx), next.length - 1);
              void openFile(next[newIdx]!.path);
            }
            return next;
          });
          return;
        }
      }
      setTabs((prev) => {
        const idx = prev.findIndex((t) => t.path === path);
        const next = prev.filter((t) => t.path !== path);
        if (activePathRef.current === path) {
          if (next.length === 0) {
            setActivePath(null);
            setMarkdown("");
            markdownRef.current = "";
          } else {
            const newIdx = Math.min(Math.max(0, idx), next.length - 1);
            const newPath = next[newIdx]!.path;
            void openFile(newPath);
          }
        }
        return next;
      });
    },
    [openFile, maybeDiscardEmptyNote],
  );

  const handleNewNote = useCallback(async () => {
    const current = activePathRef.current;
    if (current && (await maybeDiscardEmptyNote(current))) {
      setTabs((prev) => prev.filter((t) => t.path !== current));
      setActivePath(null);
      setMarkdown("");
      markdownRef.current = "";
    }
    const created = await createDefaultNote();
    await openFile(created.path, created.title);
  }, [openFile, maybeDiscardEmptyNote]);

  const markDirty = useCallback(() => {
    setTabs((t) =>
      t.map((tab) =>
        tab.path === activePathRef.current ? { ...tab, dirty: true } : tab,
      ),
    );
  }, []);

  const markClean = useCallback((path: string, title?: string) => {
    setTabs((t) =>
      t.map((tab) =>
        tab.path === path
          ? { ...tab, dirty: false, title: title || tab.title }
          : tab,
      ),
    );
  }, []);

  return {
    tabs,
    activePath,
    markdown,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    setActivePath,
    setMarkdown,
    openFile,
    closeTab,
    handleNewNote,
    markDirty,
    markClean,
    getEditorMarkdown,
  };
}
