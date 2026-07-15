import { useCallback, type MutableRefObject } from "react";

import type { TabItem } from "@/components/layout/TabBar";
import type { PersistBeforeLeave } from "@/hooks/useAppPersistenceLifecycle";

interface UseNavigatorFileLifecycleParams {
  activePathRef: MutableRefObject<string | null>;
  awaitSaveInFlight: () => Promise<void>;
  abortPathMigration: (oldPath: string) => void;
  beginPathMigration: (oldPath: string, newPath: string) => Promise<void>;
  bumpVaultIndex: () => void;
  cancelPendingSave: () => void;
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
  activePathRef,
  awaitSaveInFlight,
  abortPathMigration,
  beginPathMigration,
  bumpVaultIndex,
  cancelPendingSave,
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
    handleFileDeleted,
  };
}
