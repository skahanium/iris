import { useVirtualizer } from "@tanstack/react-virtual";
import { FileImage, FileText, FileVideo, Lock } from "lucide-react";
import { useMemo, useRef } from "react";

import type { VaultFileItem } from "@/hooks/useVaultCatalog";
import { cn } from "@/lib/utils";

export interface WorkspaceNavigatorFileListProps {
  files: VaultFileItem[];
  activePath: string | null;
  onOpenFile: (file: VaultFileItem) => void;
  onPrepareFile: (file: VaultFileItem) => void;
  onRowMenu: (
    file: VaultFileItem,
    rowIndex: number,
    x: number,
    y: number,
  ) => void;
}

function fileTitle(file: VaultFileItem): string {
  return file.title || file.path.split("/").pop() || file.path;
}

function fileIcon(file: VaultFileItem) {
  if (file.mediaKind === "image") return FileImage;
  if (file.mediaKind === "video") return FileVideo;
  return FileText;
}

/** Workspace Navigator 下层：只消费已经按选中目录筛好的直属文件。 */
export function WorkspaceNavigatorFileList({
  files,
  activePath,
  onOpenFile,
  onPrepareFile,
  onRowMenu,
}: WorkspaceNavigatorFileListProps) {
  const scrollRef = useRef<HTMLDivElement>(null);
  const virtualizer = useVirtualizer({
    count: files.length,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => 30,
    overscan: 12,
  });
  const virtualRows = virtualizer.getVirtualItems();
  const rows = useMemo(
    () =>
      virtualRows.length > 0
        ? virtualRows
        : files.map((file, index) => ({
            index,
            key: file.path,
            size: 30,
            start: index * 30,
          })),
    [files, virtualRows],
  );
  const totalSize =
    virtualRows.length > 0 ? virtualizer.getTotalSize() : files.length * 30;

  if (files.length === 0) {
    return (
      <div
        data-testid="workspace-navigator-file-list"
        role="list"
        aria-label="当前文件夹文件"
        className="min-h-0 flex-1 px-3 py-2 text-[11px] text-muted-foreground"
      >
        当前目录没有可显示的文件
      </div>
    );
  }

  return (
    <div
      ref={scrollRef}
      data-testid="workspace-navigator-file-list"
      role="list"
      aria-label="当前文件夹文件"
      className="iris-workspace-navigator-scroll min-h-0 flex-1 overflow-y-auto px-1.5 py-1"
    >
      <div style={{ height: totalSize, position: "relative" }}>
        {rows.map((virtualRow) => {
          const file = files[virtualRow.index];
          if (!file) return null;
          const Icon = fileIcon(file);
          const isActive = file.path === activePath;
          return (
            <div
              key={virtualRow.key}
              role="listitem"
              aria-label={fileTitle(file)}
              aria-current={isActive ? "page" : undefined}
              data-active={isActive || undefined}
              className={cn(
                "group absolute left-0 flex h-[30px] w-full cursor-pointer select-none items-center gap-1.5 rounded-md px-2 text-[13px] text-muted-foreground transition-colors duration-150 motion-reduce:transition-none",
                isActive &&
                  "bg-[hsl(var(--brand)/0.12)] text-[hsl(var(--brand))] before:absolute before:inset-y-1 before:left-0 before:w-px before:bg-[hsl(var(--brand))]",
                !isActive && "hover:bg-muted/50 hover:text-foreground",
              )}
              style={{ transform: `translateY(${virtualRow.start}px)` }}
              onClick={() => onOpenFile(file)}
              onMouseEnter={() => onPrepareFile(file)}
              onContextMenu={(event) => {
                event.preventDefault();
                onRowMenu(file, virtualRow.index, event.clientX, event.clientY);
              }}
            >
              <Icon className="h-4 w-4 shrink-0 text-muted-foreground/70" />
              <span className="min-w-0 flex-1 truncate">{fileTitle(file)}</span>
              {file.isLocked ? (
                <Lock className="h-3.5 w-3.5 shrink-0 text-muted-foreground/60" />
              ) : null}
            </div>
          );
        })}
      </div>
    </div>
  );
}
