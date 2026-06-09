import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("package metadata license", () => {
  it("declares AGPL-3.0-only for open-source release metadata", () => {
    const pkg = JSON.parse(readFileSync("package.json", "utf8")) as {
      license?: string;
    };

    expect(pkg.license).toBe("AGPL-3.0-only");
  });
});
