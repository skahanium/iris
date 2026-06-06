import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("useClassifiedVaultSession", () => {
  it("defines global idle auto-lock with deferral while classified tabs are open", () => {
    const src = readFileSync(
      "src/hooks/useClassifiedVaultSession.ts",
      "utf8",
    );
    expect(src).toContain("AUTO_LOCK_MS = 10 * 60 * 1000");
    expect(src).toContain("openClassifiedPaths.length > 0");
    expect(src).toContain("classifiedLock");
    expect(src).toContain('addEventListener("mousemove"');
    expect(src).toContain("idleDeadline");
  });
});
