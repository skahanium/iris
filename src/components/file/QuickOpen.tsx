import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Dialog, DialogContent, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
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

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="top-[15vh] translate-y-0 gap-0 p-0">
        <DialogHeader className="sr-only">
          <DialogTitle>搜索笔记</DialogTitle>
        </DialogHeader>
        <Input
          className="border-0 border-b rounded-none focus-visible:ring-0"
          placeholder="搜索笔记…"
          value={query}
          autoFocus
          onChange={(e) => setQuery(e.target.value)}
        />
        <ScrollArea className="max-h-80">
          {filtered.map((f) => (
            <button
              key={f.path}
              type="button"
              className="flex w-full flex-col px-4 py-2 text-left text-sm hover:bg-muted"
              onClick={() => {
                onSelect(f.path);
                onClose();
              }}
            >
              <span>{f.title}</span>
              <span className="text-xs text-muted-foreground">{f.path}</span>
            </button>
          ))}
        </ScrollArea>
        <div className="flex justify-end border-t border-border p-2">
          <Button type="button" size="sm" variant="ghost" onClick={onClose}>
            Esc 关闭
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}