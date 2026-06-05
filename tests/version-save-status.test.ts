import { describe, expect, it } from "vitest";

import { formatVersionSaveStatus } from "@/lib/version-save-status";

describe("formatVersionSaveStatus", () => {
  it("reports created manual snapshot", () => {
    expect(
      formatVersionSaveStatus({
        path: "a.md",
        kind: "manual",
        created: true,
        versionId: 1,
        error: null,
      }),
    ).toBe("已创建版本快照");
  });

  it("reports created idle snapshot", () => {
    expect(
      formatVersionSaveStatus({
        path: "a.md",
        kind: "auto_idle",
        created: true,
        versionId: 2,
        error: null,
      }),
    ).toBe("已创建空闲版本备份");
  });

  it("reports skipped duplicate content", () => {
    expect(
      formatVersionSaveStatus({
        path: "a.md",
        kind: "manual",
        created: false,
        versionId: null,
        skipReason: "duplicate_hash",
        error: null,
      }),
    ).toBe("内容未变化，已跳过版本快照");
  });

  it("reports skipped idle cooldown", () => {
    expect(
      formatVersionSaveStatus({
        path: "a.md",
        kind: "auto_idle",
        created: false,
        versionId: null,
        skipReason: "auto_idle_interval_cooldown",
        error: null,
      }),
    ).toBe("自动版本冷却中，已跳过版本快照");
  });

  it("reports error", () => {
    expect(
      formatVersionSaveStatus({
        path: "a.md",
        kind: "manual",
        created: false,
        versionId: null,
        error: "disk full",
      }),
    ).toBe("版本快照失败：disk full");
  });
});
