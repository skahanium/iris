import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

const COMPATIBLE_LICENSES = new Set([
  "0BSD",
  "Apache-2.0",
  "Apache-2.0 OR MIT",
  "BSD-2-Clause",
  "BSD-3-Clause",
  "CC0-1.0",
  "ISC",
  "MIT",
  "MIT OR Apache-2.0",
  "MIT-0",
  "MPL-2.0",
  "Python-2.0",
  "Unicode-3.0",
  "Zlib",
  "(MPL-2.0 OR Apache-2.0)",
]);

const TRANSITIVE_NON_CODE_LICENSES = new Map([
  [
    "BlueOak-1.0.0",
    "Permissive transitive package metadata license used by minimatch.",
  ],
  ["CC-BY-4.0", "Data-only browser compatibility metadata in caniuse-lite."],
]);

interface PackageLock {
  packages?: Record<string, { version?: string; license?: string }>;
}

describe("package metadata license", () => {
  it("declares AGPL-3.0-only for open-source release metadata", () => {
    const pkg = JSON.parse(readFileSync("package.json", "utf8")) as {
      license?: string;
    };

    expect(pkg.license).toBe("AGPL-3.0-only");
  });

  it("documents transitive npm license compatibility decisions", () => {
    const lockfile = JSON.parse(
      readFileSync("package-lock.json", "utf8"),
    ) as PackageLock;
    const unknown = Object.entries(lockfile.packages ?? {})
      .filter(([path]) => path.startsWith("node_modules/"))
      .map(([, meta]) => meta.license)
      .filter((license): license is string => Boolean(license))
      .filter(
        (license) =>
          !COMPATIBLE_LICENSES.has(license) &&
          !TRANSITIVE_NON_CODE_LICENSES.has(license),
      );

    expect([...new Set(unknown)].sort()).toEqual([]);
    expect(TRANSITIVE_NON_CODE_LICENSES.get("BlueOak-1.0.0")).toContain(
      "minimatch",
    );
    expect(TRANSITIVE_NON_CODE_LICENSES.get("CC-BY-4.0")).toContain(
      "caniuse-lite",
    );
  });
});
