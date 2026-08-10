import { ChevronRight, Folder, FolderOpen } from "lucide-react";
import { useCallback, useMemo, useRef, useState } from "react";

import { flattenFolderTree, type FolderTreeNode } from "@/lib/vault-tree";
import { cn } from "@/lib/utils";

export interface WorkspaceNavigatorTreeProps {
  tree: FolderTreeNode[];
  expanded: ReadonlySet<string>;
  selectedFolder: string;
  rootMarkdownCount?: number;
  onToggleFolder: (path: string) => void;
  onSelectFolder: (path: string) => void;
  /** 打开文件夹操作菜单（右键 / Shift+F10 / 菜单键）。 */
  onRowMenu: (
    node: FolderTreeNode,
    rowIndex: number,
    x?: number,
    y?: number,
  ) => void;
}

interface FolderRow {
  node: FolderTreeNode | null;
  path: string;
  name: string;
  count: number;
  depth: number;
  ancestorHasNextSibling: boolean[];
}

/**
 * Workspace Navigator 上层：仅用于定位目录。
 *
 * 文件永远不在这个 tree 中渲染；目录名称选择下层范围，箭头和左右键才控制展开。
 */
export function WorkspaceNavigatorTree({
  tree,
  expanded,
  selectedFolder,
  rootMarkdownCount = 0,
  onToggleFolder,
  onSelectFolder,
  onRowMenu,
}: WorkspaceNavigatorTreeProps) {
  const rows = useMemo<FolderRow[]>(
    () => [
      {
        node: null,
        path: "",
        name: "根目录",
        count: rootMarkdownCount,
        depth: 0,
        ancestorHasNextSibling: [],
      },
      ...flattenFolderTree(tree, expanded).map((row) => ({
        node: row.node,
        path: row.node.path,
        name: row.node.name,
        count: row.node.directMarkdownCount,
        depth: row.depth + 1,
        ancestorHasNextSibling: row.ancestorHasNextSibling,
      })),
    ],
    [expanded, rootMarkdownCount, tree],
  );
  const [focusedIndex, setFocusedIndex] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);

  const parentIndexOf = useCallback(
    (index: number): number => {
      const depth = rows[index]?.depth ?? 0;
      for (let candidate = index - 1; candidate >= 0; candidate -= 1) {
        if ((rows[candidate]?.depth ?? 0) < depth) return candidate;
      }
      return -1;
    },
    [rows],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      const row = rows[focusedIndex];
      if (!row) return;
      const isFolder = row.node !== null;
      const isExpanded = isFolder && expanded.has(row.path);

      switch (event.key) {
        case "ArrowDown":
          event.preventDefault();
          setFocusedIndex((index) => Math.min(index + 1, rows.length - 1));
          break;
        case "ArrowUp":
          event.preventDefault();
          setFocusedIndex((index) => Math.max(index - 1, 0));
          break;
        case "ArrowLeft":
          event.preventDefault();
          if (isFolder && isExpanded) onToggleFolder(row.path);
          else {
            const parentIndex = parentIndexOf(focusedIndex);
            if (parentIndex >= 0) setFocusedIndex(parentIndex);
          }
          break;
        case "ArrowRight":
          event.preventDefault();
          if (!isFolder) break;
          if (!isExpanded) onToggleFolder(row.path);
          else if (focusedIndex + 1 < rows.length) {
            setFocusedIndex(focusedIndex + 1);
          }
          break;
        case "Enter":
          event.preventDefault();
          onSelectFolder(row.path);
          break;
        case "F10":
          if (event.shiftKey && row.node) {
            event.preventDefault();
            onRowMenu(row.node, focusedIndex);
          }
          break;
        case "ContextMenu":
          if (row.node) {
            event.preventDefault();
            onRowMenu(row.node, focusedIndex);
          }
          break;
        default:
          break;
      }
    },
    [
      expanded,
      focusedIndex,
      onRowMenu,
      onSelectFolder,
      onToggleFolder,
      parentIndexOf,
      rows,
    ],
  );

  return (
    <div
      ref={containerRef}
      role="tree"
      aria-label="文件夹"
      tabIndex={0}
      data-testid="workspace-navigator-tree"
      className="iris-workspace-navigator-scroll min-h-0 flex-1 overflow-y-auto px-1.5 py-1 focus:outline-none"
      onKeyDown={handleKeyDown}
    >
      {rows.map((row, index) => {
        const isRoot = row.node === null;
        const isExpanded = !isRoot && expanded.has(row.path);
        const isSelected = row.path === selectedFolder;
        const focused = focusedIndex === index;
        return (
          <div
            key={row.path || "root"}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-expanded={isRoot ? undefined : isExpanded}
            aria-selected={isSelected || undefined}
            data-selected={isSelected || undefined}
            data-testid={
              isRoot ? "workspace-tree-root" : "workspace-tree-folder"
            }
            className={cn(
              "group relative flex h-[30px] cursor-pointer items-center gap-1 rounded-md px-1.5 text-[13px] text-muted-foreground transition-colors duration-150 motion-reduce:transition-none",
              isSelected &&
                "bg-[hsl(var(--brand)/0.12)] text-[hsl(var(--brand))] before:absolute before:inset-y-1 before:left-0 before:w-px before:bg-[hsl(var(--brand))]",
              focused &&
                !isSelected &&
                "ring-1 ring-inset ring-[hsl(var(--brand)/0.35)]",
              !isSelected && "hover:bg-muted/50 hover:text-foreground",
            )}
            style={{ paddingLeft: `${0.5 + row.depth * 1.125}rem` }}
            onClick={() => {
              setFocusedIndex(index);
              onSelectFolder(row.path);
            }}
            onContextMenu={(event) => {
              if (!row.node) return;
              event.preventDefault();
              setFocusedIndex(index);
              onRowMenu(row.node, index, event.clientX, event.clientY);
            }}
          >
            {row.ancestorHasNextSibling.map((hasNextSibling, guideDepth) =>
              hasNextSibling ? (
                <span
                  key={guideDepth}
                  aria-hidden="true"
                  data-testid="workspace-tree-guide"
                  className="pointer-events-none absolute inset-y-0 w-px bg-border-subtle/80"
                  style={{ left: `${1.125 + guideDepth * 1.125}rem` }}
                />
              ) : null,
            )}
            {isRoot ? (
              <span className="w-4 shrink-0" aria-hidden="true" />
            ) : (
              <button
                type="button"
                aria-label={`${isExpanded ? "收起" : "展开"} ${row.name}`}
                className="iris-focus-soft inline-flex h-4 w-4 shrink-0 items-center justify-center rounded-sm text-muted-foreground/70"
                onClick={(event) => {
                  event.stopPropagation();
                  onToggleFolder(row.path);
                }}
              >
                <ChevronRight
                  aria-hidden="true"
                  className={cn(
                    "h-3.5 w-3.5 transition-transform duration-150 motion-reduce:transition-none",
                    isExpanded && "rotate-90",
                  )}
                />
              </button>
            )}
            {isExpanded ? (
              <FolderOpen
                aria-hidden="true"
                data-icon="folder-open"
                className="h-4 w-4 shrink-0 text-muted-foreground/75"
              />
            ) : (
              <Folder
                aria-hidden="true"
                data-icon="folder-closed"
                className="h-4 w-4 shrink-0 text-muted-foreground/75"
              />
            )}
            <span className="min-w-0 flex-1 truncate">{row.name}</span>
            <span className="min-w-4 rounded-sm bg-muted/50 px-1 text-right text-[11px] text-muted-foreground">
              {row.count}
            </span>
          </div>
        );
      })}
    </div>
  );
}
