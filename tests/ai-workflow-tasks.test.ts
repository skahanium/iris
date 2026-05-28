import { describe, expect, it } from "vitest";

import { isPlaceholderTitle } from "@/lib/path-sync";
import { sha256Hex } from "@/lib/content-hash";
import { WORKFLOW_TASK_DEFINITIONS } from "@/types/ai";

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

describe("AI workflow task rail", () => {
  it("includes chapter/document and rules center tabs", () => {
    const ids = WORKFLOW_TASK_DEFINITIONS.map((t) => t.id);
    expect(ids).toContain("chapter_doc");
    expect(ids).toContain("rules");
    expect(ids).toHaveLength(7);
  });
});
