import { describe, expect, it } from "vitest";

import {
  charsFittingInWidth,
  estimateTextWidth,
  nextRevealLength,
  readTailLineBudget,
  STREAMING_LINE_FILL_FRACTION,
} from "@/lib/streaming-line-fit";

function asciiMeasure(text: string): number {
  return text.length * 8;
}

describe("estimateTextWidth", () => {
  it("counts CJK as the font size and ASCII as a narrower advance", () => {
    expect(estimateTextWidth("a", 14)).toBeCloseTo(14 * 0.55, 5);
    expect(estimateTextWidth("中", 14)).toBe(14);
    expect(estimateTextWidth("中a", 14)).toBeCloseTo(14 + 14 * 0.55, 5);
  });
});

describe("charsFittingInWidth", () => {
  it("returns 0 when no pixels remain", () => {
    expect(charsFittingInWidth("abc", 0, asciiMeasure)).toBe(0);
  });

  it("returns the full string when it fits", () => {
    expect(charsFittingInWidth("abc", 24, asciiMeasure)).toBe(3);
  });

  it("does not split a surrogate pair", () => {
    const emoji = "😀";
    const measure = (text: string) =>
      [...text].reduce(
        (width, point) => width + (point === emoji ? 20 : 10),
        0,
      );
    expect(charsFittingInWidth(emoji, 15, measure)).toBe(0);
    expect(charsFittingInWidth(`a${emoji}`, 25, measure)).toBe(1);
    expect(charsFittingInWidth(`a${emoji}`, 40, measure)).toBe(3);
  });
});

describe("nextRevealLength", () => {
  it("advances only one consecutive newline per update", () => {
    expect(
      nextRevealLength({
        current: "a",
        target: "a\n\n\n\nb",
        remainingPx: 100,
        lineWidthPx: 240,
        measure: asciiMeasure,
      }),
    ).toBe(2);
  });

  it("never divides a family emoji grapheme", () => {
    const family = "👨‍👩‍👧‍👦";
    const target = family.repeat(20);
    const next = nextRevealLength({
      current: "",
      target,
      remainingPx: 8,
      lineWidthPx: 240,
      measure: asciiMeasure,
    });
    expect(next).toBe(family.length);
  });

  it("advances a grapheme wider than the viewport without stalling forever", () => {
    const family = "👨‍👩‍👧‍👦";
    expect(
      nextRevealLength({
        current: "",
        target: family,
        remainingPx: 20,
        lineWidthPx: 20,
        maxAdvancePx: 20,
        measure: asciiMeasure,
      }),
    ).toBe(family.length);
  });

  it("does not cut the bounded measurement prefix through a grapheme", () => {
    const target = "a".repeat(510) + "👨‍👩‍👧‍👦";
    const next = nextRevealLength({
      current: "",
      target,
      remainingPx: 10000,
      lineWidthPx: 10000,
      maxAdvancePx: 10000,
      measure: () => 1,
    });
    expect([510, target.length]).toContain(next);
  });

  it("releases a short increment that still fits on the current line in one step", () => {
    expect(
      nextRevealLength({
        current: "",
        target: "hello",
        remainingPx: 240,
        lineWidthPx: 240,
        measure: asciiMeasure,
      }),
    ).toBe(5);
  });

  it("never releases more than the remaining pixels on the current line", () => {
    const next = nextRevealLength({
      current: "hello ",
      target: "hello world and more text",
      remainingPx: 24,
      lineWidthPx: 240,
      measure: asciiMeasure,
    });
    expect(next).toBe("hello ".length + 3);
  });

  it("caps a new empty line to a fraction of the line width", () => {
    const target = "x".repeat(80);
    const next = nextRevealLength({
      current: "",
      target,
      remainingPx: 240,
      lineWidthPx: 240,
      measure: asciiMeasure,
    });
    const expectedChars = Math.floor((240 * STREAMING_LINE_FILL_FRACTION) / 8);
    expect(next).toBe(expectedChars);
    expect(next).toBeGreaterThan(0);
    expect(next).toBeLessThan(target.length);
  });

  it("starts the next line with a fraction after the current line is full", () => {
    const current = "filled";
    const target = `${current}${"y".repeat(40)}`;
    const next = nextRevealLength({
      current,
      target,
      remainingPx: 0,
      lineWidthPx: 240,
      measure: asciiMeasure,
    });
    expect(next).toBe(current.length + Math.floor((240 * 0.4) / 8));
  });

  it("does not continue past an explicit newline in the same frame", () => {
    expect(
      nextRevealLength({
        current: "",
        target: "hello\nworld and more",
        remainingPx: 240,
        lineWidthPx: 240,
        measure: asciiMeasure,
      }),
    ).toBe("hello\n".length);
  });

  it("consumes leading newlines before filling the next visual line", () => {
    expect(
      nextRevealLength({
        current: "hello",
        target: "hello\n\nworld",
        remainingPx: 80,
        lineWidthPx: 240,
        measure: asciiMeasure,
      }),
    ).toBe("hello\n".length);
  });
});

describe("readTailLineBudget", () => {
  it("treats an empty tail as a full remaining line", () => {
    const tail = document.createElement("div");
    Object.defineProperty(tail, "clientWidth", { value: 200 });
    tail.getBoundingClientRect = () =>
      ({
        left: 10,
        right: 210,
        top: 0,
        bottom: 20,
        width: 200,
        height: 20,
        x: 10,
        y: 0,
        toJSON() {
          return {};
        },
      }) as DOMRect;
    document.body.append(tail);
    const budget = readTailLineBudget(tail);
    tail.remove();
    expect(budget?.lineWidthPx).toBe(200);
    expect(budget?.remainingPx).toBe(200);
  });

  it("subtracts the last wrapped line width from the remaining budget", () => {
    const tail = document.createElement("div");
    Object.defineProperty(tail, "clientWidth", { value: 200 });
    tail.textContent = "hello world";
    document.body.append(tail);
    const rangeProto = Range.prototype.getClientRects;
    Range.prototype.getClientRects = function getClientRects() {
      const rect = {
        width: 80,
        height: 16,
        top: 0,
        left: 0,
        bottom: 16,
        right: 80,
        x: 0,
        y: 0,
        toJSON() {
          return {};
        },
      } as DOMRect;
      return {
        length: 1,
        item: (index: number) => (index === 0 ? rect : null),
        0: rect,
      } as unknown as DOMRectList;
    };
    const budget = readTailLineBudget(tail);
    Range.prototype.getClientRects = rangeProto;
    tail.remove();
    expect(budget?.remainingPx).toBe(120);
  });
});
