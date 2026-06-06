import { describe, expect, it, vi } from "vitest";

import {
  persistActiveTabBeforeLeave,
  persistInactiveDirtyTabBeforeLeave,
} from "@/lib/persist-before-leave";
import { createLeaveSnapshotEnqueuer } from "@/lib/version-leave-snapshot";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";
import {
  createVersionSnapshotScheduler,
  type VersionSnapshotEnqueueResult,
} from "@/lib/version-snapshot-scheduler";

/** Mirrors `App.flushAllOpenTabs` close guard + per-tab persist. */
async function flushTabsOnAppClose(
  paths: string[],
  deps: {
    persistActive: (path: string) => Promise<string | null>;
    persistInactive: (path: string, cached: string) => Promise<string>;
    isActive: (path: string) => boolean;
    getCached: (path: string) => string | null;
    setAppClosing: (closing: boolean) => void;
    clearVersionIdleTimer: () => void;
  },
): Promise<void> {
  deps.setAppClosing(true);
  deps.clearVersionIdleTimer();
  try {
    for (const path of paths) {
      if (deps.isActive(path)) {
        await deps.persistActive(path);
      } else {
        const cached = deps.getCached(path);
        if (cached) {
          await deps.persistInactive(path, cached);
        }
      }
    }
  } finally {
    deps.setAppClosing(false);
  }
}

describe("app close version guard", () => {
  it("flushing multiple tabs during app close never calls versionSaveIdle", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });
    let generation = 0;
    const enqueueIdleSnapshot = (snapshot: LastSavedSnapshot) => {
      const result: VersionSnapshotEnqueueResult =
        scheduler.enqueueIdle(snapshot);
      if (result.accepted) {
        void result.done;
      }
    };
    const enqueueLeaveSnapshot = createLeaveSnapshotEnqueuer({
      enqueueIdleSnapshot,
      nextDirtyGeneration: () => {
        generation += 1;
        return generation;
      },
    });

    const snapshots = new Map<string, LastSavedSnapshot>([
      [
        "notes/active.md",
        {
          path: "notes/active.md",
          markdown: "active saved",
          savedAt: 1,
          dirtyGeneration: 1,
        },
      ],
    ]);

    const clearVersionIdleTimer = vi.fn();

    await flushTabsOnAppClose(["notes/active.md", "notes/background.md"], {
      setAppClosing: (closing) => scheduler.setAppClosing(closing),
      clearVersionIdleTimer,
      isActive: (path) => path === "notes/active.md",
      getCached: (path) =>
        path === "notes/background.md" ? "background cached" : null,
      persistActive: (path) =>
        persistActiveTabBeforeLeave({
          path,
          reason: "app_close",
          getMarkdown: () => "active live",
          flushSaveForPath: async () => "active saved",
          getLastSavedSnapshot: () => snapshots.get(path) ?? null,
          enqueueIdleSnapshot,
        }),
      persistInactive: (path, cached) =>
        persistInactiveDirtyTabBeforeLeave({
          path,
          reason: "app_close",
          cachedMarkdown: cached,
          writeFile: async () => undefined,
          enqueueLeaveSnapshot,
        }),
    });

    expect(clearVersionIdleTimer).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).not.toHaveBeenCalled();
    expect(scheduler.getStats().skipped.app_closing).toBe(0);
  });

  it("rejects idle enqueue while appClosing even if leave policy were bypassed", () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });
    scheduler.setAppClosing(true);

    const result = scheduler.enqueueIdle({
      path: "notes/a.md",
      markdown: "body",
      savedAt: 1,
      dirtyGeneration: 1,
    });

    scheduler.setAppClosing(false);
    expect(result).toEqual({ accepted: false, reason: "app_closing" });
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });
});
