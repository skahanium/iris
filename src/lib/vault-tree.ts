import type { FileListItem } from "@/types/ipc";

export interface VaultTreeNode {
  name: string;
  path: string;
  kind: "folder" | "file";
  children?: VaultTreeNode[];
  /** 文件锁定状态（仅 file 节点；由 FileListItem.isLocked 填充）。 */
  locked?: boolean;
  /** 用户可见标题（仅 file 节点；由 FileListItem.title 填充，缺省回退 name）。 */
  title?: string;
}

/** Normalize a vault folder prefix to `segment/` form with forward slashes. */
export function normalizeFolderPrefix(prefix: string): string {
  if (!prefix) return "";
  const norm = prefix.replace(/\\/g, "/").replace(/\/+/g, "/");
  if (norm === "/") return "";
  return norm.endsWith("/") ? norm : `${norm}/`;
}

/** Parent folder prefix for `notes/sub/` → `notes/`; root child `sub/` → ``. */
export function folderParentPath(folderPath: string): string {
  const norm = normalizeFolderPrefix(folderPath);
  const segments = norm.slice(0, -1).split("/").filter(Boolean);
  segments.pop();
  if (segments.length === 0) return "";
  return `${segments.join("/")}/`;
}

/** Join parent folder prefix and child name (file or folder segment). */
export function joinVaultChildPath(parent: string, childName: string): string {
  const child = childName.replace(/\\/g, "/").replace(/^\/+|\/+$/g, "");
  const base = normalizeFolderPrefix(parent);
  if (!child) return base;
  if (!base) return child;
  return `${base}${child}`;
}

/** Place a note file under a folder prefix (`notes/` + `doc.md` → `notes/doc.md`). */
export function notePathInFolder(
  folderPrefix: string,
  fileName: string,
): string {
  const base = fileName.trim().replace(/\\/g, "/");
  if (!base) return "";
  const withExt = base.endsWith(".md") ? base : `${base}.md`;
  return joinVaultChildPath(folderPrefix, withExt);
}

/** Build a folder tree from flat file paths and explicit folder prefixes. */
export function buildVaultTree(
  files: FileListItem[],
  folderPrefixes: string[] = [],
): VaultTreeNode[] {
  const root: VaultTreeNode[] = [];
  const folderMap = new Map<string, VaultTreeNode>();

  const ensureFolder = (folderPath: string, name: string): VaultTreeNode => {
    const normalized = normalizeFolderPrefix(folderPath);
    const existing = folderMap.get(normalized);
    if (existing) return existing;
    const node: VaultTreeNode = {
      name,
      path: normalized,
      kind: "folder",
      children: [],
    };
    folderMap.set(normalized, node);
    const parentPath = folderParentPath(normalized);
    if (!parentPath) {
      if (!root.includes(node)) {
        root.push(node);
      }
    } else {
      const parentName =
        parentPath.replace(/\/$/, "").split("/").pop() ?? parentPath;
      const parent = ensureFolder(parentPath, parentName);
      parent.children = parent.children ?? [];
      if (!parent.children.includes(node)) {
        parent.children.push(node);
      }
    }
    return node;
  };

  for (const folder of folderPrefixes) {
    const norm = normalizeFolderPrefix(folder);
    if (!norm) continue;
    const name = norm.replace(/\/$/, "").split("/").pop() ?? norm;
    ensureFolder(norm, name);
  }

  for (const f of files) {
    const norm = f.path.replace(/\\/g, "/");
    const parts = norm.split("/");
    if (parts.length > 1) {
      let acc = "";
      for (let i = 0; i < parts.length - 1; i += 1) {
        acc += `${parts[i]}/`;
        ensureFolder(acc, parts[i] ?? acc);
      }
    }
    const fileName = parts[parts.length - 1] ?? norm;
    const parentPath =
      parts.length > 1 ? `${parts.slice(0, -1).join("/")}/` : "";
    const fileNode: VaultTreeNode = {
      name: fileName,
      path: norm,
      kind: "file",
      locked: f.isLocked,
      title: f.title || fileName,
    };
    if (parentPath) {
      const parent = folderMap.get(parentPath);
      if (parent) {
        parent.children = parent.children ?? [];
        parent.children.push(fileNode);
      } else {
        root.push(fileNode);
      }
    } else {
      root.push(fileNode);
    }
  }

  const sortNodes = (nodes: VaultTreeNode[]) => {
    nodes.sort((a, b) => {
      if (a.kind !== b.kind) return a.kind === "folder" ? -1 : 1;
      return a.name.localeCompare(b.name, "zh-CN");
    });
    for (const n of nodes) {
      if (n.children) sortNodes(n.children);
    }
  };
  sortNodes(root);
  return root;
}

export function listFilesInFolder(
  files: FileListItem[],
  folderPrefix: string,
): FileListItem[] {
  if (!folderPrefix) return files;
  const prefix = normalizeFolderPrefix(folderPrefix);
  return files.filter((f) => f.path.replace(/\\/g, "/").startsWith(prefix));
}

/** Return only the files directly contained by a folder; descendants are excluded. */
export function listDirectFilesInFolder<T extends { path: string }>(
  files: T[],
  folderPrefix: string,
): T[] {
  const selectedFolder = normalizeFolderPrefix(folderPrefix);
  return files.filter((file) => {
    const normalized = file.path.replace(/\\/g, "/");
    const separatorIndex = normalized.lastIndexOf("/");
    const parent =
      separatorIndex >= 0 ? normalized.slice(0, separatorIndex + 1) : "";
    return parent === selectedFolder;
  });
}

