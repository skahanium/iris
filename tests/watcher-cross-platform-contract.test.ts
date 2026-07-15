import { readFileSync } from "node:fs";

import { describe, expect, it } from "vitest";

describe("file watcher cross-platform contract", () => {
  it("stores the debouncer with the platform-recommended cache type", () => {
    const source = readFileSync("src-tauri/src/watcher/mod.rs", "utf8");

    expect(source).toContain("RecommendedCache");
    expect(source).toContain(
      "Debouncer<notify::RecommendedWatcher, RecommendedCache>",
    );
    expect(source).not.toContain(
      "Debouncer<notify::RecommendedWatcher, FileIdMap>",
    );
  });
});
