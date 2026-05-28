import { describe, expect, it } from "vitest";

import {
  displayTitleForFileListItem,
  isInternalUntitledPath,
  resolveNoteDisplayTitle,
} from "@/lib/note-display";
import type { FileListItem } from "@/types/ipc";

describe("note-display", () => {
  it("detects internal untitled paths", () => {
    expect(isInternalUntitledPath("untitled-1700000000.md")).toBe(true);
    expect(isInternalUntitledPath("notes/untitled-42.md")).toBe(true);
    expect(isInternalUntitledPath("无标题1.md")).toBe(false);
  });

  it("never exposes untitled-* to users", () => {
    expect(
      resolveNoteDisplayTitle({
        path: "untitled-99.md",
        title: "untitled-99",
      }),
    ).toBe("无标题1");
    expect(
      displayTitleForFileListItem({
        path: "untitled-99.md",
        title: "untitled-99",
        updated_at: "",
      } satisfies FileListItem),
    ).toBe("无标题1");
  });

  it("keeps real titles", () => {
    expect(
      resolveNoteDisplayTitle({
        path: "早餐.md",
        title: "早餐",
      }),
    ).toBe("早餐");
  });
});
