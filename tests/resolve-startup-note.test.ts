import { describe, expect, it } from "vitest";

import { resolveStartupNote } from "@/lib/resolve-startup-note";

describe("resolveStartupNote", () => {
  it("returns null when the last session had no open tabs", () => {
    expect(
      resolveStartupNote({
        activePath: null,
        openNotePaths: [],
      }),
    ).toBeNull();
  });

  it("does not auto-open library recents when the session ended with zero tabs", () => {
    expect(
      resolveStartupNote({
        activePath: null,
        openNotePaths: [],
      }),
    ).toBeNull();
  });

  it("prefers snapshot activePath when it is still in open tabs", () => {
    expect(
      resolveStartupNote({
        activePath: "notes/a.md",
        openNotePaths: ["notes/a.md", "notes/b.md"],
      }),
    ).toEqual({ path: "notes/a.md" });
  });

  it("falls back to the first open tab when activePath is not in open tabs", () => {
    expect(
      resolveStartupNote({
        activePath: "notes/stale.md",
        openNotePaths: ["notes/other.md"],
      }),
    ).toEqual({ path: "notes/other.md" });
  });

  it("returns null when open tabs list is empty even if activePath is set", () => {
    expect(
      resolveStartupNote({
        activePath: "notes/deleted.md",
        openNotePaths: [],
      }),
    ).toBeNull();
  });

  it("uses the first open tab when snapshot has no active path", () => {
    expect(
      resolveStartupNote({
        activePath: null,
        openNotePaths: ["notes/only.md"],
      }),
    ).toEqual({ path: "notes/only.md" });
  });

  it("ignores blank activePath and uses the first open tab", () => {
    expect(
      resolveStartupNote({
        activePath: "",
        openNotePaths: ["notes/a.md"],
      }),
    ).toEqual({ path: "notes/a.md" });
  });
});
