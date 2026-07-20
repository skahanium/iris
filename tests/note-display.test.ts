import { describe, expect, it } from "vitest";

import {
  displayTitleForFileListItem,
  isInternalUntitledPath,
  mapLegacyPlaceholderStemToDisplay,
  resolveNoteDisplayTitle,
  UNNAMED_DOCUMENT_PREFIX,
} from "@/lib/note-display";
import type { FileListItem } from "@/types/ipc";

describe("note-display", () => {
  it("detects internal untitled paths", () => {
    expect(isInternalUntitledPath("untitled-1700000000.md")).toBe(true);
    expect(isInternalUntitledPath("notes/untitled-42.md")).toBe(true);
    expect(isInternalUntitledPath("无标题1.md")).toBe(false);
    expect(isInternalUntitledPath("未命名文档.md")).toBe(false);
  });

  it("never exposes untitled-* to users", () => {
    expect(
      resolveNoteDisplayTitle({
        path: "untitled-99.md",
        title: "untitled-99",
      }),
    ).toBe(UNNAMED_DOCUMENT_PREFIX);
    expect(
      displayTitleForFileListItem({
        path: "untitled-99.md",
        title: "untitled-99",
        updatedAt: "",
        isLocked: false,
      } satisfies FileListItem),
    ).toBe(UNNAMED_DOCUMENT_PREFIX);
  });

  it("maps legacy 新建文档 paths to 未命名文档", () => {
    expect(mapLegacyPlaceholderStemToDisplay("新建文档")).toBe(
      UNNAMED_DOCUMENT_PREFIX,
    );
    expect(mapLegacyPlaceholderStemToDisplay("新建文档（2）")).toBe(
      "未命名文档（2）",
    );
    expect(
      resolveNoteDisplayTitle({
        path: "新建文档.md",
        title: "新建文档",
      }),
    ).toBe(UNNAMED_DOCUMENT_PREFIX);
    expect(
      resolveNoteDisplayTitle({
        path: "新建文档（1）.md",
        title: "新建文档（1）",
      }),
    ).toBe("未命名文档（1）");
  });

  it("keeps real titles", () => {
    expect(
      resolveNoteDisplayTitle({
        path: "早餐.md",
        title: "早餐",
      }),
    ).toBe("早餐");
  });

  it("uses the filename when a legacy explicit title is empty", () => {
    expect(
      resolveNoteDisplayTitle({
        path: "早餐.md",
        title: "",
      }),
    ).toBe("早餐");
  });
});
