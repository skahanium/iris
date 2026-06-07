import { describe, expect, it } from "vitest";

import {
  allocateNewDocumentName,
  allocateUntitledDocumentName,
} from "@/lib/note-names";
import { UNNAMED_DOCUMENT_PREFIX as DISPLAY_PREFIX } from "@/lib/note-display";
import type { FileListItem } from "@/types/ipc";

function file(path: string, title: string): FileListItem {
  return { path, title, updated_at: "", isLocked: false };
}

describe("allocateNewDocumentName", () => {
  it("returns 未命名文档 when unused", () => {
    const { title, path } = allocateNewDocumentName([]);
    expect(title).toBe(DISPLAY_PREFIX);
    expect(path).toBe("未命名文档.md");
  });

  it("returns 未命名文档（1） when 未命名文档 is taken", () => {
    const { title, path } = allocateNewDocumentName([
      file("未命名文档.md", "未命名文档"),
    ]);
    expect(title).toBe("未命名文档（1）");
    expect(path).toBe("未命名文档（1）.md");
  });

  it("fills the lowest free numeric suffix", () => {
    const { title } = allocateNewDocumentName([
      file("未命名文档.md", "未命名文档"),
      file("未命名文档（2）.md", "未命名文档（2）"),
    ]);
    expect(title).toBe("未命名文档（1）");
  });

  it("respects extraTaken from open tabs", () => {
    const { title } = allocateNewDocumentName(
      [],
      ["未命名文档", "未命名文档（1）"],
    );
    expect(title).toBe("未命名文档（2）");
  });

  it("ignores legacy untitled-* paths when allocating", () => {
    const { title } = allocateNewDocumentName([
      file("untitled-1700000000.md", "untitled-1700000000"),
    ]);
    expect(title).toBe(DISPLAY_PREFIX);
  });

  it("treats legacy 新建文档 and 无标题 as taken", () => {
    const { title } = allocateNewDocumentName([
      file("新建文档.md", "新建文档"),
      file("无标题1.md", "无标题1"),
      file("无标题2.md", "无标题2"),
    ]);
    expect(title).toBe("未命名文档（2）");
  });
});

describe("allocateUntitledDocumentName (alias)", () => {
  it("matches allocateNewDocumentName", () => {
    const files = [file("未命名文档.md", "未命名文档")];
    expect(allocateUntitledDocumentName(files).title).toBe(
      allocateNewDocumentName(files).title,
    );
  });
});
