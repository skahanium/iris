//! Folder tree renderer for the vault navigator sidebar.
//!
//! Extracted from VaultNavigator so the main component stays a thin
//! orchestrator over catalog state and virtualized lists.

import { ChevronRight, Folder } from "lucide-react";

import { cn } from "@/lib/utils";
import type { VaultTreeNode } from "@/lib/vault-tree";

export function TreeFolder({
  node,
  depth,
  selected,
  expanded,
  onSelect,
  onToggle,
}: {
  node: VaultTreeNode;
  depth: number;
  selected: string;
  expanded: Set<string>;
  onSelect: (path: string) => void;
  onToggle: (path: string) => void;
}) {
  if (node.kind !== "folder") return null;
  const isOpen = expanded.has(node.path);
  const isSelected = selected === node.path;

  return (
    <div>
      <div
        className={cn(
          "group flex w-full items-center gap-1 rounded-md px-2 py-1 text-left text-xs hover:bg-accent",
          isSelected && "bg-accent font-medium text-accent-foreground",
        )}
        style={{ paddingLeft: `${depth * 12 + 8}px` }}
      >
        <button
          type="button"
          className="flex min-w-0 flex-1 items-center gap-1 text-left"
          onClick={() => {
            onSelect(node.path);
            onToggle(node.path);
          }}
        >
          <ChevronRight
            className={cn(
              "h-3 w-3 shrink-0 transition-transform",
              isOpen && "rotate-90",
            )}
          />
          <Folder className="h-3.5 w-3.5 shrink-0 text-muted-foreground" />
          <span className="min-w-0 flex-1 truncate">{node.name}</span>
        </button>
      </div>
      {isOpen &&
        node.children?.map((child) =>
          child.kind === "folder" ? (
            <TreeFolder
              key={child.path}
              node={child}
              depth={depth + 1}
              selected={selected}
              expanded={expanded}
              onSelect={onSelect}
              onToggle={onToggle}
            />
          ) : null,
        )}
    </div>
  );
}
