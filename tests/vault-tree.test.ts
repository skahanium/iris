import { describe, expect, it } from "vitest";

import {
  buildFolderTree,
  buildVaultTree,
  flattenFolderTree,
  flattenVaultTree,
  folderParentPath,
  joinVaultChildPath,
  listDirectFilesInFolder,
  listFilesInFolder,
  notePathInFolder,
  sortFolderTree,
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

  it("listDirectFilesInFolder 只返回所选目录的直属文件", () => {
    const files: FileListItem[] = [
      { path: "notes/a.md", title: "a", updatedAt: "", isLocked: false },
      {
        path: "notes/sub/b.md",
        title: "b",
        updatedAt: "",
        isLocked: false,
      },
      { path: "root.md", title: "root", updatedAt: "", isLocked: false },
    ];

    expect(
      listDirectFilesInFolder(files, "notes/").map((file) => file.path),
    ).toEqual(["notes/a.md"]);
    expect(listDirectFilesInFolder(files, "").map((file) => file.path)).toEqual(
      ["root.md"],
    );
  });

  it("buildFolderTree 仅包含目录，并统计直属 Markdown", () => {
    const files = [
      {
        path: "计量统计/补充/OR值.md",
        title: "OR值",
        updatedAt: "",
        isLocked: false,
        kind: "note" as const,
      },
      {
        path: "计量统计/补充/图.png",
        title: "图",
        updatedAt: "",
        isLocked: false,
        kind: "media" as const,
      },
      {
        path: "计量统计/补充/子目录/后代.md",
        title: "后代",
        updatedAt: "",
        isLocked: false,
        kind: "note" as const,
      },
    ];
    const tree = buildFolderTree(
      files,
      ["空目录/"],
      (file) => file.kind === "note",
    );

    expect(tree.map((node) => node.path)).toEqual(["计量统计/", "空目录/"]);
    const statistics = tree.find((node) => node.path === "计量统计/");
    const supplement = statistics?.children.find(
      (node) => node.path === "计量统计/补充/",
    );
    expect(supplement?.directMarkdownCount).toBe(1);
    expect(supplement?.children.map((node) => node.path)).toEqual([
      "计量统计/补充/子目录/",
    ]);
    expect(
      flattenFolderTree(tree, new Set(["计量统计/"])).every((row) =>
        row.node.path.endsWith("/"),
      ),
    ).toBe(true);
  });

  it("sortFolderTree 仅重排同级目录，并在计数相同时稳定按名称排序", () => {
    const tree = buildFolderTree(
      [
        { path: "甲/a.md", title: "a", updatedAt: "", isLocked: false },
        { path: "乙/a.md", title: "a", updatedAt: "", isLocked: false },
        { path: "乙/b.md", title: "b", updatedAt: "", isLocked: false },
      ],
      [],
      () => true,
    );

    expect(
      sortFolderTree(tree, { key: "count", direction: "desc" }).map(
        (node) => node.path,
      ),
    ).toEqual(["乙/", "甲/"]);
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
