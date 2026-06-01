import { describe, expect, it } from "vitest";

import { normalizeOpenDialogPath } from "@/lib/dialog-path";

describe("normalizeOpenDialogPath", () => {
  it("returns null for cancel", () => {
    expect(normalizeOpenDialogPath(null)).toBeNull();
    expect(normalizeOpenDialogPath(undefined)).toBeNull();
  });

  it("accepts a single path", () => {
    expect(normalizeOpenDialogPath("/Users/me/Notes")).toBe("/Users/me/Notes");
  });

  it("accepts first path from array", () => {
    expect(normalizeOpenDialogPath(["/a", "/b"])).toBe("/a");
  });
});
