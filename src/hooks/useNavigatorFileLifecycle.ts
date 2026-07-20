import { useCallback, type MutableRefObject } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import type { PersistBeforeLeave } from "@/hooks/useAppPersistenceLifecycle";

interface UseNavigatorFileLifecycleParams {
  abortPathMigration: (oldPath: string) => void;
  beginPathMigration: (oldPath: string, newPath: string) => Promise<void>;
  bumpVaultIndex: () => void;
  completePathMigration: (oldPath: string, newPath: string) => string;
  discardOpenTab: (path: string) => Promise<void>;
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
  abortPathMigration,
  beginPathMigration,
  bumpVaultIndex,
  completePathMigration,
  discardOpenTab,
  persistBeforeLeaveRef,
  replaceOpenTabPath,
  tabsRef,
}: UseNavigatorFileLifecycleParams) {
  const handleBeforeFilePathChange = useCallback(
    async (oldPath: string, newPath: string) => {
      if (!tabsRef.current.some((tab) => tab.path === oldPath)) return;
      await persistBeforeLeaveRef.current(oldPath);
      await beginPathMigration(oldPath, newPath);
    },
    [beginPathMigration, persistBeforeLeaveRef, tabsRef],
  );

  const handleFilePathChanged = useCallback(
    (oldPath: string, newPath: string, title?: string) => {
      if (!tabsRef.current.some((tab) => tab.path === oldPath)) return;
      const markdown = completePathMigration(oldPath, newPath);
      replaceOpenTabPath(oldPath, newPath, title, markdown);
    },
    [completePathMigration, replaceOpenTabPath, tabsRef],
  );

  const handleFilePathChangeFailed = useCallback(
    (oldPath: string) => {
      abortPathMigration(oldPath);
    },
    [abortPathMigration],
  );

  const handleBeforeFileDelete = useCallback(
    async (path: string) => {
      if (!tabsRef.current.some((tab) => tab.path === path)) return;
      await persistBeforeLeaveRef.current(path);
      await discardOpenTab(path);
    },
    [discardOpenTab, persistBeforeLeaveRef, tabsRef],
  );

  /** Flush open dirty notes before locking so the lock captures the latest body. */
  const handleBeforeFileLock = useCallback(
    async (path: string) => {
      if (!tabsRef.current.some((tab) => tab.path === path)) return;
      await persistBeforeLeaveRef.current(path);
    },
    [persistBeforeLeaveRef, tabsRef],
  );

  const handleFileDeleted = useCallback(
    (_path?: string) => {
      bumpVaultIndex();
    },
    [bumpVaultIndex],
  );

  return {
    handleBeforeFilePathChange,
    handleFilePathChanged,
    handleFilePathChangeFailed,
    handleBeforeFileDelete,
    handleBeforeFileLock,
    handleFileDeleted,
  };
}
