import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";

export interface LastSavedSnapshot {
  path: string;
  markdown: string;
  savedAt: number;
  dirtyGeneration: number;
}

export type VersionSnapshotSkipReason =
  | "empty"
  | "high_priority_active"
  | "in_flight"
  | "same_generation"
  | "app_closing";

export type VersionSnapshotEnqueueResult =
  | { accepted: true; done: Promise<void> }
  | { accepted: false; reason: VersionSnapshotSkipReason };

export interface VersionSnapshotSchedulerStats {
  enqueued: number;
  completed: number;
  failed: number;
  skipped: Record<VersionSnapshotSkipReason, number>;
}

export interface VersionSnapshotScheduler {
  enqueueIdle(snapshot: LastSavedSnapshot): VersionSnapshotEnqueueResult;
  markHighPriorityStart(path: string): void;
  markHighPriorityEnd(path: string): void;
  setAppClosing(closing: boolean): void;
  getStats(): VersionSnapshotSchedulerStats;
}

interface VersionSnapshotSchedulerDeps {
  versionSaveIdle: (path: string, content: string) => Promise<void>;
  now?: () => number;
  onError?: (error: unknown) => void;
}

function emptyStats(): VersionSnapshotSchedulerStats {
  return {
    enqueued: 0,
    completed: 0,
    failed: 0,
    skipped: {
      empty: 0,
      high_priority_active: 0,
      in_flight: 0,
      same_generation: 0,
      app_closing: 0,
    },
  };
}

function generationKey(snapshot: LastSavedSnapshot): string {
  return `${snapshot.path}:${snapshot.dirtyGeneration}`;
}

export function createVersionSnapshotScheduler({
  versionSaveIdle,
  onError,
}: VersionSnapshotSchedulerDeps): VersionSnapshotScheduler {
  const inFlightPaths = new Set<string>();
  const completedGenerations = new Set<string>();
  const highPriorityCounts = new Map<string, number>();
  let appClosing = false;
  const stats = emptyStats();

  const skip = (
    reason: VersionSnapshotSkipReason,
  ): VersionSnapshotEnqueueResult => {
    stats.skipped[reason] += 1;
    return { accepted: false, reason };
  };

  return {
    enqueueIdle(snapshot) {
      if (appClosing) {
        return skip("app_closing");
      }

      if (isNoteSubstantivelyEmpty(snapshot.markdown)) {
        return skip("empty");
      }

      if ((highPriorityCounts.get(snapshot.path) ?? 0) > 0) {
        return skip("high_priority_active");
      }

      const key = generationKey(snapshot);
      if (completedGenerations.has(key)) {
        return skip("same_generation");
      }

      if (inFlightPaths.has(snapshot.path)) {
        return skip("in_flight");
      }

      inFlightPaths.add(snapshot.path);
      completedGenerations.add(key);
      stats.enqueued += 1;

      const done = versionSaveIdle(snapshot.path, snapshot.markdown)
        .then(() => {
          stats.completed += 1;
        })
        .catch((error: unknown) => {
          stats.failed += 1;
          onError?.(error);
        })
        .finally(() => {
          inFlightPaths.delete(snapshot.path);
        });

      return { accepted: true, done };
    },

    markHighPriorityStart(path) {
      highPriorityCounts.set(path, (highPriorityCounts.get(path) ?? 0) + 1);
    },

    markHighPriorityEnd(path) {
      const next = (highPriorityCounts.get(path) ?? 0) - 1;
      if (next > 0) {
        highPriorityCounts.set(path, next);
      } else {
        highPriorityCounts.delete(path);
      }
    },

    setAppClosing(closing: boolean) {
      appClosing = closing;
    },

    getStats() {
      return {
        enqueued: stats.enqueued,
        completed: stats.completed,
        failed: stats.failed,
        skipped: { ...stats.skipped },
      };
    },
  };
}
