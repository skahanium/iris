import type { Editor } from "@tiptap/react";
import {
  useCallback,
  useMemo,
  type MutableRefObject,
  type RefObject,
} from "react";

import type { TabItem } from "@/components/layout/TabBar";
import { useEditorSave } from "@/hooks/useEditorSave";
import { useTauriCloseSave } from "@/hooks/useTauriCloseSave";
import { useVersionIdle } from "@/hooks/useVersionIdle";
import { fileWrite, versionSaveIdle, versionSaveManual } from "@/lib/ipc";
import { editorHtmlDigest, setCachedEditorHtml } from "@/lib/editor-html-cache";
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
import { waitForEditorRef } from "@/lib/wait-for-editor";

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
  dirtyRef: MutableRefObject<boolean>;
  editorRef: RefObject<Editor | null>;
  getLiveMarkdownRef: MutableRefObject<() => string>;
  getTabMarkdownCached: (path: string) => string | undefined;
  markClean: (path: string, title: string) => void;
  noteTitle: string;
  persistBeforeLeaveRef: MutableRefObject<PersistBeforeLeave>;
  schedulePathSync: (path: string, title: string) => void;
  setAiStatus: (status: string) => void;
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
  dirtyRef,
  editorRef,
  getLiveMarkdownRef,
  getTabMarkdownCached,
  markClean,
  noteTitle,
  persistBeforeLeaveRef,
  schedulePathSync,
  setAiStatus,
  setMarkdown,
  syncTabMarkdownCache,
  tabsRef,
}: UseAppPersistenceLifecycleParams) {
  const { notifyDirty, flushSave, flushSaveForPath, getLastSavedSnapshot } =
    useEditorSave(
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
      const result = versionSnapshotScheduler.enqueueIdle(snapshot);
      if (result.accepted) {
        void result.done;
      }
    },
    [versionSnapshotScheduler],
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
      await waitForEditorRef(editorRef);
      const md = await persistActiveTabBeforeLeave({
        path,
        reason,
        getMarkdown: () => getLiveMarkdownRef.current(),
        flushSaveForPath,
        getLastSavedSnapshot,
        enqueueIdleSnapshot,
      });
      if (md) {
        dirtyRef.current = false;
        setMarkdown(md);
        syncTabMarkdownCache(path, md);
        const ed = editorRef.current;
        if (ed && !ed.isDestroyed) {
          setCachedEditorHtml(
            path,
            ed.getHTML(),
            editorHtmlDigest(splitFrontmatter(md).body),
          );
        }
        markClean(path, resolveNoteDisplayTitle({ path, title: noteTitle }));
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
    useVersionIdle(activePath, getLastSavedSnapshot, enqueueIdleSnapshot);

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

  const handleSaveNote = useCallback(async () => {
    if (activeFileLocked) {
      setAiStatus("笔记已锁定，无法保存");
      return;
    }
    await flushSave();
  }, [activeFileLocked, flushSave, setAiStatus]);

  const handleSaveVersion = useCallback(async () => {
    const path = activePathRef.current;
    if (!path) return;
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
  }, [activePathRef, flushSave, setAiStatus, versionSnapshotScheduler]);

  return {
    notifyDirty,
    flushSave,
    resetVersionIdle,
    handleSaveNote,
    handleSaveVersion,
    versionSnapshotScheduler,
  };
}
