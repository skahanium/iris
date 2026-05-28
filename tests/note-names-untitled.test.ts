import { describe, expect, it } from "vitest";

import { allocateUntitledDocumentName } from "@/lib/note-names";
import type { FileListItem } from "@/types/ipc";

function file(path: string, title: string): FileListItem {
  return { path, title, updated_at: "" };
}

describe("allocateUntitledDocumentName", () => {
  it("returns 无标题1 when unused", () => {
    const { title, path } = allocateUntitledDocumentName([]);
    expect(title).toBe("无标题1");
    expect(path).toBe("无标题1.md");
  });

  it("returns 无标题2 when 无标题1 is taken", () => {
    const { title, path } = allocateUntitledDocumentName([
      file("无标题1.md", "无标题1"),
    ]);
    expect(title).toBe("无标题2");
    expect(path).toBe("无标题2.md");
  });

  it("fills the lowest free numeric suffix", () => {
    const { title } = allocateUntitledDocumentName([
      file("无标题1.md", "无标题1"),
      file("无标题3.md", "无标题3"),
    ]);
    expect(title).toBe("无标题2");
  });

  it("respects extraTaken from open tabs", () => {
    const { title } = allocateUntitledDocumentName([], ["无标题1", "无标题2"]);
    expect(title).toBe("无标题3");
  });

  it("ignores legacy untitled-* paths when allocating", () => {
    const { title } = allocateUntitledDocumentName([
      file("untitled-1700000000.md", "untitled-1700000000"),
    ]);
    expect(title).toBe("无标题1");
  });
});
