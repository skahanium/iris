import {
  shouldEnqueueAutoSnapshotOnLeave,
  type AutoSnapshotLeaveReason,
} from "@/lib/version-auto-snapshot-policy";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";

export type EnqueueLeaveSnapshot = (
  path: string,
  markdown: string,
  reason: AutoSnapshotLeaveReason,
) => void;

/** Best-effort `auto_idle` enqueue when leaving a tab (respects leave policy). */
export function createLeaveSnapshotEnqueuer(deps: {
  enqueueIdleSnapshot: (snapshot: LastSavedSnapshot) => void;
  nextDirtyGeneration: () => number;
  now?: () => number;
}): EnqueueLeaveSnapshot {
  const now = deps.now ?? (() => Date.now());

  return (path, markdown, reason) => {
    if (
      !shouldEnqueueAutoSnapshotOnLeave({
        reason,
        markdownLength: markdown.length,
      })
    ) {
      return;
    }

    deps.enqueueIdleSnapshot({
      path,
      markdown,
      savedAt: now(),
      dirtyGeneration: deps.nextDirtyGeneration(),
    });
  };
}
