import { useCallback, useRef, useState } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import {
  displayTitleFromMarkdown,
  resolveDocumentTitle,
} from "@/lib/document-title";
import { extractFrontmatterYaml } from "@/lib/markdown";
import { fileRead } from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { discardEmptyNoteIfNeeded } from "@/lib/note-tab-lifecycle";
import { resolveNoteDisplayTitle } from "@/lib/note-display";

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
  const tabsRef = useRef(tabs);

  activePathRef.current = activePath;
  markdownRef.current = markdown;
  tabsRef.current = tabs;

  const getEditorMarkdown = useCallback(() => markdownRef.current, []);

  const clearEditorState = useCallback(() => {
    activePathRef.current = null;
    markdownRef.current = "";
    frontmatterYamlRef.current = null;
    setActivePath(null);
    setMarkdown("");
  }, []);

  const maybeDiscardOnLeave = useCallback(
    async (path: string): Promise<boolean> => {
      const discarded = await discardEmptyNoteIfNeeded(
        path,
        activePathRef.current,
        markdownRef.current,
      );
      if (discarded) {
        onVaultIndexBump?.();
      }
      return discarded;
    },
    [onVaultIndexBump],
  );

  const openFile = useCallback(
    async (
      path: string,
      titleHint?: string,
      options?: { skipDiscardPrevious?: boolean },
    ) => {
      const current = activePathRef.current;
      if (
        !options?.skipDiscardPrevious &&
        current &&
        current !== path &&
        (await maybeDiscardOnLeave(current))
      ) {
        setTabs((prev) => prev.filter((t) => t.path !== current));
      }
      try {
        const content = await fileRead(path);
        frontmatterYamlRef.current = extractFrontmatterYaml(content);
        const fromMarkdown = displayTitleFromMarkdown(content, "");
        const fallbackDb = await resolveDocumentTitle(path, titleHint);
        const title = resolveNoteDisplayTitle({
          path,
          title: fromMarkdown || titleHint?.trim() || fallbackDb,
          markdown: content,
        });
        setMarkdown(content);
        markdownRef.current = content;
        activePathRef.current = path;
        setActivePath(path);
        setTabs((prev) => {
          if (prev.some((t) => t.path === path)) {
            return prev.map((t) =>
              t.path === path ? { ...t, title, dirty: false } : t,
            );
          }
          return [...prev, { path, title, dirty: false }];
        });
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`无法打开笔记：${msg}`);
        onVaultIndexBump?.();
      }
    },
    [maybeDiscardOnLeave, onStatusChange, onVaultIndexBump],
  );

  const closeTab = useCallback(
    async (path: string) => {
      const isActive = activePathRef.current === path;
      try {
        await maybeDiscardOnLeave(path);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`关闭标签失败：${msg}`);
        return;
      }

      const prevTabs = tabsRef.current;
      const idx = prevTabs.findIndex((t) => t.path === path);
      const nextTabs = prevTabs.filter((t) => t.path !== path);
      const switchTo: string | null = isActive
        ? nextTabs.length === 0
          ? null
          : nextTabs[Math.min(Math.max(0, idx), nextTabs.length - 1)]!.path
        : null;
      setTabs(nextTabs);

      if (!isActive) {
        return;
      }
      if (switchTo === null) {
        clearEditorState();
        return;
      }
      await openFile(switchTo, undefined, { skipDiscardPrevious: true });
    },
    [clearEditorState, maybeDiscardOnLeave, onStatusChange, openFile],
  );

  const handleNewNote = useCallback(async () => {
    try {
      const created = await createDefaultNote({
        extraTakenTitles: tabs.map((tab) => tab.title),
      });
      onVaultIndexBump?.();
      await openFile(created.path, created.title, {
        skipDiscardPrevious: true,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      onStatusChange?.(`新建笔记失败：${msg}`);
    }
  }, [tabs, openFile, onStatusChange, onVaultIndexBump]);

  const markDirty = useCallback(() => {
    setTabs((t) =>
      t.map((tab) =>
        tab.path === activePathRef.current ? { ...tab, dirty: true } : tab,
      ),
    );
  }, []);

  /** 更新标签标题并标记为未保存（用于 noteTitle 行编辑） */
  const updateTabTitle = useCallback((path: string, title: string) => {
    const displayTitle = resolveNoteDisplayTitle({ path, title });
    setTabs((prev) =>
      prev.map((tab) =>
        tab.path === path ? { ...tab, title: displayTitle, dirty: true } : tab,
      ),
    );
  }, []);

  const markClean = useCallback((path: string, title?: string) => {
    const displayTitle = title
      ? resolveNoteDisplayTitle({ path, title })
      : undefined;
    setTabs((prev) => {
      let changed = false;
      const next = prev.map((tab) => {
        if (tab.path !== path) {
          return tab;
        }
        const nextTitle = displayTitle || tab.title;
        if (!tab.dirty && nextTitle === tab.title) {
          return tab;
        }
        changed = true;
        return { ...tab, dirty: false, title: nextTitle };
      });
      return changed ? next : prev;
    });
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
    updateTabTitle,
    getEditorMarkdown,
  };
}
