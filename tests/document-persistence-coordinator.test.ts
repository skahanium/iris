import { describe, expect, it, vi } from "vitest";

import {
  DocumentPersistenceCoordinator,
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

    coordinator.load("note.md", "opened");
    coordinator.capture("note.md", "first edit");
    const firstCommit = coordinator.commit("note.md");
    coordinator.capture("note.md", "newer edit");

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

    coordinator.load("note.md", "opened");
    coordinator.capture("note.md", "temporary edit");
    const firstCommit = coordinator.commit("note.md");
    coordinator.capture("note.md", "opened");
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

    coordinator.load("note.md", "opened");
    coordinator.capture("note.md", "edited");
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

    coordinator.capture("first.md", "first tab");
    coordinator.capture("second.md", "second tab");
    await Promise.all([
      coordinator.barrier("first.md"),
      coordinator.barrier("second.md"),
    ]);

    expect(write.mock.calls).toEqual([
      ["first.md", "first tab"],
      ["second.md", "second tab"],
    ]);
  });

  it("reschedules rename-time edits onto the backend-allocated path", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("old.md", "opened");
    coordinator.capture("old.md", "before rename");
    await coordinator.rename("old.md", "suggested.md", async () => {
      coordinator.capture("old.md", "edited during rename");
      return "allocated.md";
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

  it("moves a captured snapshot to a rebound path before its next write", async () => {
    const write = vi.fn(async () => written);
    const coordinator = new DocumentPersistenceCoordinator({ write });

    coordinator.load("old.md", "opened");
    coordinator.capture("old.md", "unsaved");
    coordinator.rebind("old.md", "new.md");
    await coordinator.barrier("new.md");

    expect(write).toHaveBeenCalledWith("new.md", "unsaved");
  });

  it("rejects a barrier when a dirty remount has no captured snapshot", async () => {
    const coordinator = new DocumentPersistenceCoordinator({
      write: async () => written,
    });

    await expect(coordinator.barrier("missing.md")).rejects.toThrow(
      "no recoverable snapshot",
    );
  });
});
