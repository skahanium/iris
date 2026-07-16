import { useCallback, useRef, useState } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import { displayTitleFromMarkdown } from "@/lib/document-title";
import { parseNoteForEditor } from "@/lib/markdown";
import { documentOpenEnd, fileRead } from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
import { discardEmptyNoteIfNeeded } from "@/lib/note-tab-lifecycle";
import { pathStem, resolveNoteDisplayTitle } from "@/lib/note-display";
import { mergeTabsAfterPathRename } from "@/lib/note-tab-rename";
import {
  emitNoteOpenVisibleCommitTrace,
  prepareNoteOpenFromContent,
  type DocumentOpenPriority,
  type NoteOpenBudgetKind,
  type NoteOpenSource,
  type PrepareNoteOpenRequest,
  type PreparedNoteOpen,
} from "@/lib/note-open-preparation";

interface UseTabManagerOptions {
  onStatusChange?: (status: string) => void;
  onVaultIndexBump?: () => void;
  /** Flush layer-1 save for `path` before leaving/closing; returns written markdown if any. */
  persistBeforeLeave?: (path: string) => Promise<string | null>;
}

interface OpenNoteOptions {
  allowClassified?: boolean;
  homeOpenSequence?: number;
  documentOpenToken?: string;
  onDocumentOpenTokenRetained?: () => void;
  openBudgetKind?: NoteOpenBudgetKind;
  openStartedAt?: number;
  openTraceRequest?: PrepareNoteOpenRequest;
  preparedNote?: PreparedNoteOpen;
  priority?: DocumentOpenPriority;
  source?: NoteOpenSource;
}

export interface PendingNoteOpen {
  bodyMarkdown: string;
  content: string;
  editorHtmlDigest?: string;
  documentOpenToken?: string;
  editorHtmlStatus?: PreparedNoteOpen["editorHtmlStatus"];
  frontmatterYaml: string | null;
  homeOpenSequence?: number;
  isLocked: boolean;
  namespace: "normal" | "classified";
  openBudgetKind?: NoteOpenBudgetKind;
  openStartedAt?: number;
  openTraceRequest?: PrepareNoteOpenRequest;
  path: string;
  preparedEditorHtml?: string;
  preserveFragments?: PreparedNoteOpen["preserveFragments"];
  sequence: number;
  title: string;
}

interface PendingNoteOpenCommit extends PendingNoteOpen {
  discardedPreviousPath: string | null;
  reject: (error: Error) => void;
  resolve: () => void;
}

export interface CommitPendingNoteOpenOptions {
  skipContentTick?: boolean;
}

const OPEN_SUPERSEDED = "IRIS_OPEN_SUPERSEDED";

function createSupersededError(): Error {
  const error = new Error(OPEN_SUPERSEDED);
  error.name = OPEN_SUPERSEDED;
  return error;
}

function isSupersededError(error: unknown): boolean {
  return error instanceof Error && error.name === OPEN_SUPERSEDED;
}

function endDocumentOpenToken(token?: string): void {
  if (!token) return;
  void documentOpenEnd(token).catch(() => undefined);
}

