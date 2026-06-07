import { useCallback, useRef, useState } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import {
  displayTitleFromMarkdown,
  resolveDocumentTitle,
} from "@/lib/document-title";
import { clearCachedEditorHtml } from "@/lib/editor-html-cache";
import { extractFrontmatterYaml } from "@/lib/markdown";
import { fileRead } from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { discardEmptyNoteIfNeeded } from "@/lib/note-tab-lifecycle";
import { resolveNoteDisplayTitle } from "@/lib/note-display";
import { mergeTabsAfterPathRename } from "@/lib/note-tab-rename";

interface UseTabManagerOptions {
  onStatusChange?: (status: string) => void;
  onVaultIndexBump?: () => void;
  /** Flush layer-1 save for `path` before leaving/closing; returns written markdown if any. */
  persistBeforeLeave?: (path: string) => Promise<string | null>;
}

interface OpenNoteOptions {
  allowClassified?: boolean;
}

export function useTabManager(options: UseTabManagerOptions = {}) {
  const { onStatusChange, onVaultIndexBump, persistBeforeLeave } = options;
  const persistBeforeLeaveRef = useRef(persistBeforeLeave);
  persistBeforeLeaveRef.current = persistBeforeLeave;

  const [tabs, setTabs] = useState<TabItem[]>([]);
  const [activePath, setActivePath] = useState<string | null>(null);
  const [markdown, setMarkdownState] = useState("");
  /** Incremented when disk content is loaded into tab state (not on editor save). */
  const [editorContentTick, setEditorContentTick] = useState(0);
  const activePathRef = useRef<string | null>(null);
  const markdownRef = useRef("");
  const frontmatterYamlRef = useRef<string | null>(null);
  const tabsRef = useRef(tabs);
  const openFileSeqRef = useRef(0);
  const tabMarkdownCacheRef = useRef(new Map<string, string>());
  const tabLockCacheRef = useRef(new Map<string, boolean>());
  const [activeFileLocked, setActiveFileLocked] = useState(false);

  activePathRef.current = activePath;
  tabsRef.current = tabs;

  const setMarkdown = useCallback((md: string) => {
    markdownRef.current = md;
    const path = activePathRef.current;
    if (path) {
      tabMarkdownCacheRef.current.set(path, md);
    }
    setMarkdownState(md);
  }, []);

  const getEditorMarkdown = useCallback(() => markdownRef.current, []);

  const clearEditorState = useCallback(() => {
    activePathRef.current = null;
    markdownRef.current = "";
    frontmatterYamlRef.current = null;
    setActivePath(null);
    setActiveFileLocked(false);
    setMarkdown("");
  }, [setMarkdown]);

  const setFileLocked = useCallback((path: string, locked: boolean) => {
    tabLockCacheRef.current.set(path, locked);
    if (activePathRef.current === path) {
      setActiveFileLocked(locked);
    }
    setTabs((prev) =>
      prev.map((tab) => (tab.path === path ? { ...tab, locked } : tab)),
    );
  }, []);

  const cacheTabMarkdown = useCallback((path: string, md: string) => {
    tabMarkdownCacheRef.current.set(path, md);
  }, []);

  const persistAndCacheTab = useCallback(
    async (path: string): Promise<string | null> => {
      const saved = (await persistBeforeLeaveRef.current?.(path)) ?? null;
      const md =
        saved ??
        (path === activePathRef.current
          ? markdownRef.current
          : tabMarkdownCacheRef.current.get(path));
      if (md) {
        cacheTabMarkdown(path, md);
      }
      return saved;
    },
    [cacheTabMarkdown],
  );

  const maybeDiscardOnLeave = useCallback(
    async (path: string): Promise<boolean> => {
      const md =
        tabMarkdownCacheRef.current.get(path) ??
        (path === activePathRef.current ? markdownRef.current : "");
      const discarded = await discardEmptyNoteIfNeeded(
        path,
        activePathRef.current,
        md,
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
      options?: OpenNoteOptions & { skipDiscardPrevious?: boolean },
    ) => {
      if (isClassifiedVaultPath(path) && options?.allowClassified !== true) {
        onStatusChange?.("涉密笔记只能从涉密保险库打开");
        return;
      }
      const seq = ++openFileSeqRef.current;
      const current = activePathRef.current;
      if (current && current !== path) {
        await persistAndCacheTab(current);
      }
      if (
        !options?.skipDiscardPrevious &&
        current &&
        current !== path &&
        (await maybeDiscardOnLeave(current))
      ) {
        setTabs((prev) => prev.filter((t) => t.path !== current));
      }
      try {
        const { content, isLocked } = await fileRead(path, {
          allowClassified: options?.allowClassified === true,
        });
        if (openFileSeqRef.current !== seq) return;
        tabLockCacheRef.current.set(path, isLocked);
        setActiveFileLocked(isLocked);
        frontmatterYamlRef.current = extractFrontmatterYaml(content);
        const fromMarkdown = displayTitleFromMarkdown(content, "");
        const fallbackDb = await resolveDocumentTitle(path, titleHint);
        if (openFileSeqRef.current !== seq) return;
        const title = resolveNoteDisplayTitle({
          path,
          title: fromMarkdown || titleHint?.trim() || fallbackDb,
          markdown: content,
        });
        clearCachedEditorHtml(path);
        tabMarkdownCacheRef.current.set(path, content);
        activePathRef.current = path;
        setActivePath(path);
        setMarkdown(content);
        setEditorContentTick((t) => t + 1);
        setTabs((prev) => {
          if (prev.some((t) => t.path === path)) {
            return prev.map((t) =>
              t.path === path ? { ...t, title, dirty: false, locked: isLocked } : t,
            );
          }
          return [...prev, { path, title, dirty: false, locked: isLocked }];
        });
      } catch (e) {
        if (openFileSeqRef.current !== seq) return;
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`无法打开笔记：${msg}`);
        onVaultIndexBump?.();
      }
    },
    [
      maybeDiscardOnLeave,
      onStatusChange,
      onVaultIndexBump,
      persistAndCacheTab,
      setMarkdown,
    ],
  );

  /** Switch to an already-open tab without re-reading disk when session cache exists. */
  const activateTab = useCallback(
    async (path: string) => {
      if (!tabsRef.current.some((t) => t.path === path)) {
        await openFile(path);
        return;
      }
      if (activePathRef.current === path) return;

      const leaving = activePathRef.current;
      if (leaving) {
        await persistAndCacheTab(leaving);
      }

      const cached = tabMarkdownCacheRef.current.get(path);
      if (cached) {
        clearCachedEditorHtml(path);
        activePathRef.current = path;
        setActivePath(path);
        frontmatterYamlRef.current = extractFrontmatterYaml(cached);
        setMarkdown(cached);
        setActiveFileLocked(tabLockCacheRef.current.get(path) ?? false);
        setEditorContentTick((t) => t + 1);
        return;
      }

      await openFile(path, undefined, { skipDiscardPrevious: true });
    },
    [openFile, persistAndCacheTab, setMarkdown],
  );

  /** Open a note from the vault UI: reuse tab session when already open. */
  const openNote = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: OpenNoteOptions,
    ): Promise<void> => {
      if (tabsRef.current.some((t) => t.path === path)) {
        return activateTab(path);
      }
      return openFile(path, titleHint, options);
    },
    [activateTab, openFile],
  );

  const closeTab = useCallback(
    async (path: string) => {
      const isActive = activePathRef.current === path;
      try {
        await persistAndCacheTab(path);
        await maybeDiscardOnLeave(path);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`关闭标签失败：${msg}`);
        return;
      }

      tabMarkdownCacheRef.current.delete(path);
      tabLockCacheRef.current.delete(path);

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
      await activateTab(switchTo);
    },
    [
      activateTab,
      clearEditorState,
      maybeDiscardOnLeave,
      onStatusChange,
      persistAndCacheTab,
    ],
  );

  const handleNewNote = useCallback(async () => {
    try {
      const current = activePathRef.current;
      if (current) {
        await persistAndCacheTab(current);
        const discarded = await maybeDiscardOnLeave(current);
        if (discarded) {
          setTabs((prev) => prev.filter((t) => t.path !== current));
        }
      }
      const created = await createDefaultNote({
        extraTakenTitles: tabsRef.current
          .filter((t) => t.path !== current)
          .map((t) => t.title),
      });
      onVaultIndexBump?.();
      await openFile(created.path, created.title, {
        skipDiscardPrevious: true,
      });
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      onStatusChange?.(`新建笔记失败：${msg}`);
    }
  }, [
    maybeDiscardOnLeave,
    openFile,
    onStatusChange,
    onVaultIndexBump,
    persistAndCacheTab,
  ]);

  const markDirty = useCallback(() => {
    setTabs((t) =>
      t.map((tab) =>
        tab.path === activePathRef.current ? { ...tab, dirty: true } : tab,
      ),
    );
  }, []);

  /** 更新标签标题并标记为未保存（用于文档标题字段编辑） */
  const updateTabTitle = useCallback((path: string, title: string) => {
    const displayTitle = resolveNoteDisplayTitle({ path, title });
    setTabs((prev) =>
      prev.map((tab) =>
        tab.path === path ? { ...tab, title: displayTitle, dirty: true } : tab,
      ),
    );
  }, []);

  /** 重命名已打开笔记的路径（不重新读盘，保留内存中的编辑内容） */
  const replaceOpenTabPath = useCallback(
    (oldPath: string, newPath: string, title?: string) => {
      if (oldPath === newPath) return;
      const displayTitle = title
        ? resolveNoteDisplayTitle({ path: newPath, title })
        : undefined;
      setTabs((prev) =>
        mergeTabsAfterPathRename(prev, oldPath, newPath, displayTitle),
      );
      if (activePathRef.current === oldPath) {
        // 路径变更会 remount 编辑器（key=path），须先同步内存正文避免从陈旧 state 恢复
        activePathRef.current = newPath;
        setActivePath(newPath);
        setMarkdown(markdownRef.current);
      }
    },
    [setMarkdown],
  );

  const syncTabMarkdownCache = useCallback((path: string, markdown: string) => {
    tabMarkdownCacheRef.current.set(path, markdown);
  }, []);

  const getTabMarkdownCached = useCallback(
    (path: string) => tabMarkdownCacheRef.current.get(path),
    [],
  );

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
    activeFileLocked,
    markdown,
    editorContentTick,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    setActivePath,
    setMarkdown,
    setFileLocked,
    openFile,
    openNote,
    activateTab,
    closeTab,
    handleNewNote,
    markDirty,
    markClean,
    updateTabTitle,
    replaceOpenTabPath,
    syncTabMarkdownCache,
    getEditorMarkdown,
    getTabMarkdownCached,
  };
}
