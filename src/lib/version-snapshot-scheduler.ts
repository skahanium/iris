import { isNoteSubstantivelyEmpty } from "@/lib/note-substance";
import type { VersionEntry, VersionSaveOutcome } from "@/types/ipc";

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
  /** Creates a manual version after prior writes for this document complete. */
  saveManual(path: string, content: string): Promise<VersionSaveOutcome>;
  /** Finalizes a version after prior writes for this document complete. */
  finalize(
    path: string,
    content: string,
    label: string | null,
  ): Promise<VersionEntry | null>;
  setAppClosing(closing: boolean): void;
  getStats(): VersionSnapshotSchedulerStats;
}

interface VersionSnapshotSchedulerDeps {
  versionSaveIdle: (
    path: string,
    content: string,
  ) => Promise<VersionSaveOutcome | void>;
  versionSaveManual?: (
    path: string,
    content: string,
  ) => Promise<VersionSaveOutcome>;
  versionFinalizeCurrent?: (
    path: string,
    content: string,
    label: string | null,
  ) => Promise<VersionEntry | null>;
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
  versionSaveManual,
  versionFinalizeCurrent,
  onError,
}: VersionSnapshotSchedulerDeps): VersionSnapshotScheduler {
  const inFlightPaths = new Set<string>();
  const inFlightGenerations = new Set<string>();
  const completedGenerations = new Set<string>();
  const highPriorityCounts = new Map<string, number>();
  const pathTails = new Map<string, Promise<void>>();
  let appClosing = false;
  const stats = emptyStats();

  const skip = (
    reason: VersionSnapshotSkipReason,
  ): VersionSnapshotEnqueueResult => {
    stats.skipped[reason] += 1;
    return { accepted: false, reason };
  };

  const markHighPriorityStart = (path: string): void => {
    highPriorityCounts.set(path, (highPriorityCounts.get(path) ?? 0) + 1);
  };

  const markHighPriorityEnd = (path: string): void => {
    const next = (highPriorityCounts.get(path) ?? 0) - 1;
    if (next > 0) {
      highPriorityCounts.set(path, next);
    } else {
      highPriorityCounts.delete(path);
    }
  };

  /**
   * Runs version writes FIFO per document while preserving concurrency between
   * different documents. A rejected operation is absorbed by the tail, so it
   * cannot strand later writes for the same document.
   */
  const enqueueForPath = <T>(
    path: string,
    operation: () => Promise<T>,
  ): Promise<T> => {
    const previous = pathTails.get(path) ?? Promise.resolve();
    const queued = previous.then(operation, operation);
    const tail = queued.then(
      () => undefined,
      () => undefined,
    );
    pathTails.set(path, tail);
    void tail.then(() => {
      if (pathTails.get(path) === tail) {
        pathTails.delete(path);
      }
    });
    return queued;
  };

  const enqueueHighPriority = <T>(
    path: string,
    operation: () => Promise<T>,
  ): Promise<T> => {
    markHighPriorityStart(path);
    return enqueueForPath(path, operation).finally(() => {
      markHighPriorityEnd(path);
    });
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
      if (completedGenerations.has(key) || inFlightGenerations.has(key)) {
        return skip("same_generation");
      }

      if (inFlightPaths.has(snapshot.path)) {
        return skip("in_flight");
      }

      inFlightPaths.add(snapshot.path);
      inFlightGenerations.add(key);
      stats.enqueued += 1;

      const done = enqueueForPath(snapshot.path, async () => {
        try {
          await versionSaveIdle(snapshot.path, snapshot.markdown);
          completedGenerations.add(key);
          stats.completed += 1;
        } catch (error: unknown) {
          stats.failed += 1;
          onError?.(error);
        } finally {
          inFlightPaths.delete(snapshot.path);
          inFlightGenerations.delete(key);
        }
      });

      return { accepted: true, done };
    },

    saveManual(path, content) {
      if (!versionSaveManual) {
        return Promise.reject(
          new Error("manual version writer is not configured"),
        );
      }
      return enqueueHighPriority(path, () => versionSaveManual(path, content));
    },

    finalize(path, content, label) {
      if (!versionFinalizeCurrent) {
        return Promise.reject(
          new Error("finalize version writer is not configured"),
        );
      }
      return enqueueHighPriority(path, () =>
        versionFinalizeCurrent(path, content, label),
      );
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
