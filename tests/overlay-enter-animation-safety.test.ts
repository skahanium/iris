import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const overlaySizes = readFileSync("src/lib/overlay-sizes.ts", "utf8");
const tailwindConfig = readFileSync("tailwind.config.js", "utf8");

describe("overlay enter animation safety", () => {
  it("centers overlays with translate utilities", () => {
    expect(overlaySizes).toContain("left-1/2");
    expect(overlaySizes).toContain("top-1/2");
    expect(overlaySizes).toContain("-translate-x-1/2");
    expect(overlaySizes).toContain("-translate-y-1/2");
    expect(overlaySizes).toContain("animate-iris-enter");
  });

  it("keeps iris-enter/exit opacity-only so transform centering is not overridden", () => {
    expect(tailwindConfig).toMatch(
      /"iris-enter":\s*"iris-fade-in var\(--motion-base\) var\(--motion-ease-out\)"/,
    );
    expect(tailwindConfig).toMatch(
      /"iris-exit":\s*"iris-fade-out var\(--motion-exit\) var\(--motion-ease\)"/,
    );
    expect(tailwindConfig).not.toMatch(/"iris-enter":[\s\S]*iris-zoom-in/);
    expect(tailwindConfig).not.toContain(
      'from: { opacity: "0", transform: "scale(0.95)" }',
    );
  });
});
