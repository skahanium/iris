import { describe, expect, it } from "vitest";

import { mergeTabsAfterPathRename } from "@/lib/note-tab-rename";

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
});
