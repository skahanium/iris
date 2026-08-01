import { describe, expect, it } from "vitest";

import {
  buildVaultTree,
  flattenVaultTree,
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
      { path: "notes/a.md", title: "a", updatedAt: "", isLocked: false },
      { path: "b.md", title: "b", updatedAt: "", isLocked: false },
    ];
    expect(listFilesInFolder(files, "notes/")).toHaveLength(1);
  });

  it("flattenVaultTree yields visible rows by expanded set with depth", () => {
    const tree = buildVaultTree(
      [
        { path: "z.md", title: "z", updatedAt: "", isLocked: false },
        { path: "notes/a.md", title: "a", updatedAt: "", isLocked: false },
        { path: "notes/sub/b.md", title: "b", updatedAt: "", isLocked: false },
      ],
      ["notes/"],
    );

    const collapsed = flattenVaultTree(tree, new Set());
    expect(collapsed.map((row) => row.node.path)).toEqual(["notes/", "z.md"]);

    const expanded = flattenVaultTree(tree, new Set(["notes/"]));
    expect(expanded.map((row) => row.node.path)).toEqual([
      "notes/",
      "notes/sub/",
      "notes/a.md",
      "z.md",
    ]);
    expect(expanded[0]?.depth).toBe(0);
    expect(expanded[1]?.depth).toBe(1);
    expect(expanded[2]?.depth).toBe(1);

    const deep = flattenVaultTree(tree, new Set(["notes/", "notes/sub/"]));
    expect(deep.map((row) => row.node.path)).toEqual([
      "notes/",
      "notes/sub/",
      "notes/sub/b.md",
      "notes/a.md",
      "z.md",
    ]);
    expect(deep.map((row) => row.ancestorHasNextSibling)).toEqual([
      [],
      [true],
      [true, true],
      [true],
      [],
    ]);
  });
});
