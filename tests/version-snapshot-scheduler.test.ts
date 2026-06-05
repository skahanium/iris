import { describe, expect, it, vi } from "vitest";

import {
  createVersionSnapshotScheduler,
  type LastSavedSnapshot,
  type VersionSnapshotEnqueueResult,
} from "@/lib/version-snapshot-scheduler";

function snapshot(
  path: string,
  dirtyGeneration: number,
  markdown = "body",
): LastSavedSnapshot {
  return {
    path,
    markdown,
    savedAt: 1,
    dirtyGeneration,
  };
}

function expectAccepted(
  result: VersionSnapshotEnqueueResult,
): Extract<VersionSnapshotEnqueueResult, { accepted: true }> {
  expect(result.accepted).toBe(true);
  if (!result.accepted) {
    throw new Error(`expected accepted result, got ${result.reason}`);
  }
  return result;
}

describe("VersionSnapshotScheduler", () => {
  it("enqueues one idle snapshot for a path generation", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });

    const first = scheduler.enqueueIdle(snapshot("a.md", 1));
    const duplicate = scheduler.enqueueIdle(snapshot("a.md", 1));

    const accepted = expectAccepted(first);
    expect(duplicate).toEqual({ accepted: false, reason: "same_generation" });

    await accepted.done;
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
    expect(versionSaveIdle).toHaveBeenCalledWith("a.md", "body");
  });

  it("drops low priority idle snapshots while the same path is in flight", async () => {
    let resolveSave!: () => void;
    const versionSaveIdle = vi.fn(
      async () =>
        new Promise<void>((resolve) => {
          resolveSave = resolve;
        }),
    );
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });

    const first = scheduler.enqueueIdle(snapshot("a.md", 1));
    const second = scheduler.enqueueIdle(snapshot("a.md", 2));

    const accepted = expectAccepted(first);
    expect(second).toEqual({ accepted: false, reason: "in_flight" });

    resolveSave();
    await accepted.done;
    expect(versionSaveIdle).toHaveBeenCalledTimes(1);
  });

  it("allows a new idle snapshot for a later generation after the previous save completes", async () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });

    const first = expectAccepted(
      scheduler.enqueueIdle(snapshot("a.md", 1, "first")),
    );
    await first.done;
    const second = scheduler.enqueueIdle(snapshot("a.md", 2, "second"));
    const acceptedSecond = expectAccepted(second);
    await acceptedSecond.done;

    expect(versionSaveIdle).toHaveBeenCalledTimes(2);
    expect(versionSaveIdle).toHaveBeenNthCalledWith(1, "a.md", "first");
    expect(versionSaveIdle).toHaveBeenNthCalledWith(2, "a.md", "second");
  });

  it("drops idle snapshots while a high priority snapshot is active", () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });

    scheduler.markHighPriorityStart("a.md");
    const result = scheduler.enqueueIdle(snapshot("a.md", 1));
    scheduler.markHighPriorityEnd("a.md");

    expect(result).toEqual({ accepted: false, reason: "high_priority_active" });
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("skips empty saved markdown before calling IPC", () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });

    const result = scheduler.enqueueIdle(snapshot("a.md", 1, ""));

    expect(result).toEqual({ accepted: false, reason: "empty" });
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });

  it("drops idle snapshots while the app is closing", () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const scheduler = createVersionSnapshotScheduler({ versionSaveIdle });

    scheduler.setAppClosing(true);
    const result = scheduler.enqueueIdle(snapshot("a.md", 1));
    scheduler.setAppClosing(false);

    expect(result).toEqual({ accepted: false, reason: "app_closing" });
    expect(versionSaveIdle).not.toHaveBeenCalled();
  });
});