/** Folder-only node used by the compact workspace navigator. */
export interface FolderTreeNode {
  name: string;
  path: string;
  children: FolderTreeNode[];
  /** Markdown files directly inside this folder; descendants are excluded. */
  directMarkdownCount: number;
}

export interface FolderSort {
  key: "name" | "count";
  direction: "asc" | "desc";
}

/**
 * Build a folder-only tree from catalog files plus explicit empty folders.
 * The caller supplies the Markdown predicate because catalog rows also include media.
 */
export function buildFolderTree<T extends { path: string }>(
  files: T[],
  folderPrefixes: string[] = [],
  isMarkdownFile: (file: T) => boolean,
): FolderTreeNode[] {
  const root: FolderTreeNode[] = [];
  const folders = new Map<string, FolderTreeNode>();

  const ensureFolder = (folderPath: string): FolderTreeNode => {
    const normalized = normalizeFolderPrefix(folderPath);
    const existing = folders.get(normalized);
    if (existing) return existing;

    const name = normalized.replace(/\/$/, "").split("/").pop() ?? normalized;
    const node: FolderTreeNode = {
      name,
      path: normalized,
      children: [],
      directMarkdownCount: 0,
    };
    folders.set(normalized, node);

    const parentPath = folderParentPath(normalized);
    if (parentPath) ensureFolder(parentPath).children.push(node);
    else root.push(node);
    return node;
  };

  for (const folder of folderPrefixes) {
    const normalized = normalizeFolderPrefix(folder);
    if (normalized) ensureFolder(normalized);
  }

  for (const file of files) {
    const normalized = file.path.replace(/\\/g, "/");
    const separatorIndex = normalized.lastIndexOf("/");
    if (separatorIndex < 0) continue;

    const parent = normalized.slice(0, separatorIndex + 1);
    const segments = parent.slice(0, -1).split("/").filter(Boolean);
    let prefix = "";
    for (const segment of segments) {
      prefix += `${segment}/`;
      ensureFolder(prefix);
    }
    if (isMarkdownFile(file)) ensureFolder(parent).directMarkdownCount += 1;
  }

  return sortFolderTree(root, { key: "name", direction: "asc" });
}

/** Sort each sibling group without flattening or changing hierarchy. */
export function sortFolderTree(
  tree: FolderTreeNode[],
  sort: FolderSort,
): FolderTreeNode[] {
  const direction = sort.direction === "asc" ? 1 : -1;
  const compareName = (left: FolderTreeNode, right: FolderTreeNode) =>
    left.name.localeCompare(right.name, "zh-Hans-CN");
  const compare = (left: FolderTreeNode, right: FolderTreeNode) => {
    if (sort.key === "count") {
      const countDifference =
        (left.directMarkdownCount - right.directMarkdownCount) * direction;
      if (countDifference !== 0) return countDifference;
      return compareName(left, right);
    }
    const nameDifference = compareName(left, right) * direction;
    if (nameDifference !== 0) return nameDifference;
    return left.path.localeCompare(right.path, "zh-Hans-CN");
  };

  return [...tree].sort(compare).map((node) => ({
    ...node,
    children: sortFolderTree(node.children, sort),
  }));
}

/** Visible folder rows with ancestor continuation data for tree rails. */
export interface FolderTreeRow {
  node: FolderTreeNode;
  depth: number;
  ancestorHasNextSibling: boolean[];
}

/** Flatten only expanded folder branches for the navigator's upper tree. */
export function flattenFolderTree(
  tree: FolderTreeNode[],
  expanded: ReadonlySet<string>,
): FolderTreeRow[] {
  const rows: FolderTreeRow[] = [];
  const walk = (
    nodes: FolderTreeNode[],
    depth: number,
    ancestorHasNextSibling: boolean[],
  ) => {
    for (const [index, node] of nodes.entries()) {
      rows.push({ node, depth, ancestorHasNextSibling });
      if (expanded.has(node.path)) {
        walk(node.children, depth + 1, [
          ...ancestorHasNextSibling,
          index < nodes.length - 1,
        ]);
      }
    }
  };
  walk(tree, 0, []);
  return rows;
}

/** 可见树行：节点 + 深度（键盘 ↑/↓ 导航与当前文件自动显露用）。 */
export interface VaultTreeRow {
  node: VaultTreeNode;
  depth: number;
  /** 各祖先层级在当前行之后是否还有同级节点，用于绘制连续树形导轨。 */
  ancestorHasNextSibling: boolean[];
}

/** 按展开集合把树展平为可见行序列；未展开的文件夹不进入序列。 */
export function flattenVaultTree(
  tree: VaultTreeNode[],
  expanded: ReadonlySet<string>,
): VaultTreeRow[] {
  const rows: VaultTreeRow[] = [];
  const walk = (
    nodes: VaultTreeNode[],
    depth: number,
    ancestorHasNextSibling: boolean[],
  ) => {
    for (const [index, node] of nodes.entries()) {
      rows.push({ node, depth, ancestorHasNextSibling });
      if (node.kind === "folder" && expanded.has(node.path) && node.children) {
        walk(node.children, depth + 1, [
          ...ancestorHasNextSibling,
          index < nodes.length - 1,
        ]);
      }
    }
  };
  walk(tree, 0, []);
  return rows;
}
