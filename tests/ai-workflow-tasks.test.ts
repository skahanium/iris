import { describe, expect, it } from "vitest";

import { isPlaceholderTitle } from "@/lib/path-sync";
import { sha256Hex } from "@/lib/content-hash";

describe("note workflow helpers", () => {
  it("detects placeholder titles", () => {
    expect(isPlaceholderTitle("新建文档")).toBe(true);
    expect(isPlaceholderTitle("untitled-1")).toBe(true);
    expect(isPlaceholderTitle("民法笔记")).toBe(false);
  });

  it("computes stable sha256 hex", async () => {
    const a = await sha256Hex("hello");
    const b = await sha256Hex("hello");
    expect(a).toBe(b);
    expect(a).toHaveLength(64);
  });
});
