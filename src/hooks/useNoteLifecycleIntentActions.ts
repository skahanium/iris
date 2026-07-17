import { useCallback, type MutableRefObject } from "react";

interface UseNoteLifecycleIntentActionsOptions {
  activePathRef: MutableRefObject<string | null>;
  bumpVaultIndex: () => void;
  flushWhenEditorReady: (
    actionLabel: string,
  ) => Promise<{ ok: boolean; markdown: string | null }>;
  handleLockToggle: (locked: boolean) => Promise<void>;
  handleSaveNote: () => Promise<void>;
  promoteTab: (path: string) => void;
  restoreCurrentVersion: (content: string) => Promise<void>;
}

/**
 * Promotes a session-pristine tab before an explicit user action makes the
 * temporary disk file worth retaining permanently.
 */
export function useNoteLifecycleIntentActions({
  activePathRef,
  bumpVaultIndex,
  flushWhenEditorReady,
  handleLockToggle,
  handleSaveNote,
  promoteTab,
  restoreCurrentVersion,
}: UseNoteLifecycleIntentActionsOptions) {
  const promoteActiveTab = useCallback(() => {
    const path = activePathRef.current;
    if (path) promoteTab(path);
  }, [activePathRef, promoteTab]);

  const handleSaveNoteWithPromotion = useCallback(async () => {
    promoteActiveTab();
    await handleSaveNote();
  }, [handleSaveNote, promoteActiveTab]);

  const handleLockToggleWithPromotion = useCallback(
    async (locked: boolean) => {
      promoteActiveTab();
      await handleLockToggle(locked);
    },
    [handleLockToggle, promoteActiveTab],
  );

  const restoreCurrentVersionWithPromotion = useCallback(
    async (content: string) => {
      promoteActiveTab();
      await restoreCurrentVersion(content);
    },
    [promoteActiveTab, restoreCurrentVersion],
  );

  const finalizeCurrentWithPromotion = useCallback(async () => {
    promoteActiveTab();
    const result = await flushWhenEditorReady("定稿");
    if (result.ok && result.markdown) {
      bumpVaultIndex();
      return result.markdown;
    }
    return null;
  }, [bumpVaultIndex, flushWhenEditorReady, promoteActiveTab]);

  return {
    finalizeCurrentWithPromotion,
    handleLockToggleWithPromotion,
    handleSaveNoteWithPromotion,
    restoreCurrentVersionWithPromotion,
  };
}
