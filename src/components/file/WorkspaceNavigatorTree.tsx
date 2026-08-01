import { ChevronRight, FileText, Folder, FolderOpen, Lock } from "lucide-react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import { flattenVaultTree, type VaultTreeNode } from "@/lib/vault-tree";
import { cn } from "@/lib/utils";

export interface WorkspaceNavigatorTreeProps {
  tree: VaultTreeNode[];
  expanded: ReadonlySet<string>;
  /** 当前打开的文档路径（brand marker + 自动显露祖先）。 */
  activePath: string | null;
  onToggleFolder: (path: string) => void;
  onOpenFile: (node: VaultTreeNode) => void;
  onPrepareFile: (node: VaultTreeNode) => void;
  /** 打开行操作菜单（右键 / Shift+F10 / 菜单键）。 */
  onRowMenu: (node: VaultTreeNode, rowIndex: number) => void;
}

/** 当前文档祖先文件夹前缀（`notes/sub/b.md` → `["notes/", "notes/sub/"]`）。 */
function ancestorFoldersOf(path: string): string[] {
  const parts = path.replace(/\\/g, "/").split("/").filter(Boolean);
  parts.pop();
  const ancestors: string[] = [];
  let acc = "";
  for (const part of parts) {
    acc += `${part}/`;
    ancestors.push(acc);
  }
  return ancestors;
}

/**
 * 轻量工作区目录树（v1.2.19 Task 7）。
 *
 * 单列 folder/file 树：文件夹在文件之前（buildVaultTree 的 zh-CN 排序），
 * 支持 ↑/↓/←/→/Enter/Shift+F10 键盘语义、aria-expanded/treeitem 层级、
 * 当前文件 brand marker 与自动显露。不展示绝对 vault 路径与 reserved roots。
 */
