import { readFileSync } from "node:fs";
import { resolve } from "node:path";

import { describe, expect, it } from "vitest";

const buttonSrc = readFileSync(
  resolve(process.cwd(), "src/components/ui/button.tsx"),
  "utf8",
);

describe("Button brand variant", () => {
  it("exposes brand variant bound to --brand tokens", () => {
    expect(buttonSrc).toMatch(/brand:\s*"/);
    expect(buttonSrc).toContain("--brand");
    expect(buttonSrc).toContain("--brand-foreground");
  });
});
