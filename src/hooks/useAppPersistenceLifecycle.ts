import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type MutableRefObject,
  type RefObject,
} from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { useTauriCloseSave } from "@/hooks/useTauriCloseSave";
import { useVersionIdle } from "@/hooks/useVersionIdle";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import {
  DocumentPersistenceCoordinator,
  type DocumentPersistenceCaptureSource,
  type DocumentPersistenceMoveResult,
  type DocumentPersistenceSnapshot,
  type DocumentPersistenceStatus,
} from "@/lib/document-persistence-coordinator";
import { editorHtmlDigest, setCachedEditorHtml } from "@/lib/editor-html-cache";
import { splitFrontmatter } from "@/lib/frontmatter";
import {
  fileDiscard,
  fileSetLock,
  fileWrite,
  versionFinalizeCurrent,
  versionSaveIdle,
  versionSaveManual,
  versionSavePreClose,
} from "@/lib/ipc";
import { resolveNoteDisplayTitle } from "@/lib/note-display";
import type { AutoSnapshotLeaveReason } from "@/lib/version-auto-snapshot-policy";
import { createLeaveSnapshotEnqueuer } from "@/lib/version-leave-snapshot";
import {
  createVersionSnapshotScheduler,
  type LastSavedSnapshot,
} from "@/lib/version-snapshot-scheduler";

export interface PersistBeforeLeaveOptions {
  reason?: AutoSnapshotLeaveReason;
  /**
   * Close-tab leave must not paint intermediate dirty/saving/clean chrome on the
   * still-mounted editor; that re-render is the “flash then still open” feel.
   */
  suppressShellUi?: boolean;
  /**
   * Keep shell-UI suppression after persist returns so the caller can clear it
   * only after the tab is removed from chrome (avoids a final flash).
   */
  retainSuppressShellUi?: boolean;
}

export type PersistBeforeLeave = (
  path: string,
  options?: PersistBeforeLeaveOptions,
) => Promise<string | null>;

export interface PersistenceBlocker {
  action: "close";
  retry: () => Promise<void>;
}

interface UseAppPersistenceLifecycleParams {
  activeFileLocked: boolean;
  activePath: string | null;
  activePathRef: MutableRefObject<string | null>;
  applySavedMarkdown: (markdown: string) => void;
  autoSnapshotGenerationRef: MutableRefObject<number>;
  autoVersionEnabled: boolean;
  autoVersionIdleMinutes: number;
  dirtyRef: MutableRefObject<boolean>;
  persistenceContentTick: number;
  editorRef: RefObject<Editor | null>;
  editorReadyRef: RefObject<boolean>;
  getLiveMarkdownRef: MutableRefObject<() => string>;
  getTabMarkdownCached: (path: string) => string | undefined;
  markClean: (path: string, title: string) => void;
  markdown: string;
  onPersistenceBarrierRelease?: () => void;
  onPersistenceBarrierStart?: () => void;
  onPersistenceBlocked?: (blocker: PersistenceBlocker) => void;
  persistBeforeLeaveRef: MutableRefObject<PersistBeforeLeave>;
  setAiStatus: (status: string) => void;
  setFileLocked: (path: string, locked: boolean) => void;
  setMarkdown: (markdown: string) => void;
  syncTabMarkdownCache: (path: string, markdown: string) => void;
  tabsRef: MutableRefObject<TabItem[]>;
}

function isSavedStatus(status: DocumentPersistenceStatus): boolean {
  return status === "saved" || status === "saved_index_degraded";
}

/**
 * Saves that originated from the live editor must not project Markdown back into
 * React shell state. That churns `committedSourceMarkdown`, reinstalls TipTap
 * source projection, and is the shared flash on Cmd+S / lock / close-leave.
 */
function shouldProjectSavedMarkdownIntoShell(
  source: DocumentPersistenceSnapshot["source"],
): boolean {
  return source === "restore" || source === "recovery";
}

