import { describe, expect, it, vi } from "vitest";

import { DocumentPersistenceCoordinator } from "@/lib/document-persistence-coordinator";
import type { LastSavedSnapshot } from "@/lib/version-snapshot-scheduler";
import { createVersionSnapshotScheduler } from "@/lib/version-snapshot-scheduler";

describe("app close version guard", () => {
  it("flushing multiple tabs during app close never calls versionSaveIdle", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });
    const write = vi.fn(async () => ({ indexDegraded: false }));
    const coordinator = new DocumentPersistenceCoordinator({ write });
    coordinator.load("notes/active.md", "active opened");
    coordinator.load("notes/background.md", "background opened");
    coordinator.capture("notes/active.md", "active saved");
    coordinator.capture("notes/background.md", "background cached");

    const clearVersionIdleTimer = vi.fn();
    const flushAllOpenTabs = async () => {
      scheduler.setAppClosing(true);
      clearVersionIdleTimer();
      try {
        for (const path of ["notes/active.md", "notes/background.md"]) {
          await coordinator.barrier(path);
        }
      } finally {
        scheduler.setAppClosing(false);
      }
    };

    await flushAllOpenTabs();

    expect(write).toHaveBeenCalledTimes(2);
    expect(clearVersionIdleTimer).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("does not enqueue leave snapshots while the app-close barrier is active", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });
    scheduler.setAppClosing(true);
    const enqueueIdleSnapshot = (snapshot: LastSavedSnapshot) => {
      const result = scheduler.enqueueIdle(snapshot);
      if (result.accepted) {
        void result.done;
      }
    };
    enqueueIdleSnapshot({
      path: "notes/a.md",
      markdown: "body",
      savedAt: 1,
      dirtyGeneration: 1,
    });
    expect(versionSaveIdle).not.toHaveBeenCalled();
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
