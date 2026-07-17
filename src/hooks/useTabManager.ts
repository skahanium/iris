import { useCallback, useRef, useState } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import { displayTitleFromMarkdown } from "@/lib/document-title";
import { parseNoteForEditor } from "@/lib/markdown";
import { documentOpenEnd, fileRead } from "@/lib/ipc";
import { createDefaultNote } from "@/lib/note-create";
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
  /** Safely stop persistence and permanently discard a never-edited temporary note. */
  discardPristineNote?: (path: string, markdown: string) => Promise<void>;
}

export interface CloseTabResult {
  closed: boolean;
  discardedPristine: boolean;
  nextActivePath: string | null;
  remainingNoteCount: number;
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
  /** A newly registered note still waits for editor first-frame acknowledgement. */
  stageEvenIfRegistered?: boolean;
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
  const discardPristineNoteRef = useRef(options.discardPristineNote);
  discardPristineNoteRef.current = options.discardPristineNote;

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
  /** Serializes allocation + immediate registration for each + click. */
  const newNoteQueueRef = useRef<Promise<void>>(Promise.resolve());
  const [activeFileLocked, setActiveFileLocked] = useState(false);

  activePathRef.current = activePath;
  tabsRef.current = tabs;

  const replaceTabs = useCallback(
    (updater: (current: TabItem[]) => TabItem[]) => {
      const next = updater(tabsRef.current);
      tabsRef.current = next;
      setTabs(next);
      return next;
    },
    [],
  );

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

  const setFileLocked = useCallback(
    (path: string, locked: boolean) => {
      tabLockCacheRef.current.set(path, locked);
      if (activePathRef.current === path) {
        setActiveFileLocked(locked);
      }
      replaceTabs((prev) =>
        prev.map((tab) => (tab.path === path ? { ...tab, locked } : tab)),
      );
    },
    [replaceTabs],
  );

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

