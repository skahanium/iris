import { describe, expect, it } from "vitest";

import {
  mergeTabsAfterPathRename,
  selectMarkdownCacheAfterPathRename,
} from "@/lib/note-tab-rename";

describe("mergeTabsAfterPathRename", () => {
  it("renames a single tab in place", () => {
    const tabs = [{ path: "a.md", title: "A", dirty: true }];
    const next = mergeTabsAfterPathRename(tabs, "a.md", "b.md", "B");
    expect(next).toEqual([{ path: "b.md", title: "B", dirty: true }]);
  });

  it("merges duplicate tabs when both old and new paths exist", () => {
    const tabs = [
      { path: "old.md", title: "Live", dirty: true },
      { path: "new.md", title: "Stale", dirty: false },
      { path: "other.md", title: "Other", dirty: false },
    ];
    const next = mergeTabsAfterPathRename(tabs, "old.md", "new.md", "Live");
    expect(next).toHaveLength(2);
    expect(next.find((t) => t.path === "old.md")).toBeUndefined();
    expect(next.find((t) => t.path === "new.md")).toMatchObject({
      title: "Live",
      dirty: true,
    });
    expect(next.find((t) => t.path === "other.md")).toBeDefined();
  });

  it("keeps dirty when either side was dirty", () => {
    const tabs = [
      { path: "old.md", title: "Old", dirty: false },
      { path: "new.md", title: "New", dirty: true },
    ];
    const next = mergeTabsAfterPathRename(tabs, "old.md", "new.md", "Merged");
    expect(next.find((t) => t.path === "new.md")).toMatchObject({
      dirty: true,
      title: "Merged",
    });
  });
});

describe("selectMarkdownCacheAfterPathRename", () => {
  it("prefers the rename-source override", () => {
    expect(
      selectMarkdownCacheAfterPathRename({
        destinationCached: "destination body",
        destinationDirty: true,
        sourceCached: "source body",
        sourceDirty: true,
        sourceOverride: "override body",
      }),
    ).toBe("override body");
  });

  it("keeps destination dirty content when source has no recoverable snapshot", () => {
    expect(
      selectMarkdownCacheAfterPathRename({
        destinationCached: "destination only",
        destinationDirty: true,
        sourceCached: undefined,
        sourceDirty: false,
        sourceOverride: undefined,
      }),
    ).toBe("destination only");
  });

  it("prefers dirty destination over clean source cache", () => {
    expect(
      selectMarkdownCacheAfterPathRename({
        destinationCached: "destination dirty",
        destinationDirty: true,
        sourceCached: "source clean",
        sourceDirty: false,
        sourceOverride: undefined,
      }),
    ).toBe("destination dirty");
  });

  it("prefers dirty source over dirty destination when both exist", () => {
    expect(
      selectMarkdownCacheAfterPathRename({
        destinationCached: "destination dirty",
        destinationDirty: true,
        sourceCached: "source dirty",
        sourceDirty: true,
        sourceOverride: undefined,
      }),
    ).toBe("source dirty");
  });
});
