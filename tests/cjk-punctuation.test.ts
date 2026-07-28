import { describe, expect, it } from "vitest";

import {
  CJK_PUNCT_MAP,
  convertAsciiPunctChar,
  countUnmatchedSmartQuotes,
  isCjkContextChar,
} from "@/lib/cjk-punctuation";

describe("isCjkContextChar", () => {
  it("treats Han / Hiragana / Katakana / Hangul as CJK context", () => {
    expect(isCjkContextChar("说")).toBe(true);
    expect(isCjkContextChar("の")).toBe(true);
    expect(isCjkContextChar("カ")).toBe(true);
    expect(isCjkContextChar("한")).toBe(true);
  });

  it("treats fullwidth punctuation as CJK context", () => {
    expect(isCjkContextChar("。")).toBe(true);
    expect(isCjkContextChar("，")).toBe(true);
    expect(isCjkContextChar("“")).toBe(true);
    expect(isCjkContextChar("（")).toBe(true);
  });

  it("rejects ASCII letters, digits, whitespace and empty string", () => {
    expect(isCjkContextChar("a")).toBe(false);
    expect(isCjkContextChar("1")).toBe(false);
    expect(isCjkContextChar(".")).toBe(false);
    expect(isCjkContextChar(" ")).toBe(false);
    expect(isCjkContextChar("")).toBe(false);
  });
});

describe("convertAsciiPunctChar: non-quote punctuation", () => {
  it("converts . , : ; ! ? ( ) after a CJK character", () => {
    for (const [ascii, full] of Object.entries(CJK_PUNCT_MAP)) {
      const result = convertAsciiPunctChar(ascii, "说", 0, 0);
      expect(result.changed).toBe(true);
      expect(result.converted).toBe(full);
    }
  });

  it("leaves ASCII punctuation untouched after ASCII letters/digits", () => {
    expect(convertAsciiPunctChar(".", "1", 0, 0)).toEqual({
      converted: ".",
      changed: false,
    });
    expect(convertAsciiPunctChar(".", "w", 0, 0)).toEqual({
      converted: ".",
      changed: false,
    });
    expect(convertAsciiPunctChar(",", "o", 0, 0)).toEqual({
      converted: ",",
      changed: false,
    });
  });

  it("leaves ASCII punctuation untouched at block start (empty before)", () => {
    expect(convertAsciiPunctChar(".", "", 0, 0)).toEqual({
      converted: ".",
      changed: false,
    });
    expect(convertAsciiPunctChar("(", " ", 0, 0)).toEqual({
      converted: "(",
      changed: false,
    });
  });

  it("does not convert unknown characters", () => {
    expect(convertAsciiPunctChar("a", "说", 0, 0)).toEqual({
      converted: "a",
      changed: false,
    });
    expect(convertAsciiPunctChar("#", "说", 0, 0)).toEqual({
      converted: "#",
      changed: false,
    });
  });

  it("rejects multi-char input without throwing", () => {
    expect(convertAsciiPunctChar("..", "说", 0, 0)).toEqual({
      converted: "..",
      changed: false,
    });
  });
});

describe("convertAsciiPunctChar: smart quotes", () => {
  it("opens “ when no unmatched open double quote exists", () => {
    expect(convertAsciiPunctChar('"', "说", 0, 0)).toEqual({
      converted: "“",
      changed: true,
    });
  });

  it("closes ” when an unmatched open double quote exists", () => {
    expect(convertAsciiPunctChar('"', "好", 1, 0)).toEqual({
      converted: "”",
      changed: true,
    });
  });

  it("opens ‘ when no unmatched open single quote exists", () => {
    expect(convertAsciiPunctChar("'", "说", 0, 0)).toEqual({
      converted: "‘",
      changed: true,
    });
  });

  it("closes ’ when an unmatched open single quote exists", () => {
    expect(convertAsciiPunctChar("'", "好", 0, 1)).toEqual({
      converted: "’",
      changed: true,
    });
  });

  it("does not convert quotes outside CJK context", () => {
    expect(convertAsciiPunctChar('"', " ", 0, 0)).toEqual({
      converted: '"',
      changed: false,
    });
    expect(convertAsciiPunctChar("'", "a", 0, 0)).toEqual({
      converted: "'",
      changed: false,
    });
  });
});

describe("countUnmatchedSmartQuotes", () => {
  it("returns zero for text without smart quotes", () => {
    expect(countUnmatchedSmartQuotes("他说你好")).toEqual({
      double: 0,
      single: 0,
    });
  });

  it("counts an unmatched open double quote", () => {
    expect(countUnmatchedSmartQuotes("他说“你好")).toEqual({
      double: 1,
      single: 0,
    });
  });

  it("resets to zero when paired", () => {
    expect(countUnmatchedSmartQuotes("他说“你好”")).toEqual({
      double: 0,
      single: 0,
    });
  });

  it("tracks single quotes independently", () => {
    expect(countUnmatchedSmartQuotes("他说‘你好’“再见")).toEqual({
      double: 1,
      single: 0,
    });
  });

  it("ignores ASCII quotes", () => {
    expect(countUnmatchedSmartQuotes("He said \"hi\" and 'bye'")).toEqual({
      double: 0,
      single: 0,
    });
  });
});