export function useAppPersistenceLifecycle({
  activeFileLocked,
  activePath,
  activePathRef,
  applySavedMarkdown,
  autoSnapshotGenerationRef,
  autoVersionEnabled,
  autoVersionIdleMinutes,
  dirtyRef,
  persistenceContentTick,
  editorRef,
  editorReadyRef,
  getLiveMarkdownRef,
  getTabMarkdownCached,
  markClean,
  markdown,
  onPersistenceBarrierRelease,
  onPersistenceBarrierStart,
  onPersistenceBlocked,
  persistBeforeLeaveRef,
  setAiStatus,
  setFileLocked,
  setMarkdown,
  syncTabMarkdownCache,
  tabsRef,
}: UseAppPersistenceLifecycleParams) {
  const coordinatorRef = useRef<DocumentPersistenceCoordinator | null>(null);
  if (!coordinatorRef.current) {
    coordinatorRef.current = new DocumentPersistenceCoordinator({
      write: async (path, content) => {
        const result = await fileWrite(path, content);
        return { indexDegraded: result.indexStatus === "degraded" };
      },
    });
  }
  const coordinator = coordinatorRef.current;
  const [saveStatus, setSaveStatus] =
    useState<DocumentPersistenceStatus>("clean");
  const [saveError, setSaveError] = useState<string | null>(null);
  const [hasDirtyDocuments, setHasDirtyDocuments] = useState(false);
  const [isPersistenceBarrierActive, setIsPersistenceBarrierActive] =
    useState(false);
  const cancelledWriteRef = useRef<Promise<void>>(Promise.resolve());
  const persistenceBarrierActiveRef = useRef(false);
  const persistenceBarrierTaskRef = useRef<Promise<void> | null>(null);
  /** Session id or path key while close-tab suppress is active. */
  const suppressShellUiKeyRef = useRef<string | null>(null);
  /** Latest shell markdown without putting it in acknowledgeSnapshot deps. */
  const markdownShellRef = useRef(markdown);
  markdownShellRef.current = markdown;
  const isPersistenceBarrierActiveNow = useCallback(
    () => persistenceBarrierActiveRef.current,
    [],
  );

  const isShellUiSuppressedForPath = useCallback(
    (path: string): boolean => {
      const key = suppressShellUiKeyRef.current;
      if (!key) return false;
      if (key === path) return true;
      const tab = tabsRef.current.find((item) => item.path === path);
      return Boolean(tab?.documentSessionId && tab.documentSessionId === key);
    },
    [tabsRef],
  );
  const isShellUiSuppressedForPathRef = useRef(isShellUiSuppressedForPath);
  isShellUiSuppressedForPathRef.current = isShellUiSuppressedForPath;

  const clearSuppressShellUi = useCallback(() => {
    suppressShellUiKeyRef.current = null;
  }, []);

  /**
   * Arm close-tab shell-UI suppress before any await, and cancel a pending
   * debounce timer so it cannot paint dirty/saving chrome mid-close.
   */
  const beginSuppressShellUi = useCallback(
    (path: string) => {
      const tab = tabsRef.current.find((item) => item.path === path);
      suppressShellUiKeyRef.current = tab?.documentSessionId ?? path;
      coordinator.cancelScheduled(path);
    },
    [coordinator, tabsRef],
  );
  const getLastSavedSnapshot = useCallback((): LastSavedSnapshot | null => {
    const path = activePathRef.current;
    if (!path) return null;
    const snapshot = coordinator.get(path);
    if (!snapshot || snapshot.baselineRevision !== snapshot.revision) {
      return null;
    }
    return {
      path,
      markdown: snapshot.baselineMarkdown,
      savedAt: snapshot.savedAt ?? Date.now(),
      dirtyGeneration: snapshot.revision,
    };
  }, [activePathRef, coordinator]);

  const acknowledgeSnapshot = useCallback(
    (snapshot: DocumentPersistenceSnapshot) => {
      // `clean` is emitted by coordinator.load(). It establishes a disk
      // baseline only and must never project stale editor state back into the
      // tab title or Markdown.
      if (!isSavedStatus(snapshot.status)) {
        return;
      }
      syncTabMarkdownCache(snapshot.path, snapshot.markdown);
      const suppressShellUi = isShellUiSuppressedForPathRef.current(
        snapshot.path,
      );
      if (!suppressShellUi) {
        markClean(
          snapshot.path,
          resolveNoteDisplayTitle({ path: snapshot.path }),
        );
      }
      if (snapshot.path !== activePathRef.current) return;
      applySavedMarkdown(snapshot.markdown);
      dirtyRef.current = false;
      // Live-origin saves already match the editor; projecting them back flashes
      // the shell (save, lock flush, and leave-before-close all hit this path).
      const projectIntoShell = shouldProjectSavedMarkdownIntoShell(
        snapshot.source,
      );
      const willSetMarkdown =
        projectIntoShell && markdownShellRef.current !== snapshot.markdown;
      if (willSetMarkdown) {
        setMarkdown(snapshot.markdown);
      }
    },
    [
      activePathRef,
      applySavedMarkdown,
      dirtyRef,
      markClean,
      setMarkdown,
      syncTabMarkdownCache,
    ],
  );
  const acknowledgeSnapshotRef = useRef(acknowledgeSnapshot);
  acknowledgeSnapshotRef.current = acknowledgeSnapshot;

  useEffect(() => {
    return coordinator.subscribe((snapshot) => {
      setHasDirtyDocuments(coordinator.hasDirtyDocuments());
      if (!snapshot) return;
      const suppressShellUi = isShellUiSuppressedForPathRef.current(
        snapshot.path,
      );
      if (snapshot.path === activePathRef.current && !suppressShellUi) {
        setSaveStatus(snapshot.status);
        setSaveError(snapshot.error);
      }
      acknowledgeSnapshotRef.current(snapshot);
    });
  }, [activePathRef, coordinator]);

  useEffect(() => {
    const path = activePath;
    if (!path) return;
    coordinator.load(path, markdown, persistenceContentTick);
    // `persistenceContentTick` denotes only an authoritative disk/prepared load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [activePath, coordinator, persistenceContentTick]);

  const versionSnapshotScheduler = useMemo(
    () =>
      createVersionSnapshotScheduler({
        versionSaveIdle,
        versionSaveManual,
        versionFinalizeCurrent,
        versionSavePreClose,
        onError: (err) => {
          const msg = err instanceof Error ? err.message : String(err);
          setAiStatus(`自动版本备份提交失败：${msg}`);
        },
      }),
    [setAiStatus],
  );

  const enqueueIdleSnapshot = useCallback(
    (snapshot: LastSavedSnapshot) => {
      if (!autoVersionEnabled) return;
      if (snapshot.path === activePathRef.current && !editorReadyRef.current) {
        return;
      }
      const result = versionSnapshotScheduler.enqueueIdle(snapshot);
      if (result.accepted) void result.done;
    },
    [
      activePathRef,
      autoVersionEnabled,
      editorReadyRef,
      versionSnapshotScheduler,
    ],
  );

  const enqueueLeaveSnapshot = useMemo(
    () =>
      createLeaveSnapshotEnqueuer({
        enqueueIdleSnapshot,
        nextDirtyGeneration: () => {
          autoSnapshotGenerationRef.current += 1;
          return autoSnapshotGenerationRef.current;
        },
      }),
    [autoSnapshotGenerationRef, enqueueIdleSnapshot],
  );

  const flushSaveForPath = useCallback(
    async (
      path: string,
      source: DocumentPersistenceCaptureSource,
      getMarkdownOverride?: () => string,
    ): Promise<string | null> => {
      const markdownSnapshot =
        getMarkdownOverride?.() ??
        (path === activePathRef.current
          ? getLiveMarkdownRef.current()
          : getTabMarkdownCached(path));
      if (markdownSnapshot === undefined) {
        throw new Error(`no recoverable snapshot for ${path}`);
      }
      coordinator.capture(path, markdownSnapshot, source);
      return (await coordinator.barrier(path)).markdown;
    },
    [activePathRef, coordinator, getLiveMarkdownRef, getTabMarkdownCached],
  );

  const flushSave = useCallback(async (): Promise<string | null> => {
    const path = activePathRef.current;
    if (!path) return null;
    return flushSaveForPath(path, "explicit_save");
  }, [activePathRef, flushSaveForPath]);

  const restoreVersion = useCallback(
    async (path: string, markdown: string): Promise<string> => {
      coordinator.capture(path, markdown, "restore");
      return (await coordinator.barrier(path)).markdown;
    },
    [coordinator],
  );

  const restoreCurrentVersion = useCallback(
    async (markdown: string): Promise<void> => {
      const path = activePathRef.current;
      if (!path) throw new Error("没有可恢复版本的当前文档");
      await restoreVersion(path, markdown);
    },
    [activePathRef, restoreVersion],
  );

  const notifyDirty = useCallback(
    (sourcePath: string): boolean => {
      if (persistenceBarrierActiveRef.current) return false;
      const path = activePathRef.current;
      if (!path || path !== sourcePath || !editorReadyRef.current) return false;
      const markdown = getLiveMarkdownRef.current();
      const existing = coordinator.get(path);
      // TipTap can emit onUpdate after Cmd+S / projection refresh even when the
      // serialized markdown still matches the clean baseline. Treat that as a
      // no-op so tab.dirty does not flip back on and force a leave flush flash.
      if (
        existing &&
        existing.baselineRevision === existing.revision &&
        existing.baselineMarkdown === markdown
      ) {
        return false;
      }
      coordinator.capture(path, markdown, "user_edit");
      // Keep remount/leave backup in sync with the live dirty body. Clearing
      // this cache on first dirty made Home/close throw when TipTap was gone.
      syncTabMarkdownCache(path, markdown);
      return true;
    },
    [
      activePathRef,
      coordinator,
      editorReadyRef,
      getLiveMarkdownRef,
      syncTabMarkdownCache,
    ],
  );

  const cancelPendingSave = useCallback(() => {
    const path = activePathRef.current;
    if (!path) return;
    cancelledWriteRef.current = coordinator.discard(path);
  }, [activePathRef, coordinator]);

  const awaitSaveInFlight = useCallback(async (): Promise<void> => {
    await cancelledWriteRef.current;
  }, []);

  /**
   * Stops this path's delayed/in-flight persistence before deleting its
   * temporary disk file. On deletion failure the coordinator is restored so
   * the still-existing file remains protected by normal save handling.
   */
  const discardPristineNote = useCallback(
    async (path: string, markdownSnapshot: string): Promise<void> => {
      const beforeDiscard = coordinator.get(path);
      await coordinator.discard(path);
      try {
        await fileDiscard(path);
      } catch (error) {
        coordinator.load(
          path,
          beforeDiscard?.markdown ?? markdownSnapshot,
          beforeDiscard?.loadGeneration ?? -1,
        );
        throw error;
      }
    },
    [coordinator],
  );

  persistBeforeLeaveRef.current = async (
    path: string,
    options: PersistBeforeLeaveOptions = {},
  ) => {
    const reason = options.reason ?? "tab_leave";
    const suppressShellUi = Boolean(options.suppressShellUi);
    const retainSuppress = Boolean(options.retainSuppressShellUi);
    if (suppressShellUi) {
      const tab = tabsRef.current.find((item) => item.path === path);
      suppressShellUiKeyRef.current = tab?.documentSessionId ?? path;
    }
    try {
      return await runPersistBeforeLeave(path, reason);
    } finally {
      if (suppressShellUi && !retainSuppress) {
        // Only clear when this call still owns the suppress key.
        const tab = tabsRef.current.find((item) => item.path === path);
        const key = tab?.documentSessionId ?? path;
        if (suppressShellUiKeyRef.current === key) {
          suppressShellUiKeyRef.current = null;
        }
      }
    }
  };

  // Hoisted implementation so close-tab can suppress shell paints during leave.
  async function runPersistBeforeLeave(
    path: string,
    reason: AutoSnapshotLeaveReason,
  ): Promise<string | null> {
    const persisted = coordinator.get(path);
    const tab = tabsRef.current.find((item) => item.path === path);
    const tabCached = getTabMarkdownCached(path);
    if (persisted && persisted.baselineRevision === persisted.revision) {
      // Coordinator baseline is clean, but leave must still flush when the live
      // editor or remount cache diverges (e.g. edit landed in TipTap before
      // notifyDirty, or tab cache holds a remount snapshot).
      const liveMarkdown =
        path === activePathRef.current && editorReadyRef.current
          ? getLiveMarkdownRef.current()
          : null;
      const liveDiverges =
        liveMarkdown !== null && liveMarkdown !== persisted.markdown;
      const cacheDiverges =
        tabCached !== undefined && tabCached !== persisted.markdown;
      if (!liveDiverges && !cacheDiverges) {
        if (tab?.dirty && !isShellUiSuppressedForPath(path)) {
          markClean(path, resolveNoteDisplayTitle({ path }));
        }
        if (reason !== "app_close") {
          void versionSnapshotScheduler
            .savePreClose(path, persisted.markdown)
            .catch(() => undefined);
        }
        return persisted.markdown;
      }
    }
    if (!persisted) {
      if (!tab?.dirty) return tabCached ?? null;
      if (tabCached === undefined) {
        throw new Error(
          "dirty document is remounting with no recoverable snapshot",
        );
      }
      const source: DocumentPersistenceCaptureSource =
        path === activePathRef.current && !editorReadyRef.current
          ? "recovery"
          : "leave";
      const saved = await flushSaveForPath(path, source, () => tabCached);
      if (!saved) return null;
      if (reason !== "app_close") {
        enqueueLeaveSnapshot(path, saved, reason);
        void versionSnapshotScheduler.savePreClose(path, saved).catch(() => {
          // Best-effort; close must not fail because version IPC failed.
        });
      }
      return saved;
    }
    const cached =
      path === activePathRef.current && editorReadyRef.current
        ? getLiveMarkdownRef.current()
        : (tabCached ?? persisted.markdown);
    const source: DocumentPersistenceCaptureSource =
      path === activePathRef.current && !editorReadyRef.current
        ? "recovery"
        : "leave";
    const editor = editorRef.current;
    const editorHtmlSnapshot =
      path === activePathRef.current &&
      editorReadyRef.current &&
      editor &&
      !editor.isDestroyed
        ? editor.getHTML()
        : null;
    const saved = await flushSaveForPath(path, source, () => cached);
    if (!saved) return null;
    if (editorHtmlSnapshot) {
      setCachedEditorHtml(
        path,
        editorHtmlSnapshot,
        editorHtmlDigest(splitFrontmatter(saved).body),
        isClassifiedVaultPath(path) ? "classified" : "normal",
      );
    }
    if (reason !== "app_close") {
      enqueueLeaveSnapshot(path, saved, reason);
      void versionSnapshotScheduler.savePreClose(path, saved).catch(() => {
        // Best-effort; close must not fail because version IPC failed.
      });
    }
    return saved;
  }

  const { onActivity: resetVersionIdle, clearTimer: clearVersionIdleTimer } =
    useVersionIdle(activePath, getLastSavedSnapshot, enqueueIdleSnapshot, {
      enabled: autoVersionEnabled,
      idleMs: autoVersionIdleMinutes * 60 * 1000,
    });

  const releasePersistenceBarrier = useCallback(() => {
    if (!persistenceBarrierActiveRef.current) return;
    persistenceBarrierActiveRef.current = false;
    persistenceBarrierTaskRef.current = null;
    versionSnapshotScheduler.setAppClosing(false);
    onPersistenceBarrierRelease?.();
    setIsPersistenceBarrierActive(false);
  }, [onPersistenceBarrierRelease, versionSnapshotScheduler]);

  const flushAllOpenTabs = useCallback((): Promise<void> => {
    if (persistenceBarrierTaskRef.current) {
      return persistenceBarrierTaskRef.current;
    }
    persistenceBarrierActiveRef.current = true;
    onPersistenceBarrierStart?.();
    setIsPersistenceBarrierActive(true);
    versionSnapshotScheduler.setAppClosing(true);
    clearVersionIdleTimer();
    const task = (async () => {
      try {
        // Tabs can still contain a prepared/remounting snapshot that has not yet
        // been captured by the coordinator. Stage those snapshots first, then
        // make the coordinator own the completion condition. `barrierAll` keeps
        // scanning until no new revision was observed during a write pass.
        const tabsAtClose = [...tabsRef.current];
        for (const tab of tabsAtClose) {
          if (tab.lifecycle !== "session_pristine") continue;
          const snapshot =
            getTabMarkdownCached(tab.path) ??
            coordinator.get(tab.path)?.markdown;
          if (snapshot === undefined) {
            throw new Error(
              `temporary note has no discard snapshot: ${tab.path}`,
            );
          }
          await discardPristineNote(tab.path, snapshot);
        }
        await Promise.all(
          tabsAtClose
            .filter((tab) => tab.lifecycle !== "session_pristine")
            .map((tab) =>
              persistBeforeLeaveRef.current(tab.path, { reason: "app_close" }),
            ),
        );
        await coordinator.barrierAll();
        // Await pre_close markers so exit does not race the version write.
        await Promise.all(
          tabsAtClose
            .filter((tab) => tab.lifecycle !== "session_pristine")
            .map(async (tab) => {
              const markdown =
                getTabMarkdownCached(tab.path) ??
                coordinator.get(tab.path)?.markdown;
              if (!markdown) return;
              try {
                await versionSnapshotScheduler.savePreClose(tab.path, markdown);
              } catch {
                // Best-effort on exit.
              }
            }),
        );
      } catch (error) {
        releasePersistenceBarrier();
        throw error;
      }
    })();
    persistenceBarrierTaskRef.current = task;
    return task;
  }, [
    clearVersionIdleTimer,
    coordinator,
    discardPristineNote,
    getTabMarkdownCached,
    onPersistenceBarrierStart,
    persistBeforeLeaveRef,
    releasePersistenceBarrier,
    tabsRef,
    versionSnapshotScheduler,
  ]);

  useTauriCloseSave({
    flushBeforeClose: flushAllOpenTabs,
    releaseAfterCloseFailure: releasePersistenceBarrier,
    onError: () => setAiStatus("关闭前保存失败，请重试或返回编辑"),
    onBlocked: (retry) => onPersistenceBlocked?.({ action: "close", retry }),
  });

  const flushWhenEditorReady = useCallback(
    async (
      actionLabel: string,
    ): Promise<{ ok: boolean; markdown: string | null }> => {
      if (activeFileLocked) {
        setAiStatus("笔记已锁定，无法保存");
        return { ok: false, markdown: null };
      }
      try {
        const path = activePathRef.current;
        if (!path) return { ok: true, markdown: null };
        if (!editorReadyRef.current) {
          const cached = getTabMarkdownCached(path);
          if (cached === undefined) {
            setAiStatus(
              `文档仍在加载，无法${actionLabel}；未找到可安全写入的快照`,
            );
            return { ok: false, markdown: null };
          }
          return {
            ok: true,
            markdown: await flushSaveForPath(path, "recovery", () => cached),
          };
        }
        return { ok: true, markdown: await flushSave() };
      } catch (error) {
        const message = error instanceof Error ? error.message : String(error);
        setAiStatus(`${actionLabel}失败：${message}`);
        return { ok: false, markdown: null };
      }
    },
    [
      activeFileLocked,
      activePathRef,
      editorReadyRef,
      flushSave,
      flushSaveForPath,
      getTabMarkdownCached,
      setAiStatus,
    ],
  );

  const handleSaveNote = useCallback(async () => {
    await flushWhenEditorReady("保存");
  }, [flushWhenEditorReady]);

  const handleLockToggle = useCallback(
    async (locked: boolean) => {
      const path = activePathRef.current;
      if (!path || isClassifiedVaultPath(path)) return;
      try {
        if (locked && !(await flushWhenEditorReady("锁定保存")).ok) return;
        setFileLocked(path, locked);
        await fileSetLock(path, locked);
      } catch (err: unknown) {
        setFileLocked(path, !locked);
        const msg = err instanceof Error ? err.message : String(err);
        setAiStatus(`锁定状态保存失败：${msg}`);
      }
    },
    [activePathRef, flushWhenEditorReady, setAiStatus, setFileLocked],
  );

  const renamePath = useCallback(
    async (
      oldPath: string,
      newPath: string,
      markdownSnapshot: string,
      move: () => Promise<DocumentPersistenceMoveResult>,
    ): Promise<string> => {
      coordinator.capture(oldPath, markdownSnapshot, "rename");
      const snapshot = await coordinator.rename(oldPath, newPath, move);
      setSaveStatus(snapshot.status);
      setSaveError(snapshot.error);
      return snapshot.markdown;
    },
    [coordinator, setSaveError, setSaveStatus],
  );

  const beginPathMigration = useCallback(
    async (oldPath: string, newPath: string): Promise<void> => {
      await coordinator.beginPathMigration(oldPath, newPath);
    },
    [coordinator],
  );

  const completePathMigration = useCallback(
    (oldPath: string, newPath: string): string =>
      coordinator.completePathMigration(oldPath, newPath).markdown,
    [coordinator],
  );

  const abortPathMigration = useCallback(
    (oldPath: string): void => {
      coordinator.abortPathMigration(oldPath);
    },
    [coordinator],
  );

  return {
    notifyDirty,
    flushSave,
    flushWhenEditorReady,
    restoreVersion,
    restoreCurrentVersion,
    cancelPendingSave,
    awaitSaveInFlight,
    discardPristineNote,
    resetVersionIdle,
    handleSaveNote,
    handleLockToggle,
    versionSnapshotScheduler,
    flushSaveForPath,
    renamePath,
    beginPathMigration,
    completePathMigration,
    abortPathMigration,
    flushAllOpenTabs,
    clearSuppressShellUi,
    beginSuppressShellUi,
    saveStatus,
    saveError,
    hasDirtyDocuments,
    isPersistenceBarrierActive,
    isPersistenceBarrierActiveNow,
    releasePersistenceBarrier,
  };
}
