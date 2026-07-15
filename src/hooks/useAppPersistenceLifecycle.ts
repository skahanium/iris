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
  type DocumentPersistenceSnapshot,
  type DocumentPersistenceStatus,
} from "@/lib/document-persistence-coordinator";
import { editorHtmlDigest, setCachedEditorHtml } from "@/lib/editor-html-cache";
import { splitFrontmatter } from "@/lib/frontmatter";
import {
  fileSetLock,
  fileWrite,
  versionSaveIdle,
  versionSaveManual,
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
}

export type PersistBeforeLeave = (
  path: string,
  options?: PersistBeforeLeaveOptions,
) => Promise<string | null>;

interface UseAppPersistenceLifecycleParams {
  activeFileLocked: boolean;
  activePath: string | null;
  activePathRef: MutableRefObject<string | null>;
  applySavedMarkdown: (markdown: string) => void;
  autoSnapshotGenerationRef: MutableRefObject<number>;
  autoVersionEnabled: boolean;
  autoVersionIdleMinutes: number;
  dirtyRef: MutableRefObject<boolean>;
  editorContentTick: number;
  editorRef: RefObject<Editor | null>;
  editorReadyRef: RefObject<boolean>;
  getLiveMarkdownRef: MutableRefObject<() => string>;
  getTabMarkdownCached: (path: string) => string | undefined;
  markClean: (path: string, title: string) => void;
  markdown: string;
  noteTitle: string;
  persistBeforeLeaveRef: MutableRefObject<PersistBeforeLeave>;
  schedulePathSync: (path: string, title: string) => void;
  setAiStatus: (status: string) => void;
  setFileLocked: (path: string, locked: boolean) => void;
  setMarkdown: (markdown: string) => void;
  syncTabMarkdownCache: (path: string, markdown: string) => void;
  tabsRef: MutableRefObject<TabItem[]>;
}