export function WorkspaceNavigatorTree({
  tree,
  expanded,
  activePath,
  onToggleFolder,
  onOpenFile,
  onPrepareFile,
  onRowMenu,
}: WorkspaceNavigatorTreeProps) {
  const rows = useMemo(
    () => flattenVaultTree(tree, expanded),
    [expanded, tree],
  );
  const [focusedIndex, setFocusedIndex] = useState(0);
  const containerRef = useRef<HTMLDivElement>(null);
  const rowRefs = useRef(new Map<number, HTMLDivElement>());
  const lastRevealedActivePathRef = useRef<string | null>(null);

  // 当前文件变化时自动显露：展开缺失的祖先文件夹并滚动到该行。
  // 用户显式“折叠全部”后不因同一 activePath 的 state 更新立即反弹展开。
  useEffect(() => {
    if (!activePath || lastRevealedActivePathRef.current === activePath) return;
    lastRevealedActivePathRef.current = activePath;
    for (const ancestor of ancestorFoldersOf(activePath)) {
      if (!expanded.has(ancestor)) onToggleFolder(ancestor);
    }
  }, [activePath, expanded, onToggleFolder]);

  useEffect(() => {
    if (!activePath) return;
    const index = rows.findIndex((row) => row.node.path === activePath);
    if (index >= 0) {
      setFocusedIndex(index);
      rowRefs.current.get(index)?.scrollIntoView?.({ block: "nearest" });
    }
  }, [activePath, rows]);

  const setRowRef = useCallback((index: number, el: HTMLDivElement | null) => {
    if (el) rowRefs.current.set(index, el);
    else rowRefs.current.delete(index);
  }, []);

  const parentIndexOf = useCallback(
    (index: number): number => {
      const depth = rows[index]?.depth ?? 0;
      for (let i = index - 1; i >= 0; i -= 1) {
        if ((rows[i]?.depth ?? 0) < depth) return i;
      }
      return -1;
    },
    [rows],
  );

  const handleKeyDown = useCallback(
    (event: React.KeyboardEvent<HTMLDivElement>) => {
      if (rows.length === 0) return;
      const row = rows[focusedIndex];
      if (!row) return;
      const node = row.node;

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
          if (node.kind === "folder" && expanded.has(node.path)) {
            onToggleFolder(node.path);
          } else {
            const parentIndex = parentIndexOf(focusedIndex);
            if (parentIndex >= 0) setFocusedIndex(parentIndex);
          }
          break;
        case "ArrowRight":
          event.preventDefault();
          if (node.kind === "folder") {
            if (!expanded.has(node.path)) {
              onToggleFolder(node.path);
            } else if (focusedIndex + 1 < rows.length) {
              setFocusedIndex(focusedIndex + 1);
            }
          }
          break;
        case "Enter":
          event.preventDefault();
          if (node.kind === "file") {
            onOpenFile(node);
          } else {
            onToggleFolder(node.path);
          }
          break;
        case "F10":
          if (event.shiftKey) {
            event.preventDefault();
            onRowMenu(node, focusedIndex);
          }
          break;
        case "ContextMenu":
          event.preventDefault();
          onRowMenu(node, focusedIndex);
          break;
        default:
          break;
      }
    },
    [
      expanded,
      focusedIndex,
      onOpenFile,
      onRowMenu,
      onToggleFolder,
      parentIndexOf,
      rows,
    ],
  );

  if (rows.length === 0) {
    return (
      <p className="px-3 py-2 text-[11px] text-muted-foreground">暂无笔记</p>
    );
  }

  return (
    <div
      ref={containerRef}
      role="tree"
      aria-label="笔记库导航"
      tabIndex={0}
      data-testid="workspace-navigator-tree"
      className="min-h-0 flex-1 overflow-y-auto px-1.5 py-1 focus:outline-none"
      onKeyDown={handleKeyDown}
      onBlur={() => {
        // 焦点离开树时回到标题栏入口由入口按钮处理；这里只保持内部索引。
      }}
    >
      {rows.map(({ node, depth, ancestorHasNextSibling }, index) => {
        const isFolder = node.kind === "folder";
        const isActive = node.path === activePath;
        const isExpanded = isFolder && expanded.has(node.path);
        const focused = index === focusedIndex;
        return (
          <div
            key={node.path}
            ref={(el) => setRowRef(index, el)}
            role="treeitem"
            aria-level={depth + 1}
            aria-expanded={isFolder ? isExpanded : undefined}
            aria-selected={isActive || undefined}
            data-testid={
              isFolder ? "workspace-tree-folder" : "workspace-tree-file"
            }
            data-active={isActive || undefined}
            className={cn(
              "group relative flex min-h-7 cursor-pointer items-center gap-1 rounded-md px-1.5 py-1 text-xs",
              isFolder ? "text-foreground/90" : "text-muted-foreground",
              isActive &&
                "bg-[hsl(var(--brand)/0.12)] text-[hsl(var(--brand))] before:absolute before:inset-y-1.5 before:left-0 before:w-px before:bg-[hsl(var(--brand))]",
              focused &&
                !isActive &&
                "ring-1 ring-inset ring-[hsl(var(--brand)/0.35)]",
              !isActive && "hover:bg-muted/50 hover:text-foreground",
            )}
            style={{ paddingLeft: `${0.625 + depth * 1.125}rem` }}
            onClick={() => {
              if (isFolder) onToggleFolder(node.path);
              else onOpenFile(node);
            }}
            onMouseEnter={() => {
              if (!isFolder) onPrepareFile(node);
            }}
            onFocus={() => setFocusedIndex(index)}
            onContextMenu={(event) => {
              event.preventDefault();
              setFocusedIndex(index);
              onRowMenu(node, index);
            }}
          >
            {ancestorHasNextSibling.map((hasNextSibling, guideDepth) =>
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
            {isFolder ? (
              <>
                <ChevronRight
                  className={cn(
                    "h-3 w-3 shrink-0 text-muted-foreground/60 transition-transform duration-fast",
                    isExpanded && "rotate-90",
                  )}
                />
                {isExpanded ? (
                  <FolderOpen
                    aria-hidden="true"
                    data-icon="folder-open"
                    className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                  />
                ) : (
                  <Folder
                    aria-hidden="true"
                    data-icon="folder-closed"
                    className="h-3.5 w-3.5 shrink-0 text-muted-foreground/70"
                  />
                )}
              </>
            ) : (
              <FileText className="h-3.5 w-3.5 shrink-0 text-muted-foreground/60" />
            )}
            <span className="min-w-0 truncate">{node.title ?? node.name}</span>
            {node.locked ? (
              <Lock className="ml-auto h-3 w-3 shrink-0 text-muted-foreground/50" />
            ) : null}
          </div>
        );
      })}
    </div>
  );
}
