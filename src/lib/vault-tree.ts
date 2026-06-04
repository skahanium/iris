import type { FileListItem } from "@/types/ipc";

export interface VaultTreeNode {
  name: string;
  path: string;
  kind: "folder" | "file";
  children?: VaultTreeNode[];
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
