import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useEffect,
  useMemo,
  useRef,
  type MutableRefObject,
  type RefObject,
} from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useTauriCloseSave } from "@/hooks/useTauriCloseSave";
import { useVersionIdle } from "@/hooks/useVersionIdle";
import {
  fileSetLock,
  fileWrite,
  versionSaveIdle,
  versionSaveManual,
} from "@/lib/ipc";
import { editorHtmlDigest, setCachedEditorHtml } from "@/lib/editor-html-cache";
import { isClassifiedVaultPath } from "@/lib/classified-path";
import { splitFrontmatter } from "@/lib/frontmatter";
import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import { resolveNoteDisplayTitle } from "@/lib/note-display";
import type { AutoSnapshotLeaveReason } from "@/lib/version-auto-snapshot-policy";
import { createLeaveSnapshotEnqueuer } from "@/lib/version-leave-snapshot";
import {
  persistActiveTabBeforeLeave,
  persistInactiveDirtyTabBeforeLeave,
} from "@/lib/persist-before-leave";
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
  const {
    notifyDirty,
    flushSave,
    flushSaveForPath,
    cancelPendingSave,
    awaitSaveInFlight,
    getLastSavedSnapshot,
    recordSavedSnapshot,
  } = useEditorSave(
    activePath,
    () => getLiveMarkdownRef.current(),
    (md) => {
      applySavedMarkdown(md);
      dirtyRef.current = false;
      const path = activePathRef.current;
      if (path) {
        setMarkdown(md);
        syncTabMarkdownCache(path, md);
        markClean(path, resolveNoteDisplayTitle({ path, title: noteTitle }));
        if (noteTitle.trim() === "") {
          schedulePathSync(path, noteTitle);
        }
      }
    },
  );

  const markdownBaselineRef = useRef(markdown);
  markdownBaselineRef.current = markdown;

  useEffect(() => {
    if (!activePath) return;
    recordSavedSnapshot(activePath, markdownBaselineRef.current);
  }, [activePath, editorContentTick, recordSavedSnapshot]);

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
      if (result.accepted) {
        void result.done;
      }
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

  persistBeforeLeaveRef.current = async (
    path: string,
    options: PersistBeforeLeaveOptions = {},
  ) => {
    const reason = options.reason ?? "tab_leave";
    const tab = tabsRef.current.find((t) => t.path === path);
    if (path === activePathRef.current) {
      if (!editorReadyRef.current) {
        setAiStatus("文档仍在加载，已跳过自动保存以保护磁盘内容");
        return null;
      }
      const markdownSnapshot = getLiveMarkdownRef.current();
      const titleSnapshot = noteTitle;
      const editor = editorRef.current;
      const editorHtmlSnapshot =
        editorReadyRef.current && editor && !editor.isDestroyed
          ? editor.getHTML()
          : null;
      const namespace = isClassifiedVaultPath(path) ? "classified" : "normal";
      syncTabMarkdownCache(path, markdownSnapshot);
      const md = await persistActiveTabBeforeLeave({
        path,
        reason,
        getMarkdown: () => markdownSnapshot,
        flushSaveForPath,
        getLastSavedSnapshot,
        enqueueIdleSnapshot,
      });
      if (md) {
        syncTabMarkdownCache(path, md);
        if (editorHtmlSnapshot) {
          setCachedEditorHtml(
            path,
            editorHtmlSnapshot,
            editorHtmlDigest(splitFrontmatter(md).body),
            namespace,
          );
        }
        markClean(
          path,
          resolveNoteDisplayTitle({ path, title: titleSnapshot }),
        );
        if (activePathRef.current === path) {
          applySavedMarkdown(md);
          dirtyRef.current = false;
          setMarkdown(md);
          if (titleSnapshot.trim() === "") {
            schedulePathSync(path, titleSnapshot);
          }
        }
      }
      return md;
    }
    if (!tab?.dirty) {
      return getTabMarkdownCached(path) ?? null;
    }
    const cached = getTabMarkdownCached(path);
    if (!cached || isNoteSubstantivelyEmpty(cached)) {
      return null;
    }
    await persistInactiveDirtyTabBeforeLeave({
      path,
      reason,
      cachedMarkdown: cached,
      writeFile: async (targetPath, content) => {
        await fileWrite(targetPath, content);
      },
      enqueueLeaveSnapshot,
    });
    markClean(path, tab.title);
    return cached;
  };

  const { onActivity: resetVersionIdle, clearTimer: clearVersionIdleTimer } =
    useVersionIdle(activePath, getLastSavedSnapshot, enqueueIdleSnapshot, {
      enabled: autoVersionEnabled,
      idleMs: autoVersionIdleMinutes * 60 * 1000,
    });

  const flushAllOpenTabs = useCallback(async () => {
    const paths = tabsRef.current.map((tab) => tab.path);
    versionSnapshotScheduler.setAppClosing(true);
    clearVersionIdleTimer();
    try {
      for (const path of paths) {
        await persistBeforeLeaveRef.current(path, { reason: "app_close" });
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
    onError: (message) => {
      setAiStatus(`关闭前保存失败：${message}`);
    },
  });

  const flushWhenEditorReady = useCallback(
    async (
      actionLabel: string,
    ): Promise<{ ok: boolean; markdown: string | null }> => {
      if (activeFileLocked) {
        setAiStatus("笔记已锁定，无法保存");
        return { ok: false, markdown: null };
      }
      if (activePathRef.current && !editorReadyRef.current) {
        setAiStatus(`文档仍在加载，无法${actionLabel}；未修改磁盘内容`);
        return { ok: false, markdown: null };
      }
      const markdown = await flushSave();
      return { ok: true, markdown };
    },
    [activeFileLocked, activePathRef, editorReadyRef, flushSave, setAiStatus],
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
    const md = await flushSave();
    if (!md) return;
    setAiStatus("正在后台创建版本快照…");
    versionSnapshotScheduler.markHighPriorityStart(path);
    void versionSaveManual(path, md)
      .catch((err: unknown) => {
        const msg = err instanceof Error ? err.message : String(err);
        setAiStatus(`版本快照提交失败：${msg}`);
      })
      .finally(() => {
        versionSnapshotScheduler.markHighPriorityEnd(path);
      });
  }, [
    activePathRef,
    editorReadyRef,
    flushSave,
    setAiStatus,
    versionSnapshotScheduler,
  ]);

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
  };
}