  const promoteTab = useCallback(
    (path: string) => {
      replaceTabs((current) =>
        current.map((tab) =>
          tab.path === path && tab.lifecycle === "session_pristine"
            ? { ...tab, lifecycle: "persisted" }
            : tab,
        ),
      );
    },
    [replaceTabs],
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
      replaceTabs((prev) => {
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
            lifecycle: "persisted",
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
    [replaceTabs],
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
          ? persistAndCacheTab(current)
          : Promise.resolve(null);

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
        const [{ content, isLocked }] = await Promise.all([
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
        const discardedPreviousPath = null;
        const shouldStageOpen =
          options?.stageEvenIfRegistered === true ||
          !tabsRef.current.some((tab) => tab.path === path);
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
    async (path: string): Promise<CloseTabResult> => {
      const target = tabsRef.current.find((tab) => tab.path === path);
      if (!target) {
        return {
          closed: false,
          discardedPristine: false,
          nextActivePath: activePathRef.current,
          remainingNoteCount: tabsRef.current.length,
        };
      }
      const isActive = activePathRef.current === path;
      const pristine = target.lifecycle === "session_pristine";
      try {
        if (isPathOpening(path)) {
          cancelPendingNoteOpen();
        }
        if (pristine) {
          const content =
            tabMarkdownCacheRef.current.get(path) ??
            (isActive ? markdownRef.current : "");
          if (!content) {
            throw new Error("临时笔记内容尚未就绪，无法安全丢弃");
          }
          if (!discardPristineNoteRef.current) {
            throw new Error("临时笔记丢弃服务尚未就绪");
          }
          await discardPristineNoteRef.current(path, content);
          onVaultIndexBump?.();
        } else {
          await persistAndCacheTab(path);
        }
      } catch (e) {
        const msg = e instanceof Error ? e.message : String(e);
        onStatusChange?.(`关闭标签失败：${msg}`);
        return {
          closed: false,
          discardedPristine: false,
          nextActivePath: activePathRef.current,
          remainingNoteCount: tabsRef.current.length,
        };
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
      replaceTabs(() => nextTabs);

      if (!isActive) {
        return {
          closed: true,
          discardedPristine: pristine,
          nextActivePath: activePathRef.current,
          remainingNoteCount: nextTabs.length,
        };
      }
      if (switchTo === null) {
        clearEditorState();
        return {
          closed: true,
          discardedPristine: pristine,
          nextActivePath: null,
          remainingNoteCount: 0,
        };
      }
      await activateTab(switchTo);
      return {
        closed: true,
        discardedPristine: pristine,
        nextActivePath: switchTo,
        remainingNoteCount: nextTabs.length,
      };
    },
    [
      activateTab,
      cancelPendingNoteOpen,
      clearEditorState,
      onStatusChange,
      onVaultIndexBump,
      persistAndCacheTab,
      isPathOpening,
      replaceTabs,
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
        replaceTabs(() => nextTabs);
        return;
      }
      if (switchTo === null) {
        replaceTabs(() => nextTabs);
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
      replaceTabs,
    ],
  );

  const handleNewNote = useCallback(
    (options: { homeOpenSequence?: number } = {}) => {
      const run = async () => {
        let createdPath: string | null = null;
        try {
          cancelOpenTransaction();
          const current = activePathRef.current;
          if (current) {
            await persistAndCacheTab(current);
          }
          const created = await createDefaultNote({
            extraTakenTitles: tabsRef.current.map((tab) => tab.title),
          });
          createdPath = created.path;
          tabMarkdownCacheRef.current.set(created.path, created.content);
          tabLockCacheRef.current.set(created.path, false);
          replaceTabs((existing) => [
            ...existing,
            {
              dirty: false,
              lifecycle: "session_pristine",
              locked: false,
              path: created.path,
              title: created.title,
            },
          ]);
          activePathRef.current = created.path;
          markdownRef.current = created.content;
          frontmatterYamlRef.current = parseNoteForEditor(
            created.content,
            pathStem(created.path),
          ).yaml;
          setActivePath(created.path);
          setActiveFileLocked(false);
          setMarkdownState(created.content);
          setEditorContentTick((tick) => tick + 1);
          setPersistenceContentTick((tick) => tick + 1);
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
            { content: created.content, isLocked: false },
          );
          await openFile(created.path, created.title, {
            homeOpenSequence: options.homeOpenSequence,
            openBudgetKind: "hot",
            openStartedAt,
            openTraceRequest,
            preparedNote,
            stageEvenIfRegistered: true,
            skipDiscardPrevious: true,
          });
        } catch (e) {
          if (isSupersededError(e)) return;
          if (
            createdPath &&
            tabsRef.current.some((tab) => tab.path === createdPath)
          ) {
            const content = tabMarkdownCacheRef.current.get(createdPath);
            try {
              if (content && discardPristineNoteRef.current) {
                await discardPristineNoteRef.current(createdPath, content);
                await discardOpenTab(createdPath);
                onVaultIndexBump?.();
              }
            } catch {
              // Keep the registered tab if rollback cannot prove the disk file is gone.
            }
          }
          const msg = e instanceof Error ? e.message : String(e);
          onStatusChange?.(`新建笔记失败：${msg}`);
          if (options.homeOpenSequence !== undefined) {
            throw e;
          }
        }
      };
      const task = newNoteQueueRef.current.then(run, run);
      newNoteQueueRef.current = task.catch(() => undefined);
      return task;
    },
    [
      cancelOpenTransaction,
      discardOpenTab,
      onStatusChange,
      onVaultIndexBump,
      openFile,
      persistAndCacheTab,
      replaceTabs,
    ],
  );

  const markDirty = useCallback(() => {
    const path = activePathRef.current;
    if (!path) return;
    replaceTabs((t) =>
      t.map((tab) =>
        tab.path === path
          ? { ...tab, dirty: true, lifecycle: "persisted" }
          : tab,
      ),
    );
  }, [replaceTabs]);

  /** 更新标签标题并标记为未保存（用于文档标题字段编辑） */
  const updateTabTitle = useCallback(
    (path: string, title: string) => {
      const displayTitle = resolveNoteDisplayTitle({ path, title });
      replaceTabs((prev) =>
        prev.map((tab) =>
          tab.path === path
            ? {
                ...tab,
                dirty: true,
                lifecycle: "persisted",
                title: displayTitle,
              }
            : tab,
        ),
      );
    },
    [replaceTabs],
  );

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
      replaceTabs((prev) =>
        mergeTabsAfterPathRename(prev, oldPath, newPath, displayTitle).map(
          (tab) =>
            tab.path === newPath ? { ...tab, lifecycle: "persisted" } : tab,
        ),
      );
      if (activePathRef.current === oldPath) {
        // Path change remounts the editor (key=path); sync markdown first to avoid restoring stale state.
        activePathRef.current = newPath;
        setActivePath(newPath);
        setMarkdown(markdownOverride ?? markdownRef.current);
      }
    },
    [replaceTabs, setMarkdown],
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

  const markClean = useCallback(
    (path: string, title?: string) => {
      const displayTitle = title
        ? resolveNoteDisplayTitle({ path, title })
        : undefined;
      replaceTabs((prev) => {
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
    },
    [replaceTabs],
  );

  return {
    tabs,
    activePath,
    activeFileLocked,
    markdown,
    editorContentTick,
    persistenceContentTick,
    pendingNoteOpen,
    cancelPendingNoteOpen,
    cancelOpenTransaction,
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
    promoteTab,
    updateTabTitle,
    replaceOpenTabPath,
    syncTabMarkdownCache,
    invalidateDocumentRuntimeState,
    getEditorMarkdown,
    getTabMarkdownCached,
    getCommittedMarkdown,
  };
}
