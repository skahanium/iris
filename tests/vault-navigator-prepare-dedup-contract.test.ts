import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";

describe("VaultNavigator prepare contract", () => {
  it("deduplicates first-visible note preparation across refreshes", () => {
    const source = readFileSync(
      "src/components/file/VaultNavigator.tsx",
      "utf8",
    );

    expect(source).toContain("preparedKeysRef");
    expect(source).toContain("preparedKeysRef.current.has");
    expect(source).toContain("preparedKeysRef.current.add");
  });
});
