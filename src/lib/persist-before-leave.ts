import {
  shouldEnqueueAutoSnapshotOnLeave,
  type AutoSnapshotLeaveReason,
} from "@/lib/version-auto-snapshot-policy";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";

export interface PersistActiveTabBeforeLeaveInput {
  path: string;
  reason: AutoSnapshotLeaveReason;
  getMarkdown: () => string;
  flushSaveForPath: (
    targetPath: string,
    getMarkdownOverride?: () => string,
  ) => Promise<string | null>;
  getLastSavedSnapshot: () => LastSavedSnapshot | null;
  enqueueIdleSnapshot: (snapshot: LastSavedSnapshot) => void;
}

/** Layer-1 flush for the active tab; may enqueue `auto_idle` per leave policy. */
export async function persistActiveTabBeforeLeave(
  input: PersistActiveTabBeforeLeaveInput,
): Promise<string | null> {
  const {
    path,
    reason,
    getMarkdown,
    flushSaveForPath,
    getLastSavedSnapshot,
    enqueueIdleSnapshot,
  } = input;

  const md = await flushSaveForPath(path, getMarkdown);
  if (!md) {
    return null;
  }

  const savedSnapshot = getLastSavedSnapshot();
  if (
    savedSnapshot &&
    savedSnapshot.path === path &&
    savedSnapshot.markdown === md &&
    shouldEnqueueAutoSnapshotOnLeave({
      reason,
      markdownLength: md.length,
    })
  ) {
    enqueueIdleSnapshot(savedSnapshot);
  }

  return md;
}

export interface PersistInactiveDirtyTabBeforeLeaveInput {
  path: string;
  reason: AutoSnapshotLeaveReason;
  cachedMarkdown: string;
  writeFile: (path: string, content: string) => Promise<void>;
  enqueueLeaveSnapshot: (
    path: string,
    markdown: string,
    reason: AutoSnapshotLeaveReason,
  ) => void;
}

/** Layer-1 write for a background dirty tab; may enqueue leave snapshot per policy. */
export async function persistInactiveDirtyTabBeforeLeave(
  input: PersistInactiveDirtyTabBeforeLeaveInput,
): Promise<string> {
  const { path, reason, cachedMarkdown, writeFile, enqueueLeaveSnapshot } =
    input;
  await writeFile(path, cachedMarkdown);
  enqueueLeaveSnapshot(path, cachedMarkdown, reason);
  return cachedMarkdown;
}
