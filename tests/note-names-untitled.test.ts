import { describe, expect, it } from "vitest";

import {
  allocateNewDocumentName,
  allocateUntitledDocumentName,
} from "@/lib/note-names";
import type { FileListItem } from "@/types/ipc";

function file(path: string, title: string): FileListItem {
  return { path, title, updated_at: "" };
}

describe("allocateNewDocumentName", () => {
  it("returns 新建文档 when unused", () => {
    const { title, path } = allocateNewDocumentName([]);
    expect(title).toBe("新建文档");
    expect(path).toBe("新建文档.md");
  });

  it("returns 新建文档（1） when 新建文档 is taken", () => {
    const { title, path } = allocateNewDocumentName([
      file("新建文档.md", "新建文档"),
    ]);
    expect(title).toBe("新建文档（1）");
    expect(path).toBe("新建文档（1）.md");
  });

  it("fills the lowest free numeric suffix", () => {
    const { title } = allocateNewDocumentName([
      file("新建文档.md", "新建文档"),
      file("新建文档（2）.md", "新建文档（2）"),
    ]);
    expect(title).toBe("新建文档（1）");
  });

  it("respects extraTaken from open tabs", () => {
    const { title } = allocateNewDocumentName([], ["新建文档", "新建文档（1）"]);
    expect(title).toBe("新建文档（2）");
  });

  it("ignores legacy untitled-* paths when allocating", () => {
    const { title } = allocateNewDocumentName([
      file("untitled-1700000000.md", "untitled-1700000000"),
    ]);
    expect(title).toBe("新建文档");
  });

  it("ignores legacy 无标题N labels when allocating", () => {
    const { title } = allocateNewDocumentName([
      file("无标题1.md", "无标题1"),
      file("无标题2.md", "无标题2"),
    ]);
    expect(title).toBe("新建文档");
  });
});

describe("allocateUntitledDocumentName (legacy)", () => {
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
