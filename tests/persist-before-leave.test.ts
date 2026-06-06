import { describe, expect, it, vi } from "vitest";

import {
  persistActiveTabBeforeLeave,
  persistInactiveDirtyTabBeforeLeave,
} from "@/lib/persist-before-leave";
import { createLeaveSnapshotEnqueuer } from "@/lib/version-leave-snapshot";
import {
  createVersionSnapshotScheduler,
  type LastSavedSnapshot,
} from "@/lib/version-snapshot-scheduler";

function savedSnapshot(path: string, markdown: string): LastSavedSnapshot {
  return {
    path,
    markdown,
    savedAt: 1,
    dirtyGeneration: 1,
  };
}

describe("persistActiveTabBeforeLeave", () => {
  it("does not enqueue auto_idle on app_close after layer-1 flush", async () => {
    const enqueueIdleSnapshot = vi.fn();
    const flushSaveForPath = vi.fn(async () => "saved body");
    const getLastSavedSnapshot = vi.fn(() =>
      savedSnapshot("notes/a.md", "saved body"),
    );

    const md = await persistActiveTabBeforeLeave({
      path: "notes/a.md",
      reason: "app_close",
      getMarkdown: () => "live body",
      flushSaveForPath,
      getLastSavedSnapshot,
      enqueueIdleSnapshot,
    });

    expect(md).toBe("saved body");
    expect(flushSaveForPath).toHaveBeenCalledWith(
      "notes/a.md",
      expect.any(Function),
    );
    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();
  });

  it("enqueues auto_idle on tab_leave when snapshot matches flushed markdown", async () => {
    const enqueueIdleSnapshot = vi.fn();
    const snapshot = savedSnapshot("notes/a.md", "saved body");
    const flushSaveForPath = vi.fn(async () => "saved body");
    const getLastSavedSnapshot = vi.fn(() => snapshot);

    await persistActiveTabBeforeLeave({
      path: "notes/a.md",
      reason: "tab_leave",
      getMarkdown: () => "live body",
      flushSaveForPath,
      getLastSavedSnapshot,
      enqueueIdleSnapshot,
    });

    expect(enqueueIdleSnapshot).toHaveBeenCalledWith(snapshot);
  });
});

describe("persistInactiveDirtyTabBeforeLeave", () => {
  it("writes layer-1 but does not reach version IPC on app_close", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });
    const enqueueIdleSnapshot = vi.fn((snapshot: LastSavedSnapshot) => {
      scheduler.enqueueIdle(snapshot);
    });
    const enqueueLeaveSnapshot = createLeaveSnapshotEnqueuer({
      enqueueIdleSnapshot,
      nextDirtyGeneration: () => 1,
    });
    const writeFile = vi.fn(async () => undefined);

    const md = await persistInactiveDirtyTabBeforeLeave({
      path: "notes/background.md",
      reason: "app_close",
      cachedMarkdown: "cached body",
      writeFile,
      enqueueLeaveSnapshot,
    });

    expect(md).toBe("cached body");
    expect(writeFile).toHaveBeenCalledWith(
      "notes/background.md",
      "cached body",
    );
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("may enqueue auto_idle on tab_leave for inactive dirty tabs", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });
    const enqueueIdleSnapshot = vi.fn((snapshot: LastSavedSnapshot) => {
      scheduler.enqueueIdle(snapshot);
    });
    const enqueueLeaveSnapshot = createLeaveSnapshotEnqueuer({
      enqueueIdleSnapshot,
      nextDirtyGeneration: () => 2,
    });
    const writeFile = vi.fn(async () => undefined);

    await persistInactiveDirtyTabBeforeLeave({
      path: "notes/background.md",
      reason: "tab_leave",
      cachedMarkdown: "cached body",
      writeFile,
      enqueueLeaveSnapshot,
    });

    expect(writeFile).toHaveBeenCalled();
    expect(versionSaveIdle).toHaveBeenCalledWith(
      "notes/background.md",
      "cached body",
    );
  });
});
