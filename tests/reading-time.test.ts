import { describe, expect, it } from "vitest";

import {
  characterCountExcludingWhitespace,
  readingMinutes,
} from "@/lib/reading-time";

describe("characterCountExcludingWhitespace", () => {
  it("counts non-whitespace characters for CJK", () => {
    expect(characterCountExcludingWhitespace("一二三四五六七八九十")).toBe(10);
    expect(characterCountExcludingWhitespace("a\n\nb\tc")).toBe(3);
  });
});

describe("readingMinutes", () => {
  it("estimates Chinese text", () => {
    expect(readingMinutes("一二三四五六七八九十")).toBeGreaterThanOrEqual(1);
  });

  it("estimates English words", () => {
    const words = Array.from({ length: 300 }, (_, i) => `word${i}`).join(" ");
    expect(readingMinutes(words)).toBeGreaterThanOrEqual(1);
  });
});
