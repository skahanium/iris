import { describe, expect, it, vi } from "vitest";

import {
  createVersionSnapshotScheduler,
  type LastSavedSnapshot,
  type VersionSnapshotEnqueueResult,
} from "@/lib/version-snapshot-scheduler";
import type { VersionEntry, VersionSaveOutcome } from "@/types/ipc";

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

function saveOutcome(): VersionSaveOutcome {
  return { created: true, versionId: 1, skipReason: null };
}

function finalizedEntry(): VersionEntry {
  return {
    id: 1,
    file_id: 1,
    version_no: "20260716000000000",
    label: null,
    content_hash: "hash",
    word_count: 1,
    is_finalized: true,
    kind: "finalize",
    created_at: "2026-07-16T00:00:00Z",
  };
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

    await Promise.resolve();
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

  it("does not mark a failed generation complete, so it can be retried", async () => {
    const onError = vi.fn();
    const versionSaveIdle = vi
      .fn<() => Promise<void>>()
      .mockRejectedValueOnce(new Error("disk unavailable"))
      .mockResolvedValueOnce(undefined);
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle,
      onError,
    });

    await expectAccepted(scheduler.enqueueIdle(snapshot("a.md", 1))).done;
    const retry = expectAccepted(scheduler.enqueueIdle(snapshot("a.md", 1)));
    await retry.done;

    expect(versionSaveIdle).toHaveBeenCalledTimes(2);
    expect(onError).toHaveBeenCalledTimes(1);
    expect(scheduler.getStats()).toMatchObject({
      completed: 1,
      failed: 1,
    });
  });

  it("drops an idle snapshot requested after a manual version is queued", () => {
    const versionSaveIdle = vi.fn(async () => undefined);
    const versionSaveManual = vi.fn(
      () => new Promise<VersionSaveOutcome>(() => undefined),
    );
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle,
      versionSaveManual,
    });

    void scheduler.saveManual("a.md", "manual body");
    const result = scheduler.enqueueIdle(snapshot("a.md", 1));

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

  it("serializes a manual version behind an in-flight idle version for the same document", async () => {
    let resolveIdle!: (value: VersionSaveOutcome) => void;
    const versionSaveIdle = vi.fn(
      () =>
        new Promise<VersionSaveOutcome>((resolve) => {
          resolveIdle = resolve;
        }),
    );
    const versionSaveManual = vi.fn(async () => saveOutcome());
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle,
      versionSaveManual,
    });

    const idle = expectAccepted(scheduler.enqueueIdle(snapshot("a.md", 1)));
    const manual = scheduler.saveManual("a.md", "manual body");

    await Promise.resolve();
    expect(versionSaveIdle).toHaveBeenCalledWith("a.md", "body");
    expect(versionSaveManual).not.toHaveBeenCalled();

    resolveIdle(saveOutcome());
    await idle.done;
    await expect(manual).resolves.toEqual(saveOutcome());
    expect(versionSaveManual).toHaveBeenCalledWith("a.md", "manual body");
  });

  it("releases a queued manual version after the preceding idle write fails", async () => {
    const onError = vi.fn();
    const versionSaveIdle = vi.fn(async () => {
      throw new Error("disk unavailable");
    });
    const versionSaveManual = vi.fn(async () => saveOutcome());
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle,
      versionSaveManual,
      onError,
    });

    const idle = expectAccepted(scheduler.enqueueIdle(snapshot("a.md", 1)));
    const manual = scheduler.saveManual("a.md", "manual body");

    await idle.done;
    await expect(manual).resolves.toEqual(saveOutcome());
    expect(onError).toHaveBeenCalledTimes(1);
    expect(versionSaveManual).toHaveBeenCalledWith("a.md", "manual body");
  });

  it("serializes finalize behind manual save for the same document", async () => {
    let resolveManual!: (value: VersionSaveOutcome) => void;
    const versionSaveManual = vi.fn(
      () =>
        new Promise<VersionSaveOutcome>((resolve) => {
          resolveManual = resolve;
        }),
    );
    const versionFinalizeCurrent = vi.fn(async () => finalizedEntry());
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle: vi.fn(async () => saveOutcome()),
      versionSaveManual,
      versionFinalizeCurrent,
    });

    const manual = scheduler.saveManual("a.md", "manual body");
    const finalize = scheduler.finalize("a.md", "final body", "release");

    await Promise.resolve();
    expect(versionSaveManual).toHaveBeenCalledWith("a.md", "manual body");
    expect(versionFinalizeCurrent).not.toHaveBeenCalled();

    resolveManual(saveOutcome());
    await expect(manual).resolves.toEqual(saveOutcome());
    await expect(finalize).resolves.toEqual(finalizedEntry());
    expect(versionFinalizeCurrent).toHaveBeenCalledWith(
      "a.md",
      "final body",
      "release",
    );
  });

  it("allows different documents to write versions concurrently", async () => {
    let resolveFirst!: (value: VersionSaveOutcome) => void;
    const versionSaveManual = vi.fn((path: string) =>
      path === "a.md"
        ? new Promise<VersionSaveOutcome>((resolve) => {
            resolveFirst = resolve;
          })
        : Promise.resolve(saveOutcome()),
    );
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle: vi.fn(async () => saveOutcome()),
      versionSaveManual,
    });

    const first = scheduler.saveManual("a.md", "a");
    const second = scheduler.saveManual("b.md", "b");

    await expect(second).resolves.toEqual(saveOutcome());
    expect(versionSaveManual).toHaveBeenCalledWith("a.md", "a");
    expect(versionSaveManual).toHaveBeenCalledWith("b.md", "b");

    resolveFirst(saveOutcome());
    await expect(first).resolves.toEqual(saveOutcome());
  });

  it("continues a document queue after a failed version write", async () => {
    const versionSaveManual = vi
      .fn<() => Promise<VersionSaveOutcome>>()
      .mockRejectedValueOnce(new Error("disk unavailable"))
      .mockResolvedValueOnce(saveOutcome());
    const scheduler = createVersionSnapshotScheduler({
      versionSaveIdle: vi.fn(async () => saveOutcome()),
      versionSaveManual,
    });

    await expect(scheduler.saveManual("a.md", "first")).rejects.toThrow(
      "disk unavailable",
    );
    await expect(scheduler.saveManual("a.md", "second")).resolves.toEqual(
      saveOutcome(),
    );
    expect(versionSaveManual).toHaveBeenNthCalledWith(1, "a.md", "first");
    expect(versionSaveManual).toHaveBeenNthCalledWith(2, "a.md", "second");
  });
});
