import { useVirtualizer } from "@tanstack/react-virtual";
import { useEffect, useRef, useState } from "react";

import { Button } from "@/components/ui/button";
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface QuickOpenProps {
  open: boolean;
  onClose: () => void;
  onSelect: (path: string) => void;
}

export function QuickOpen({ open, onClose, onSelect }: QuickOpenProps) {
  const [query, setQuery] = useState("");
  const [files, setFiles] = useState<FileListItem[]>([]);
  const parentRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    void fileList().then(setFiles);
    setQuery("");
  }, [open]);

  const filtered = files.filter(
    (f) =>
      f.title.toLowerCase().includes(query.toLowerCase()) ||
      f.path.toLowerCase().includes(query.toLowerCase()),
  );

  const virtualizer = useVirtualizer({
    count: filtered.length,
    getScrollElement: () => parentRef.current,
    estimateSize: () => 52,
    overscan: 10,
  });

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent
        size="compact"
        className="gap-0 overflow-hidden p-0"
      >
        <DialogHeader className="sr-only">
          <DialogTitle>搜索笔记</DialogTitle>
        </DialogHeader>
        <Input
          className="rounded-none border-0 border-b pr-10 focus-visible:ring-0"
          placeholder="搜索笔记…"
          value={query}
          autoFocus
          onChange={(e) => setQuery(e.target.value)}
        />
        <div ref={parentRef} className="max-h-80 overflow-auto">
          <div
            style={{ height: `${virtualizer.getTotalSize()}px`, position: "relative" }}
          >
            {virtualizer.getVirtualItems().map((virtualItem) => {
              const f = filtered[virtualItem.index]!;
              return (
                <button
                  key={f.path}
                  type="button"
                  style={{
                    position: "absolute",
                    top: 0,
                    left: 0,
                    width: "100%",
                    height: `${virtualItem.size}px`,
                    transform: `translateY(${virtualItem.start}px)`,
                  }}
                  className="flex w-full flex-col px-4 py-2 text-left text-sm hover:bg-muted"
                  onClick={() => {
                    onSelect(f.path);
                    onClose();
                  }}
                >
                  <span>{f.title}</span>
                  <span className="text-xs text-muted-foreground">{f.path}</span>
                </button>
              );
            })}
          </div>
        </div>
        <div className="flex justify-end border-t border-border p-2">
          <Button type="button" size="sm" variant="ghost" onClick={onClose}>
            Esc 关闭
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}
