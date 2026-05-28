import { describe, expect, it } from "vitest";

import {
  getLabelMatchRange,
  splitLabelByMatch,
} from "@/lib/command-palette-match";

describe("command-palette-match", () => {
  it("returns match range for substring", () => {
    expect(getLabelMatchRange("全文搜索", "搜索")).toEqual({
      start: 2,
      end: 4,
    });
  });

  it("returns null when no match", () => {
    expect(getLabelMatchRange("设置", "图谱")).toBeNull();
  });

  it("splits label into highlighted segments", () => {
    expect(splitLabelByMatch("快速打开笔记", "打开")).toEqual([
      { text: "快速", highlighted: false },
      { text: "打开", highlighted: true },
      { text: "笔记", highlighted: false },
    ]);
  });
});
