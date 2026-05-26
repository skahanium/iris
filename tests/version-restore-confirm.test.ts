import { describe, expect, it } from "vitest";

import { buildRestoreConfirmMessage } from "@/components/version/version-restore-confirm";
import type { VersionEntry } from "@/types/ipc";

function entry(overrides: Partial<VersionEntry>): VersionEntry {
  return {
    id: 1,
    file_id: 1,
    version_no: "20260525143052123",
    label: null,
    content_hash: "h",
    word_count: 10,
    is_finalized: false,
    kind: "manual",
    created_at: "",
    ...overrides,
  };
}

describe("buildRestoreConfirmMessage", () => {
  it("mentions finalized when restoring a finalized snapshot", () => {
    const msg = buildRestoreConfirmMessage(
      entry({ is_finalized: true, kind: "finalize" }),
      false,
    );
    expect(msg).toContain("定稿");
    expect(msg).toContain("恢复前备份");
  });

  it("warns about unsaved edits", () => {
    const msg = buildRestoreConfirmMessage(entry({ kind: "manual" }), true);
    expect(msg).toContain("未保存");
  });

  it("uses standard copy for a routine restore", () => {
    const msg = buildRestoreConfirmMessage(entry({ kind: "auto_idle" }), false);
    expect(msg).toContain("确定继续");
    expect(msg).not.toContain("定稿");
  });
});
