import type { MutableRefObject } from "react";

import {
  useCurrentFileChangeListener,
  type ConflictState,
} from "@/hooks/useCurrentFileChangeListener";

interface UseExternalDocumentLifecycleOptions {
  activePathRef: MutableRefObject<string | null>;
  awaitSaveInFlight: () => Promise<void>;
  bumpVaultIndex: () => void;
  cancelPendingSave: () => void;
  discardOpenTab: (path: string) => Promise<void>;
  getLiveMarkdownRef: MutableRefObject<() => string>;
  invalidatePreparedNote: (path: string) => void;
  promoteTab: (path: string) => void;
  setConflictState: (state: ConflictState | null) => void;
}

/** Keeps external file changes from bypassing the note lifecycle boundary. */
export function useExternalDocumentLifecycle({
  activePathRef,
  awaitSaveInFlight,
  bumpVaultIndex,
  cancelPendingSave,
  discardOpenTab,
  getLiveMarkdownRef,
  invalidatePreparedNote,
  promoteTab,
  setConflictState,
}: UseExternalDocumentLifecycleOptions): void {
  useCurrentFileChangeListener({
    activePathRef,
    awaitSaveInFlight,
    bumpVaultIndex,
    cancelPendingSave,
    discardOpenTab,
    getLiveMarkdownRef,
    onFileChanged: invalidatePreparedNote,
    onExternalModification: promoteTab,
    setConflictState,
  });
}
