import { describe, expect, it } from "vitest";

import {
  allocateNewDocumentName,
  DEFAULT_NEW_DOCUMENT_TITLE,
  titleToNotePath,
} from "@/lib/note-names";
import type { FileListItem } from "@/types/ipc";

function file(path: string, title: string): FileListItem {
  return { path, title, updated_at: "" };
}

describe("allocateNewDocumentName", () => {
  it("returns 新建文档 when unused", () => {
    const { title, path } = allocateNewDocumentName([]);
    expect(title).toBe(DEFAULT_NEW_DOCUMENT_TITLE);
    expect(path).toBe("新建文档.md");
  });

  it("returns 新建文档（1） when base title is taken", () => {
    const { title, path } = allocateNewDocumentName([
      file("新建文档.md", "新建文档"),
    ]);
    expect(title).toBe("新建文档（1）");
    expect(path).toBe("新建文档（1）.md");
  });

  it("fills the lowest free suffix", () => {
    const { title } = allocateNewDocumentName([
      file("新建文档.md", "新建文档"),
      file("新建文档（2）.md", "新建文档（2）"),
    ]);
    expect(title).toBe("新建文档（1）");
  });

  it("respects taken titles even when path differs", () => {
    const { title } = allocateNewDocumentName([file("note-1.md", "新建文档")]);
    expect(title).toBe("新建文档（1）");
  });

  it("uses a custom title hint inside the selected folder", () => {
    const { title, path } = allocateNewDocumentName(
      [file("notes/会议.md", "会议")],
      [],
      "notes/",
      "会议",
    );

    expect(title).toBe("会议（1）");
    expect(path).toBe("notes/会议（1）.md");
  });
});

describe("titleToNotePath", () => {
  it("removes invalid path characters", () => {
    expect(titleToNotePath("bad:name")).toBe("bad_name.md");
  });
});
