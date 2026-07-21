import { describe, expect, it, vi } from "vitest";

import {
  DocumentPersistenceCoordinator,
  DocumentPersistenceSnapshotRejectedError,
  type DocumentPersistenceWriteResult,
} from "@/lib/document-persistence-coordinator";

function deferred<T>() {
  let resolve!: (value: T) => void;
  const promise = new Promise<T>((nextResolve) => {
    resolve = nextResolve;
  });
  return { promise, resolve };
}

const written: DocumentPersistenceWriteResult = { indexDegraded: false };

describe("DocumentPersistenceCoordinator", () => {
  it("keeps a dirty user-edit snapshot when a late load for the same path arrives", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("note.md", "authoritative disk body", 1);
    coordinator.capture("note.md", "user edit", "user_edit");
    const lateLoad = coordinator.load("note.md", "", 2);

    expect(lateLoad).toMatchObject({
      markdown: "user edit",
      source: "user_edit",
      status: "dirty",
    });

    await coordinator.barrier("note.md");
    expect(write).toHaveBeenCalledWith("note.md", "user edit");
  });

  it("adopts a newer disk load generation after the tracked document is clean", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("note.md", "first disk body", 1);
    coordinator.capture("note.md", "saved user edit", "user_edit");
    await coordinator.barrier("note.md");

    const reloaded = coordinator.load("note.md", "new disk body", 2);

    expect(reloaded).toMatchObject({
      baselineMarkdown: "new disk body",
      baselineSource: "load",
      loadGeneration: 2,
      markdown: "new disk body",
      source: "load",
      status: "clean",
    });
    await coordinator.barrier("note.md");
    expect(write).toHaveBeenCalledTimes(1);
  });

  it("rejects an empty recovery or leave snapshot before it can replace a non-empty document", () => {
    const coordinator = new DocumentPersistenceCoordinator({
      write: async () => written,
    });

    coordinator.load("note.md", "authoritative disk body", 1);

    for (const source of ["recovery", "leave"] as const) {
      expect(() => coordinator.capture("note.md", "", source)).toThrow(
        DocumentPersistenceSnapshotRejectedError,
      );
    }

    expect(coordinator.get("note.md")).toMatchObject({
      baselineMarkdown: "authoritative disk body",
      markdown: "authoritative disk body",
      source: "load",
    });
  });

  it("allows a deliberate user clear and preserves its provenance through the durable receipt", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("note.md", "authoritative disk body", 1);
    coordinator.capture("note.md", "", "user_edit");
    await coordinator.barrier("note.md");

    expect(write).toHaveBeenCalledWith("note.md", "");
    expect(coordinator.get("note.md")).toMatchObject({
      baselineMarkdown: "",
      baselineSource: "user_edit",
      markdown: "",
      source: "user_edit",
      status: "saved",
    });
  });

  it("keeps a newer revision dirty when an older write receipt arrives late", async () => {
    const firstWrite = deferred<DocumentPersistenceWriteResult>();
    const write = vi
      .fn<
        (
          path: string,
          markdown: string,
        ) => Promise<DocumentPersistenceWriteResult>
      >()
      .mockReturnValueOnce(firstWrite.promise)
      .mockResolvedValue(written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("note.md", "opened", 1);
    coordinator.capture("note.md", "first edit", "user_edit");
    const firstCommit = coordinator.commit("note.md");
    coordinator.capture("note.md", "newer edit", "user_edit");

    firstWrite.resolve(written);
    await firstCommit;

    expect(coordinator.get("note.md")).toMatchObject({
      markdown: "newer edit",
      status: "dirty",
    });

    await coordinator.barrier("note.md");
    expect(write).toHaveBeenLastCalledWith("note.md", "newer edit");
    expect(coordinator.get("note.md")).toMatchObject({
      markdown: "newer edit",
      baselineMarkdown: "newer edit",
      status: "saved",
    });
  });

  it("writes a return to the confirmed baseline after an older write finishes", async () => {
    const firstWrite = deferred<DocumentPersistenceWriteResult>();
    const write = vi
      .fn<
        (
          path: string,
          markdown: string,
        ) => Promise<DocumentPersistenceWriteResult>
      >()
      .mockReturnValueOnce(firstWrite.promise)
      .mockResolvedValue(written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("note.md", "opened", 1);
    coordinator.capture("note.md", "temporary edit", "user_edit");
    const firstCommit = coordinator.commit("note.md");
    coordinator.capture("note.md", "opened", "user_edit");
    firstWrite.resolve(written);
    await firstCommit;
    await coordinator.barrier("note.md");

    expect(write.mock.calls).toEqual([
      ["note.md", "temporary edit"],
      ["note.md", "opened"],
    ]);
    expect(coordinator.get("note.md")).toMatchObject({
      markdown: "opened",
      baselineMarkdown: "opened",
      status: "saved",
    });
  });

  it("coalesces concurrent barriers for one document into one write", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("note.md", "opened", 1);
    coordinator.capture("note.md", "edited", "user_edit");
    await Promise.all([
      coordinator.barrier("note.md"),
      coordinator.barrier("note.md"),
    ]);

    expect(write).toHaveBeenCalledTimes(1);
    expect(write).toHaveBeenCalledWith("note.md", "edited");
  });

  it("persists independently captured dirty snapshots for multiple tabs", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.capture("first.md", "first tab", "user_edit");
    coordinator.capture("second.md", "second tab", "user_edit");
    await Promise.all([
      coordinator.barrier("first.md"),
      coordinator.barrier("second.md"),
    ]);

    expect(write.mock.calls).toEqual([
      ["first.md", "first tab"],
      ["second.md", "second tab"],
    ]);
  });

  it("holds a global barrier until a document captured during an earlier write is acknowledged", async () => {
    const firstWrite = deferred<DocumentPersistenceWriteResult>();
    const write = vi
      .fn<
        (
          path: string,
          markdown: string,
        ) => Promise<DocumentPersistenceWriteResult>
      >()
      .mockReturnValueOnce(firstWrite.promise)
      .mockResolvedValue(written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("first.md", "opened", 1);
    coordinator.capture("first.md", "first edit", "user_edit");
    const barrier = coordinator.barrierAll();

    expect(write).toHaveBeenCalledWith("first.md", "first edit");
    coordinator.capture(
      "second.md",
      "edit captured while closing",
      "user_edit",
    );
    firstWrite.resolve(written);

    await barrier;

    expect(write.mock.calls).toEqual([
      ["first.md", "first edit"],
      ["second.md", "edit captured while closing"],
    ]);
    expect(coordinator.hasDirtyDocuments()).toBe(false);
  });

  it("reschedules rename-time edits onto the backend-allocated path", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("old.md", "opened", 1);
    coordinator.capture("old.md", "before rename", "user_edit");
    await coordinator.rename("old.md", "suggested.md", async () => {
      coordinator.capture("old.md", "edited during rename", "user_edit");
      return { path: "allocated.md", indexDegraded: false };
    });
    await coordinator.barrier("allocated.md");

    expect(write.mock.calls).toEqual([
      ["old.md", "before rename"],
      ["allocated.md", "edited during rename"],
    ]);
    expect(coordinator.get("allocated.md")).toMatchObject({
      markdown: "edited during rename",
      baselineMarkdown: "edited during rename",
      status: "saved",
    });
    expect(coordinator.get("old.md")).toBeNull();
    expect(coordinator.get("suggested.md")).toBeNull();
  });

  it("allows a backend-allocated filename when the migration initially keeps the old path", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("old.md", "opened", 1);
    coordinator.capture("old.md", "before rename", "user_edit");
    await coordinator.rename("old.md", "old.md", async () => {
      coordinator.capture("old.md", "edited during rename", "user_edit");
      return { path: "allocated.md", indexDegraded: false };
    });
    await coordinator.barrier("allocated.md");

    expect(write.mock.calls).toEqual([
      ["old.md", "before rename"],
      ["allocated.md", "edited during rename"],
    ]);
    expect(coordinator.get("old.md")).toBeNull();
    expect(coordinator.get("allocated.md")?.markdown).toBe(
      "edited during rename",
    );
  });

  it("queues timer-triggered edits on the new path while a move is still pending", async () => {
    vi.useFakeTimers();
    try {
      const move = deferred<{ path: string; indexDegraded: boolean }>();
      const moveStarted = deferred<void>();
      const write = vi.fn(async () => written);
      const coordinator = new DocumentPersistenceCoordinator({
        delayMs: 50,
        write,
      });

      coordinator.load("old.md", "opened", 1);
      coordinator.capture("old.md", "before move", "user_edit");
      const rename = coordinator.rename("old.md", "new.md", () => {
        moveStarted.resolve();
        return move.promise;
      });

      await moveStarted.promise;

      coordinator.capture("old.md", "edited while moving", "user_edit");
      await vi.advanceTimersByTimeAsync(50);

      expect(write.mock.calls).toEqual([["old.md", "before move"]]);

      move.resolve({ path: "allocated.md", indexDegraded: false });
      await rename;
      await coordinator.barrier("allocated.md");

      expect(write.mock.calls).toEqual([
        ["old.md", "before move"],
        ["allocated.md", "edited while moving"],
      ]);
      expect(coordinator.get("old.md")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("moves a captured snapshot to a rebound path before its next write", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("old.md", "opened", 1);
    coordinator.capture("old.md", "unsaved", "user_edit");
    coordinator.rebind("old.md", "new.md");
    await coordinator.barrier("new.md");

    expect(write).toHaveBeenCalledWith("new.md", "unsaved");
  });

  it("projects a successful rename with a degraded derived index", async () => {
    const coordinator = new DocumentPersistenceCoordinator({
      write: async () => written,
    });

    coordinator.load("old.md", "opened", 1);
    await coordinator.rename("old.md", "new.md", async () => ({
      path: "new.md",
      indexDegraded: true,
    }));

    expect(coordinator.get("new.md")).toMatchObject({
      baselineMarkdown: "opened",
      indexDegraded: true,
      status: "saved_index_degraded",
    });
  });

  it("rejects a barrier when a dirty remount has no captured snapshot", async () => {
    const coordinator = new DocumentPersistenceCoordinator({
      write: async () => written,
    });

    await expect(coordinator.barrier("missing.md")).rejects.toThrow(
      "no recoverable snapshot",
    );
  });

  it("cancels a scheduled debounce commit without discarding the dirty snapshot", async () => {
    vi.useFakeTimers();
    try {
      const write = vi.fn(async () => written);
      const coordinator = new DocumentPersistenceCoordinator({
        delayMs: 1200,
        write,
      });

      coordinator.load("note.md", "opened", 1);
      coordinator.capture("note.md", "edited", "user_edit");
      expect(coordinator.get("note.md")?.status).toBe("dirty");

      coordinator.cancelScheduled("note.md");
      await vi.advanceTimersByTimeAsync(2000);

      expect(write).not.toHaveBeenCalled();
      expect(coordinator.get("note.md")).toMatchObject({
        markdown: "edited",
        status: "dirty",
      });

      await coordinator.barrier("note.md");
      expect(write).toHaveBeenCalledWith("note.md", "edited");
    } finally {
      vi.useRealTimers();
    }
  });
});
