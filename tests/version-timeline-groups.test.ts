import { describe, expect, it } from "vitest";

import {
  formatVersionDisplayTime,
  groupVersions,
  isVisibleInDayList,
  parseVersionDate,
} from "@/components/version/version-timeline-groups";
import type { VersionEntry } from "@/types/ipc";

function entry(
  overrides: Partial<VersionEntry> & Pick<VersionEntry, "id" | "kind">,
): VersionEntry {
  return {
    file_id: 1,
    version_no: "20260526120000000",
    label: null,
    content_hash: "h",
    word_count: 10,
    is_finalized: false,
    created_at: "2026-05-26T12:00:00Z",
    ...overrides,
  };
}

const now = new Date("2026-05-26T15:00:00Z");

describe("groupVersions", () => {
  it("puts finalized entries in the top section", () => {
    const layout = groupVersions(
      [
        entry({ id: 1, kind: "manual" }),
        entry({ id: 2, kind: "finalize", is_finalized: true, label: "v1" }),
      ],
      now,
    );
    expect(layout.finalized).toHaveLength(1);
    expect(layout.finalized[0]?.id).toBe(2);
  });

  it("collapses auto_idle into a single group per day (default UI lists none)", () => {
    const autoEntries = Array.from({ length: 5 }, (_, i) =>
      entry({
        id: i + 1,
        kind: "auto_idle",
        version_no: `20260526120${String(i).padStart(4, "0")}`,
        created_at: `2026-05-26T12:0${i}:00Z`,
      }),
    );
    const layout = groupVersions(autoEntries, now);
    const today = layout.days.find((d) => d.bucket === "today");
    expect(today).toBeDefined();
    expect(today!.visible).toHaveLength(0);
    expect(today!.collapsed).toHaveLength(1);
    expect(today!.collapsed[0]?.label).toBe("自动备份（5）");
    expect(today!.collapsed[0]?.entries).toHaveLength(5);
  });

  it("keeps manual entries visible in the day list", () => {
    const layout = groupVersions(
      [entry({ id: 1, kind: "manual", version_no: "20260526143000000" })],
      now,
    );
    const today = layout.days.find((d) => d.bucket === "today");
    expect(today?.visible).toHaveLength(1);
    expect(today?.collapsed).toHaveLength(0);
  });

  it("groups pre_restore separately from auto_idle", () => {
    const layout = groupVersions(
      [
        entry({ id: 1, kind: "auto_idle" }),
        entry({ id: 2, kind: "pre_restore", version_no: "20260526110000000" }),
      ],
      now,
    );
    const today = layout.days.find((d) => d.bucket === "today");
    expect(today?.collapsed).toHaveLength(2);
    expect(today?.collapsed.map((g) => g.type)).toEqual([
      "auto_idle",
      "pre_restore",
    ]);
  });
});

describe("isVisibleInDayList", () => {
  it("excludes auto_idle and finalized", () => {
    expect(isVisibleInDayList(entry({ id: 1, kind: "manual" }))).toBe(true);
    expect(isVisibleInDayList(entry({ id: 2, kind: "auto_idle" }))).toBe(false);
    expect(
      isVisibleInDayList(
        entry({ id: 3, kind: "finalize", is_finalized: true }),
      ),
    ).toBe(false);
  });
});

describe("parseVersionDate", () => {
  it("prefers created_at ISO timestamp", () => {
    const d = parseVersionDate(
      entry({
        id: 1,
        kind: "manual",
        version_no: "20260526120000000",
        created_at: "2026-06-04T03:17:13Z",
      }),
    );
    expect(d.toISOString()).toBe("2026-06-04T03:17:13.000Z");
  });

  it("parses version_no as UTC when created_at is missing", () => {
    const d = parseVersionDate(
      entry({
        id: 1,
        kind: "manual",
        version_no: "20260526143052123",
        created_at: "",
      }),
    );
    expect(d.toISOString()).toBe("2026-05-26T14:30:52.000Z");
  });
});

describe("formatVersionDisplayTime", () => {
  it("shows local time from created_at", () => {
    const formatted = formatVersionDisplayTime(
      entry({
        id: 1,
        kind: "manual",
        created_at: "2026-06-04T03:17:13Z",
      }),
    );
    const expected = new Date("2026-06-04T03:17:13Z").toLocaleString("zh-CN", {
      year: "numeric",
      month: "2-digit",
      day: "2-digit",
      hour: "2-digit",
      minute: "2-digit",
      second: "2-digit",
      hour12: false,
    });
    expect(formatted).toBe(expected);
  });
});