function namespaceForPath(path: string): "normal" | "classified" {
  return isClassifiedVaultPath(path) ? "classified" : "normal";
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
  /** Incremented only when authoritative disk/prepared content is committed. */
  const [persistenceContentTick, setPersistenceContentTick] = useState(0);
  const [pendingNoteOpen, setPendingNoteOpen] =
    useState<PendingNoteOpen | null>(null);
  const activePathRef = useRef<string | null>(null);
  const markdownRef = useRef("");
  const frontmatterYamlRef = useRef<string | null>(null);
  const tabsRef = useRef(tabs);
  const openFileSeqRef = useRef(0);
  const pendingNoteOpenCommitRef = useRef<PendingNoteOpenCommit | null>(null);
  const tabMarkdownCacheRef = useRef(new Map<string, string>());
  const tabLockCacheRef = useRef(new Map<string, boolean>());
  /** Only a blank note created in this app session may bypass the recycle bin. */
  const disposableNewPathsRef = useRef(new Set<string>());
  const [activeFileLocked, setActiveFileLocked] = useState(false);

  activePathRef.current = activePath;
  tabsRef.current = tabs;

  const cancelPendingNoteOpen = useCallback(
    (reason: Error = createSupersededError()) => {
      const pending = pendingNoteOpenCommitRef.current;
      if (!pending) return;
      pendingNoteOpenCommitRef.current = null;
      endDocumentOpenToken(pending.documentOpenToken);
      pending.reject(reason);
      setPendingNoteOpen(null);
    },
    [],
  );

  const hasUncommittedOpen = useCallback(
    () => pendingNoteOpenCommitRef.current !== null,
    [],
  );

  const isPathOpening = useCallback((path: string) => {
    const pending = pendingNoteOpenCommitRef.current;
    return pending?.path === path;
  }, []);

  const getCommittedMarkdown = useCallback(
    (path: string) => tabMarkdownCacheRef.current.get(path),
    [],
  );

  const cancelOpenTransaction = useCallback(() => {
    openFileSeqRef.current += 1;
    cancelPendingNoteOpen();
  }, [cancelPendingNoteOpen]);

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
    cancelPendingNoteOpen();
    activePathRef.current = null;
    markdownRef.current = "";
    frontmatterYamlRef.current = null;
    setActivePath(null);
    setActiveFileLocked(false);
    setMarkdown("");
  }, [cancelPendingNoteOpen, setMarkdown]);

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
      if (isPathOpening(path)) {
        return null;
      }
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
    [cacheTabMarkdown, isPathOpening],
  );

  const maybeDiscardOnLeave = useCallback(
    async (path: string): Promise<boolean> => {
      if (!disposableNewPathsRef.current.has(path)) {
        return false;
      }
      if (isPathOpening(path)) {
        return false;
      }
      if (!tabsRef.current.some((tab) => tab.path === path)) {
        return false;
      }
      const md =
        tabMarkdownCacheRef.current.get(path) ??
        (path === activePathRef.current ? markdownRef.current : "");
      if (!md) {
        return false;
      }
      const discarded = await discardEmptyNoteIfNeeded(
        path,
        activePathRef.current,
        md,
      );
      if (discarded) {
        disposableNewPathsRef.current.delete(path);
        onVaultIndexBump?.();
      } else {
        // A non-empty new note is a real user document from this point onward.
        disposableNewPathsRef.current.delete(path);
      }
      return discarded;
    },
    [isPathOpening, onVaultIndexBump],
  );

  const buildPendingNoteOpen = useCallback(
    ({
      content,
      documentOpenToken,
      homeOpenSequence,
      isLocked,
      openBudgetKind,
      openStartedAt,
      openTraceRequest,
      path,
      preparedNote,
      sequence,
      titleHint,
    }: {
      content: string;
      documentOpenToken?: string;
      homeOpenSequence?: number;
      isLocked: boolean;
      openBudgetKind?: NoteOpenBudgetKind;
      openStartedAt?: number;
      openTraceRequest?: PrepareNoteOpenRequest;
      path: string;
      preparedNote: PreparedNoteOpen | null;
      sequence: number;
      titleHint?: string;
    }): PendingNoteOpen => {
      const parsed = preparedNote
        ? null
        : parseNoteForEditor(content, pathStem(path));
      const bodyMarkdown = preparedNote?.bodyMarkdown ?? parsed!.bodyMd;
      const frontmatterYaml = preparedNote?.frontmatterYaml ?? parsed!.yaml;
      const fromMarkdown = displayTitleFromMarkdown(content, "");
      const title = resolveNoteDisplayTitle({
        path,
        title:
          preparedNote?.title ||
          fromMarkdown ||
          titleHint?.trim() ||
          parsed?.title,
        markdown: content,
      });
      return {
        bodyMarkdown,
        content,
        documentOpenToken,
        editorHtmlDigest: preparedNote?.editorHtmlDigest,
        editorHtmlStatus: preparedNote?.editorHtmlStatus,
        frontmatterYaml,
        homeOpenSequence,
        isLocked,
        namespace: namespaceForPath(path),
        openBudgetKind,
        openStartedAt,
        openTraceRequest,
        path,
        preparedEditorHtml: preparedNote?.preparedEditorHtml,
        preserveFragments: preparedNote?.preserveFragments,
        sequence,
        title,
      };
    },
    [],
  );

  const applyCommittedNoteOpen = useCallback(
    (
      pending: PendingNoteOpen,
      discardedPreviousPath: string | null,
      skipTickBump?: boolean,
    ) => {
      tabLockCacheRef.current.set(pending.path, pending.isLocked);
      tabMarkdownCacheRef.current.set(pending.path, pending.content);
      frontmatterYamlRef.current = pending.frontmatterYaml;
      activePathRef.current = pending.path;
      markdownRef.current = pending.content;
      setActiveFileLocked(pending.isLocked);
      setActivePath(pending.path);
      setMarkdownState(pending.content);
      if (!skipTickBump) {
        setEditorContentTick((tick) => tick + 1);
      }
      setPersistenceContentTick((tick) => tick + 1);
      setTabs((prev) => {
        const withoutDiscarded = discardedPreviousPath
          ? prev.filter((tab) => tab.path !== discardedPreviousPath)
          : prev;
        if (withoutDiscarded.some((tab) => tab.path === pending.path)) {
          return withoutDiscarded.map((tab) =>
            tab.path === pending.path
              ? {
                  ...tab,
                  dirty: false,
                  locked: pending.isLocked,
                  title: pending.title,
                }
              : tab,
          );
        }
        return [
          ...withoutDiscarded,
          {
            dirty: false,
            locked: pending.isLocked,
            path: pending.path,
            title: pending.title,
          },
        ];
      });
      setPendingNoteOpen(null);
      if (
        pending.openStartedAt !== undefined &&
        pending.openTraceRequest &&
        pending.openBudgetKind
      ) {
        emitNoteOpenVisibleCommitTrace(
          pending.openTraceRequest,
          pending.openStartedAt,
          pending.openBudgetKind,
        );
      }
    },
    [],
  );

  const stagePendingNoteOpen = useCallback(
    (
      pending: PendingNoteOpen,
      discardedPreviousPath: string | null,
    ): Promise<void> => {
      const previous = pendingNoteOpenCommitRef.current;
      if (previous) {
        endDocumentOpenToken(previous.documentOpenToken);
        previous.reject(createSupersededError());
      }
      const commit: PendingNoteOpenCommit = {
        ...pending,
        discardedPreviousPath,
        reject: () => undefined,
        resolve: () => undefined,
      };
      pendingNoteOpenCommitRef.current = commit;
      setPendingNoteOpen(pending);
      return Promise.resolve();
    },
    [],
  );

  const commitPendingNoteOpen = useCallback(
    (
      path: string,
      sequence: number,
      options: CommitPendingNoteOpenOptions = {},
    ): boolean => {
      const pending = pendingNoteOpenCommitRef.current;
      if (!pending || pending.path !== path || pending.sequence !== sequence) {
        return false;
      }

      pendingNoteOpenCommitRef.current = null;
      applyCommittedNoteOpen(
        pending,
        pending.discardedPreviousPath,
        options.skipContentTick === true,
      );
      pending.resolve();
      return true;
    },
    [applyCommittedNoteOpen],
  );

  const openFile = useCallback(
    async (
      path: string,
      titleHint?: string,
      options?: OpenNoteOptions & { skipDiscardPrevious?: boolean },
    ) => {
      if (isClassifiedVaultPath(path) && options?.allowClassified !== true) {
        onStatusChange?.("涉密笔记只能从涉密保险库打开");
        throw new Error("涉密笔记只能从涉密保险库打开");
      }
      const seq = ++openFileSeqRef.current;
      const openStartedAt = options?.openStartedAt ?? performance.now();
      cancelPendingNoteOpen();
      const current = activePathRef.current;

      const previousCleanupPromise =
        current && current !== path
          ? (async () => {
              await persistAndCacheTab(current);
              if (!options?.skipDiscardPrevious) {
                return maybeDiscardOnLeave(current);
              }
              return false;
            })()
          : Promise.resolve(false);

      const preparedNote =
        options?.preparedNote?.path === path ? options.preparedNote : null;
      const openBudgetKind =
        options?.openBudgetKind ?? (preparedNote ? "hot" : "none");
      const openTraceRequest = options?.openTraceRequest ?? {
        allowClassified: options?.allowClassified,
        path,
        titleHint,
      };
      const readPromise = preparedNote
        ? Promise.resolve({
            content: preparedNote.content,
            isLocked: preparedNote.isLocked,
          })
        : fileRead(path, {
            allowClassified: options?.allowClassified === true,
          });

      try {
        const [{ content, isLocked }, discardedPrevious] = await Promise.all([
          readPromise,
          previousCleanupPromise,
        ]);
        if (openFileSeqRef.current !== seq) throw createSupersededError();
        const pending = buildPendingNoteOpen({
          content,
          documentOpenToken: options?.documentOpenToken,
          homeOpenSequence: options?.homeOpenSequence,
          isLocked,
          openBudgetKind,
          openStartedAt,
          openTraceRequest,
          path,
          preparedNote,
          sequence: seq,
          titleHint,
        });
        const discardedPreviousPath =
          discardedPrevious && current ? current : null;
        const shouldStageOpen = !tabsRef.current.some(
          (tab) => tab.path === path,
        );
        if (shouldStageOpen) {
          await stagePendingNoteOpen(pending, discardedPreviousPath);
          if (options?.documentOpenToken) {
            options.onDocumentOpenTokenRetained?.();
          }
          return;
        }
        applyCommittedNoteOpen(pending, discardedPreviousPath);
      } catch (e) {
        if (openFileSeqRef.current !== seq || isSupersededError(e)) {
          throw e;
        }
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`无法打开笔记：${msg}`);
        onVaultIndexBump?.();
        throw e;
      }
    },
    [
      applyCommittedNoteOpen,
      buildPendingNoteOpen,
      cancelPendingNoteOpen,
      maybeDiscardOnLeave,
      onStatusChange,
      onVaultIndexBump,
      persistAndCacheTab,
      stagePendingNoteOpen,
    ],
  );

  /** Switch to an already-open tab without re-reading disk when session cache exists. */
  const activateTab = useCallback(
    async (path: string, options?: OpenNoteOptions) => {
      if (!tabsRef.current.some((t) => t.path === path)) {
        await openFile(path, undefined, options);
        return;
      }
      if (activePathRef.current === path) return;

      const seq = ++openFileSeqRef.current;
      const openStartedAt = performance.now();
      cancelPendingNoteOpen();
      const leaving = activePathRef.current;
      const leavingTab = leaving
        ? tabsRef.current.find((t) => t.path === leaving)
        : undefined;
      const shouldPersistLeavingInBackground = Boolean(leavingTab?.dirty);
      if (leaving) {
        cacheTabMarkdown(leaving, markdownRef.current);
      }
      if (openFileSeqRef.current !== seq) return;

      const cached = tabMarkdownCacheRef.current.get(path);
      const cachedTitle = tabsRef.current.find(
        (tab) => tab.path === path,
      )?.title;
      if (cached !== undefined) {
        const pending = buildPendingNoteOpen({
          content: cached,
          homeOpenSequence: options?.homeOpenSequence,
          isLocked: tabLockCacheRef.current.get(path) ?? false,
          openBudgetKind: options?.openBudgetKind ?? "hot",
          openStartedAt: options?.openStartedAt ?? openStartedAt,
          openTraceRequest: options?.openTraceRequest ?? {
            path,
            priority: options?.priority ?? "hot",
            source: options?.source ?? "tab",
            titleHint: cachedTitle,
          },
          path,
          preparedNote: null,
          sequence: seq,
          titleHint: cachedTitle,
        });
        applyCommittedNoteOpen(pending, null, true);
        if (leaving && shouldPersistLeavingInBackground) {
          void persistAndCacheTab(leaving).catch((error: unknown) => {
            const msg = error instanceof Error ? error.message : String(error);
            onStatusChange?.(`保存离开的笔记失败：${msg}`);
          });
        }
        return;
      }

      await openFile(path, undefined, {
        ...options,
        openBudgetKind: options?.openBudgetKind ?? "none",
        openStartedAt: options?.openStartedAt ?? openStartedAt,
        openTraceRequest: options?.openTraceRequest ?? {
          path,
          priority: options?.priority ?? "hot",
          source: options?.source ?? "tab",
          titleHint: cachedTitle,
        },
        skipDiscardPrevious: true,
      });
    },
    [
      applyCommittedNoteOpen,
      buildPendingNoteOpen,
      cacheTabMarkdown,
      cancelPendingNoteOpen,
      onStatusChange,
      openFile,
      persistAndCacheTab,
    ],
  );

  /** Open a note from the vault UI: reuse tab session when already open. */
  const openNote = useCallback(
    (
      path: string,
      titleHint?: string,
      options?: OpenNoteOptions,
    ): Promise<void> => {
      if (tabsRef.current.some((t) => t.path === path)) {
        return activateTab(path, options);
      }
      return openFile(path, titleHint, options);
    },
    [activateTab, openFile],
  );

  const closeTab = useCallback(
    async (path: string): Promise<boolean> => {
      const isActive = activePathRef.current === path;
      try {
        await persistAndCacheTab(path);
        await maybeDiscardOnLeave(path);
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`关闭标签失败：${msg}`);
        return false;
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
        return true;
      }
      if (switchTo === null) {
        clearEditorState();
        return true;
      }
      await activateTab(switchTo);
      return true;
    },
    [
      activateTab,
      clearEditorState,
      maybeDiscardOnLeave,
      onStatusChange,
      persistAndCacheTab,
    ],
  );

  const discardOpenTab = useCallback(
    async (path: string) => {
      const prevTabs = tabsRef.current;
      const isActive = activePathRef.current === path;
      const idx = prevTabs.findIndex((t) => t.path === path);
      const nextTabs = prevTabs.filter((t) => t.path !== path);
      const switchTo: string | null = isActive
        ? nextTabs.length === 0
          ? null
          : nextTabs[Math.min(Math.max(0, idx), nextTabs.length - 1)]!.path
        : null;

      tabMarkdownCacheRef.current.delete(path);
      tabLockCacheRef.current.delete(path);

      if (!isActive) {
        setTabs(nextTabs);
        return;
      }
      if (switchTo === null) {
        setTabs(nextTabs);
        clearEditorState();
        return;
      }

      const cached = tabMarkdownCacheRef.current.get(switchTo);
      if (cached !== undefined) {
        const seq = ++openFileSeqRef.current;
        cancelPendingNoteOpen();
        const pending = buildPendingNoteOpen({
          content: cached,
          isLocked: tabLockCacheRef.current.get(switchTo) ?? false,
          openBudgetKind: "warm",
          openStartedAt: performance.now(),
          openTraceRequest: {
            path: switchTo,
            titleHint: nextTabs.find((tab) => tab.path === switchTo)?.title,
          },
          path: switchTo,
          preparedNote: null,
          sequence: seq,
          titleHint: nextTabs.find((tab) => tab.path === switchTo)?.title,
        });
        applyCommittedNoteOpen(pending, path, true);
        return;
      }

      await openFile(switchTo, undefined, { skipDiscardPrevious: true });
    },
    [
      applyCommittedNoteOpen,
      buildPendingNoteOpen,
      cancelPendingNoteOpen,
      clearEditorState,
      openFile,
    ],
  );

  const handleNewNote = useCallback(
    async (options: { homeOpenSequence?: number } = {}) => {
      try {
        cancelOpenTransaction();
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
        disposableNewPathsRef.current.add(created.path);
        onVaultIndexBump?.();
        const openStartedAt = performance.now();
        const openTraceRequest: PrepareNoteOpenRequest = {
          path: created.path,
          priority: "hot",
          source: "new-note",
          titleHint: created.title,
        };
        const preparedNote = await prepareNoteOpenFromContent(
          openTraceRequest,
          {
            content: created.content,
            isLocked: false,
          },
        );
        await openFile(created.path, created.title, {
          homeOpenSequence: options.homeOpenSequence,
          openBudgetKind: "hot",
          openStartedAt,
          openTraceRequest,
          preparedNote,
          skipDiscardPrevious: true,
        });
      } catch (e) {
        if (isSupersededError(e)) return;
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`新建笔记失败：${msg}`);
      }
    },
    [
      maybeDiscardOnLeave,
      cancelOpenTransaction,
      openFile,
      onStatusChange,
      onVaultIndexBump,
      persistAndCacheTab,
    ],
  );

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
    (
      oldPath: string,
      newPath: string,
      title?: string,
      markdownOverride?: string,
    ) => {
      if (oldPath === newPath) return;
      const displayTitle = title
        ? resolveNoteDisplayTitle({ path: newPath, title })
        : undefined;
      const cachedMarkdown =
        markdownOverride ?? tabMarkdownCacheRef.current.get(oldPath);
      if (cachedMarkdown) {
        tabMarkdownCacheRef.current.set(newPath, cachedMarkdown);
      }
      tabMarkdownCacheRef.current.delete(oldPath);
      const cachedLock = tabLockCacheRef.current.get(oldPath);
      if (cachedLock !== undefined) {
        tabLockCacheRef.current.set(newPath, cachedLock);
        tabLockCacheRef.current.delete(oldPath);
      }
      setTabs((prev) =>
        mergeTabsAfterPathRename(prev, oldPath, newPath, displayTitle),
      );
      if (activePathRef.current === oldPath) {
        // Path change remounts the editor (key=path); sync markdown first to avoid restoring stale state.
        activePathRef.current = newPath;
        setActivePath(newPath);
        setMarkdown(markdownOverride ?? markdownRef.current);
      }
    },
    [setMarkdown],
  );

  const syncTabMarkdownCache = useCallback((path: string, markdown: string) => {
    tabMarkdownCacheRef.current.set(path, markdown);
  }, []);

  const invalidateDocumentRuntimeState = useCallback(
    (path: string) => {
      if (isPathOpening(path)) {
        cancelPendingNoteOpen();
      }
      tabMarkdownCacheRef.current.delete(path);
      tabLockCacheRef.current.delete(path);
    },
    [cancelPendingNoteOpen, isPathOpening],
  );

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
    persistenceContentTick,
    pendingNoteOpen,
    cancelPendingNoteOpen,
    hasUncommittedOpen,
    isPathOpening,
    activePathRef,
    markdownRef,
    frontmatterYamlRef,
    setActivePath,
    setMarkdown,
    setFileLocked,
    openFile,
    openNote,
    activateTab,
    commitPendingNoteOpen,
    closeTab,
    discardOpenTab,
    handleNewNote,
    markDirty,
    markClean,
    updateTabTitle,
    replaceOpenTabPath,
    syncTabMarkdownCache,
    invalidateDocumentRuntimeState,
    getEditorMarkdown,
    getTabMarkdownCached,
    getCommittedMarkdown,
  };
}
