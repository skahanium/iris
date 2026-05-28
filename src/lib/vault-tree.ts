import type { FileListItem } from "@/types/ipc";

export interface VaultTreeNode {
  name: string;
  path: string;
  kind: "folder" | "file";
  children?: VaultTreeNode[];
}

/** Build a folder tree from flat file paths. */
export function buildVaultTree(files: FileListItem[]): VaultTreeNode[] {
  const root: VaultTreeNode[] = [];
  const folderMap = new Map<string, VaultTreeNode>();

  const ensureFolder = (folderPath: string, name: string): VaultTreeNode => {
    const existing = folderMap.get(folderPath);
    if (existing) return existing;
    const node: VaultTreeNode = {
      name,
      path: folderPath,
      kind: "folder",
      children: [],
    };
    folderMap.set(folderPath, node);
    if (folderPath.includes("/")) {
      const parentPath = folderPath.replace(/\/$/, "").split("/").slice(0, -1).join("/");
      const parentName = parentPath.split("/").pop() ?? parentPath;
      const parent = ensureFolder(`${parentPath}/`, parentName);
      parent.children = parent.children ?? [];
      parent.children.push(node);
    } else {
      root.push(node);
    }
    return node;
  };

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
  const prefix = folderPrefix.endsWith("/") ? folderPrefix : `${folderPrefix}/`;
  return files.filter((f) => f.path.replace(/\\/g, "/").startsWith(prefix));
}
