import { describe, expect, it, vi } from "vitest";

import { createLeaveSnapshotEnqueuer } from "@/lib/version-leave-snapshot";

describe("createLeaveSnapshotEnqueuer", () => {
  it("skips version enqueue on app_close", () => {
    const enqueueIdleSnapshot = vi.fn();
    const enqueueLeave = createLeaveSnapshotEnqueuer({
      enqueueIdleSnapshot,
      nextDirtyGeneration: () => 1,
      now: () => 42,
    });

    enqueueLeave("notes/a.md", "body", "app_close");

    expect(enqueueIdleSnapshot).not.toHaveBeenCalled();
  });

  it("enqueues on tab_leave within size limit", () => {
    const enqueueIdleSnapshot = vi.fn();
    const enqueueLeave = createLeaveSnapshotEnqueuer({
      enqueueIdleSnapshot,
      nextDirtyGeneration: () => 7,
      now: () => 100,
    });

    enqueueLeave("notes/a.md", "body", "tab_leave");

    expect(enqueueIdleSnapshot).toHaveBeenCalledWith({
      path: "notes/a.md",
      markdown: "body",
      savedAt: 100,
      dirtyGeneration: 7,
    });
  });
});
