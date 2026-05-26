import { describe, expect, it } from "vitest";

import { readingMinutes } from "@/lib/reading-time";

describe("readingMinutes", () => {
  it("estimates Chinese text", () => {
    expect(readingMinutes("一二三四五六七八九十")).toBeGreaterThanOrEqual(1);
  });

  it("estimates English words", () => {
    const words = Array.from({ length: 300 }, (_, i) => `word${i}`).join(" ");
    expect(readingMinutes(words)).toBeGreaterThanOrEqual(1);
  });
});
