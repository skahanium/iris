import { describe, expect, it } from "vitest";

import { resolveStartupNote } from "@/lib/resolve-startup-note";

describe("resolveStartupNote", () => {
  it("returns null when there is no snapshot path and no recent notes", () => {
    expect(
      resolveStartupNote({
        activePath: null,
        openNotePaths: [],
        recentPaths: [],
      }),
    ).toBeNull();
  });

  it("prefers snapshot activePath when it is still in open tabs", () => {
    expect(
      resolveStartupNote({
        activePath: "notes/a.md",
        openNotePaths: ["notes/a.md", "notes/b.md"],
        recentPaths: ["notes/c.md"],
      }),
    ).toEqual({ path: "notes/a.md" });
  });

  it("prefers snapshot activePath when it appears in recent paths", () => {
    expect(
      resolveStartupNote({
        activePath: "notes/stale.md",
        openNotePaths: ["notes/other.md"],
        recentPaths: ["notes/stale.md", "notes/newer.md"],
      }),
    ).toEqual({ path: "notes/stale.md" });
  });

  it("falls back to the first recent path when activePath is missing from vault", () => {
    expect(
      resolveStartupNote({
        activePath: "notes/deleted.md",
        openNotePaths: [],
        recentPaths: ["notes/latest.md", "notes/older.md"],
      }),
    ).toEqual({ path: "notes/latest.md" });
  });

  it("uses the first recent path when snapshot has no active path", () => {
    expect(
      resolveStartupNote({
        activePath: null,
        openNotePaths: [],
        recentPaths: ["notes/only.md"],
      }),
    ).toEqual({ path: "notes/only.md" });
  });

  it("ignores empty activePath and uses recent when snapshot active is blank", () => {
    expect(
      resolveStartupNote({
        activePath: "",
        openNotePaths: ["notes/a.md"],
        recentPaths: ["notes/recent.md"],
      }),
    ).toEqual({ path: "notes/recent.md" });
  });
});
