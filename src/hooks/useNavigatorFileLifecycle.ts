import { useCallback, type MutableRefObject } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import type { PersistBeforeLeave } from "@/hooks/useAppPersistenceLifecycle";
import { resolveNoteDisplayTitle } from "@/lib/note-display";

interface UseNavigatorFileLifecycleParams {
  activePathRef: MutableRefObject<string | null>;
  awaitSaveInFlight: () => Promise<void>;
  bumpVaultIndex: () => void;
  cancelPendingSave: () => void;
  discardOpenTab: (path: string) => Promise<void>;
  getTabMarkdownCached: (path: string) => string | undefined;
  markClean: (path: string, title?: string) => void;
  markdownRef: MutableRefObject<string>;
  persistBeforeLeaveRef: MutableRefObject<PersistBeforeLeave>;
  replaceOpenTabPath: (
    oldPath: string,
    newPath: string,
    title?: string,
    markdownOverride?: string,
  ) => void;
  tabsRef: MutableRefObject<TabItem[]>;
}

export function useNavigatorFileLifecycle({
  activePathRef,
  awaitSaveInFlight,
  bumpVaultIndex,
  cancelPendingSave,
  discardOpenTab,
  getTabMarkdownCached,
  markClean,
  markdownRef,
  persistBeforeLeaveRef,
  replaceOpenTabPath,
  tabsRef,
}: UseNavigatorFileLifecycleParams) {
  const handleBeforeFilePathChange = useCallback(
    async (path: string) => {
      if (!tabsRef.current.some((tab) => tab.path === path)) return;
      await persistBeforeLeaveRef.current(path);
    },
    [persistBeforeLeaveRef, tabsRef],
  );

  const handleFilePathChanged = useCallback(
    (oldPath: string, newPath: string, title?: string) => {
      if (!tabsRef.current.some((tab) => tab.path === oldPath)) return;
      const markdownOverride =
        getTabMarkdownCached(oldPath) ??
        (activePathRef.current === oldPath ? markdownRef.current : undefined);
      replaceOpenTabPath(oldPath, newPath, title, markdownOverride);
      markClean(
        newPath,
        resolveNoteDisplayTitle({ path: newPath, title: title ?? newPath }),
      );
    },
    [
      activePathRef,
      getTabMarkdownCached,
      markClean,
      markdownRef,
      replaceOpenTabPath,
      tabsRef,
    ],
  );

  const handleBeforeFileDelete = useCallback(
    async (path: string) => {
      if (!tabsRef.current.some((tab) => tab.path === path)) return;
      if (activePathRef.current === path) {
        cancelPendingSave();
        await awaitSaveInFlight();
      }
      await discardOpenTab(path);
    },
    [
      activePathRef,
      awaitSaveInFlight,
      cancelPendingSave,
      discardOpenTab,
      tabsRef,
    ],
  );

  const handleFileDeleted = useCallback(() => {
    bumpVaultIndex();
  }, [bumpVaultIndex]);

  return {
    handleBeforeFilePathChange,
    handleFilePathChanged,
    handleBeforeFileDelete,
    handleFileDeleted,
  };
}
