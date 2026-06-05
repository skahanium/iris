import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

import {
  AUTO_LEAVE_SNAPSHOT_MAX_CHARS,
  ENABLE_TAB_LEAVE_AUTO_SNAPSHOT,
  shouldEnqueueAutoSnapshotOnLeave,
} from "../src/lib/version-auto-snapshot-policy";

function readPolicySource(): string {
  return readFileSync("src/lib/version-auto-snapshot-policy.ts", "utf8");
}

describe("version auto snapshot leave policy", () => {
  it("keeps tab leave snapshots enabled after P0 validation", () => {
    expect(ENABLE_TAB_LEAVE_AUTO_SNAPSHOT).toBe(true);
  });

  it("documents that auto_idle has no frontend length gate (leave-only 12k)", () => {
    const source = readPolicySource();
    expect(source).toContain("auto_idle");
    expect(source).toMatch(
      /\| `auto_idle`[\s\S]*\| \*\*No\*\* frontend gate \|/,
    );
    expect(source).toContain("AUTO_LEAVE_SNAPSHOT_MAX_CHARS");
  });

  it("allows tab leave snapshots within the large-document threshold", () => {
    expect(
      shouldEnqueueAutoSnapshotOnLeave({
        reason: "tab_leave",
        markdownLength: AUTO_LEAVE_SNAPSHOT_MAX_CHARS,
      }),
    ).toBe(true);
  });

  it("skips tab leave snapshots above the large-document threshold", () => {
    expect(
      shouldEnqueueAutoSnapshotOnLeave({
        reason: "tab_leave",
        markdownLength: AUTO_LEAVE_SNAPSHOT_MAX_CHARS + 1,
      }),
    ).toBe(false);
  });

  it("skips app close snapshots to avoid starting version IPC while closing", () => {
    expect(
      shouldEnqueueAutoSnapshotOnLeave({
        reason: "app_close",
        markdownLength: 1,
      }),
    ).toBe(false);
  });
});