function isSavedStatus(status: DocumentPersistenceStatus): boolean {
  return status === "saved" || status === "saved_index_degraded";
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
  editorContentTick,
  editorRef,
  editorReadyRef,
  getLiveMarkdownRef,
  getTabMarkdownCached,
  markClean,
  markdown,
  noteTitle,
  persistBeforeLeaveRef,
  schedulePathSync,
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
  const cancelledWriteRef = useRef<Promise<void>>(Promise.resolve());
  const noteTitleRef = useRef(noteTitle);
  noteTitleRef.current = noteTitle;

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
      if (!isSavedStatus(snapshot.status) && snapshot.status !== "clean") {
        return;
      }
      const tab = tabsRef.current.find((item) => item.path === snapshot.path);
      const title =
        snapshot.path === activePathRef.current
          ? noteTitleRef.current
          : (tab?.title ?? snapshot.path);
      syncTabMarkdownCache(snapshot.path, snapshot.markdown);
      markClean(
        snapshot.path,
        resolveNoteDisplayTitle({ path: snapshot.path, title }),
      );
      if (snapshot.path !== activePathRef.current) return;
      applySavedMarkdown(snapshot.markdown);
      dirtyRef.current = false;
      setMarkdown(snapshot.markdown);
      if (noteTitleRef.current.trim() === "") {
        schedulePathSync(snapshot.path, noteTitleRef.current);
      }
    },
    [
      activePathRef,
      applySavedMarkdown,
      dirtyRef,
      markClean,
      schedulePathSync,
      setMarkdown,
      syncTabMarkdownCache,
      tabsRef,
    ],
  );

  useEffect(() => {
    return coordinator.subscribe((snapshot) => {
      if (snapshot.path === activePathRef.current) {
        setSaveStatus(snapshot.status);
        setSaveError(snapshot.error);
      }
      acknowledgeSnapshot(snapshot);
    });
  }, [acknowledgeSnapshot, activePathRef, coordinator]);

  useEffect(() => {
    const path = activePathRef.current;
    if (!path) return;
    coordinator.load(path, markdown);
    // `editorContentTick` denotes only an authoritative disk/prepared load.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [coordinator, editorContentTick]);

  const versionSnapshotScheduler = useMemo(
    () =>
      createVersionSnapshotScheduler({
        versionSaveIdle,
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
      coordinator.capture(path, markdownSnapshot);
      return (await coordinator.barrier(path)).markdown;
    },
    [activePathRef, coordinator, getLiveMarkdownRef, getTabMarkdownCached],
  );

  const flushSave = useCallback(async (): Promise<string | null> => {
    const path = activePathRef.current;
    if (!path) return null;
    return flushSaveForPath(path);
  }, [activePathRef, flushSaveForPath]);

  const notifyDirty = useCallback(() => {
    const path = activePathRef.current;
    if (!path || !editorReadyRef.current) return;
    coordinator.capture(path, getLiveMarkdownRef.current());
  }, [activePathRef, coordinator, editorReadyRef, getLiveMarkdownRef]);

  const cancelPendingSave = useCallback(() => {
    const path = activePathRef.current;
    if (!path) return;
    cancelledWriteRef.current = coordinator.discard(path);
  }, [activePathRef, coordinator]);

  const awaitSaveInFlight = useCallback(async (): Promise<void> => {
    await cancelledWriteRef.current;
  }, []);

  persistBeforeLeaveRef.current = async (
    path: string,
    options: PersistBeforeLeaveOptions = {},
  ) => {
    const reason = options.reason ?? "tab_leave";
    const tab = tabsRef.current.find((item) => item.path === path);
    if (!tab?.dirty) {
      return (
        getTabMarkdownCached(path) ?? coordinator.get(path)?.markdown ?? null
      );
    }
    const cached =
      path === activePathRef.current && editorReadyRef.current
        ? getLiveMarkdownRef.current()
        : getTabMarkdownCached(path);
    if (cached === undefined) {
      throw new Error(
        "dirty document is remounting with no recoverable snapshot",
      );
    }
    const editor = editorRef.current;
    const editorHtmlSnapshot =
      path === activePathRef.current &&
      editorReadyRef.current &&
      editor &&
      !editor.isDestroyed
        ? editor.getHTML()
        : null;
    const saved = await flushSaveForPath(path, () => cached);
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
    }
    return saved;
  };

  const { onActivity: resetVersionIdle, clearTimer: clearVersionIdleTimer } =
    useVersionIdle(activePath, getLastSavedSnapshot, enqueueIdleSnapshot, {
      enabled: autoVersionEnabled,
      idleMs: autoVersionIdleMinutes * 60 * 1000,
    });

  const flushAllOpenTabs = useCallback(async (): Promise<void> => {
    versionSnapshotScheduler.setAppClosing(true);
    clearVersionIdleTimer();
    try {
      for (const tab of tabsRef.current) {
        await persistBeforeLeaveRef.current(tab.path, { reason: "app_close" });
      }
    } finally {
      versionSnapshotScheduler.setAppClosing(false);
    }
  }, [
    clearVersionIdleTimer,
    persistBeforeLeaveRef,
    tabsRef,
    versionSnapshotScheduler,
  ]);

  useTauriCloseSave({
    flushBeforeClose: flushAllOpenTabs,
    onError: (message) => setAiStatus(`关闭前保存失败：${message}`),
  });

  const flushWhenEditorReady = useCallback(
    async (
      actionLabel: string,
    ): Promise<{ ok: boolean; markdown: string | null }> => {
      if (activeFileLocked) {
        setAiStatus("笔记已锁定，无法保存");
        return { ok: false, markdown: null };
      }
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
          markdown: await flushSaveForPath(path, () => cached),
        };
      }
      return { ok: true, markdown: await flushSave() };
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

  const handleSaveVersion = useCallback(async () => {
    const path = activePathRef.current;
    if (!path) return;
    if (!editorReadyRef.current) {
      setAiStatus("文档仍在加载，无法定稿；未修改磁盘内容");
      return;
    }
    const saved = await flushSave();
    if (!saved) return;
    setAiStatus("正在后台创建版本快照…");
    versionSnapshotScheduler.markHighPriorityStart(path);
    void versionSaveManual(path, saved)
      .catch((err: unknown) => {
        const msg = err instanceof Error ? err.message : String(err);
        setAiStatus(`版本快照提交失败：${msg}`);
      })
      .finally(() => versionSnapshotScheduler.markHighPriorityEnd(path));
  }, [
    activePathRef,
    editorReadyRef,
    flushSave,
    setAiStatus,
    versionSnapshotScheduler,
  ]);

  const renamePath = useCallback(
    async (
      oldPath: string,
      newPath: string,
      markdownSnapshot: string,
      move: () => Promise<string>,
    ): Promise<string> => {
      coordinator.capture(oldPath, markdownSnapshot);
      return (await coordinator.rename(oldPath, newPath, move)).markdown;
    },
    [coordinator],
  );

  return {
    notifyDirty,
    flushSave,
    flushWhenEditorReady,
    cancelPendingSave,
    awaitSaveInFlight,
    resetVersionIdle,
    handleSaveNote,
    handleLockToggle,
    handleSaveVersion,
    versionSnapshotScheduler,
    flushSaveForPath,
    renamePath,
    flushAllOpenTabs,
    saveStatus,
    saveError,
  };
}
