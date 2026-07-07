import { computeVisibleTabCount } from "@/lib/tab-overflow";
import { describe, expect, it } from "vitest";

describe("computeVisibleTabCount", () => {
  it("shows all tabs when they fit at min width without a more button", () => {
    expect(
      computeVisibleTabCount({
        railWidthPx: 1000,
        tabMinPx: 72,
        moreButtonPx: 32,
        gapPx: 4,
        tabCount: 5,
      }),
    ).toBe(5);
  });

  it("compresses and reserves space for the more button when tabs overflow", () => {
    expect(
      computeVisibleTabCount({
        railWidthPx: 360,
        tabMinPx: 72,
        moreButtonPx: 32,
        gapPx: 4,
        tabCount: 10,
      }),
    ).toBe(4);
  });

  it("lets callers reserve both the more button and a trailing new-note button", () => {
    expect(
      computeVisibleTabCount({
        railWidthPx: 360,
        tabMinPx: 72,
        moreButtonPx: 32,
        trailingButtonPx: 32,
        gapPx: 4,
        tabCount: 10,
      }),
    ).toBe(3);
  });

  it("keeps at least one tab in the more menu when overflowing", () => {
    expect(
      computeVisibleTabCount({
        railWidthPx: 200,
        tabMinPx: 72,
        moreButtonPx: 32,
        gapPx: 4,
        tabCount: 3,
      }),
    ).toBe(2);
  });

  it("returns 0 when the rail cannot fit even one min-width tab plus the more button", () => {
    expect(
      computeVisibleTabCount({
        railWidthPx: 30,
        tabMinPx: 72,
        moreButtonPx: 32,
        gapPx: 4,
        tabCount: 5,
      }),
    ).toBe(0);
  });

  it("returns 0 for an empty tab set", () => {
    expect(
      computeVisibleTabCount({
        railWidthPx: 1000,
        tabMinPx: 72,
        moreButtonPx: 32,
        gapPx: 4,
        tabCount: 0,
      }),
    ).toBe(0);
  });
});
