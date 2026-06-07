import { describe, expect, it } from "vitest";

import {
  buildVaultTree,
  folderParentPath,
  joinVaultChildPath,
  listFilesInFolder,
  notePathInFolder,
} from "@/lib/vault-tree";
import type { FileListItem } from "@/types/ipc";

describe("vault-tree", () => {
  it("includes empty folders from folder prefixes", () => {
    const tree = buildVaultTree([], ["notes/", "notes/inbox/"]);
    expect(tree.map((n) => n.path)).toContain("notes/");
    const notes = tree.find((n) => n.path === "notes/");
    expect(notes?.children?.some((c) => c.path === "notes/inbox/")).toBe(true);
  });

  it("joinVaultChildPath keeps slashes between parent and child", () => {
    expect(joinVaultChildPath("notes/", "sub")).toBe("notes/sub");
    expect(joinVaultChildPath("", "sub")).toBe("sub");
    expect(notePathInFolder("notes/", "doc")).toBe("notes/doc.md");
  });

  it("folderParentPath strips the last segment", () => {
    expect(folderParentPath("notes/inbox/")).toBe("notes/");
    expect(folderParentPath("inbox/")).toBe("");
  });

  it("listFilesInFolder respects selected prefix", () => {
    const files: FileListItem[] = [
      { path: "notes/a.md", title: "a", updated_at: "", isLocked: false },
      { path: "b.md", title: "b", updated_at: "", isLocked: false },
    ];
    expect(listFilesInFolder(files, "notes/")).toHaveLength(1);
  });
});
