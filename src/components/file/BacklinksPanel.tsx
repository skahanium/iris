import { useEffect, useState } from "react";

import { ScrollArea } from "@/components/ui/scroll-area";
import { SidePanel } from "@/components/ui/side-panel";
import { fileBacklinks } from "@/lib/ipc";
import type { BacklinkEntry } from "@/types/ipc";

interface BacklinksPanelProps {
  open: boolean;
  onClose: () => void;
  notePath: string | null;
  onOpen: (path: string) => void;
  aiPanelOpen?: boolean;
}

export function BacklinksPanel({
  open,
  onClose,
  notePath,
  onOpen,
  aiPanelOpen = false,
}: BacklinksPanelProps) {
  const [backlinks, setBacklinks] = useState<BacklinkEntry[]>([]);

  useEffect(() => {
    if (!open || !notePath) return;
    void fileBacklinks(notePath).then(setBacklinks);
  }, [open, notePath]);

  return (
    <SidePanel
      open={open}
      onClose={onClose}
      title="反向链接"
      width="sm"
      aiPanelOpen={aiPanelOpen}
    >
      <ScrollArea className="min-h-0 flex-1">
        {backlinks.length === 0 ? (
          <p className="p-3 text-xs text-muted-foreground">无反向链接</p>
        ) : (
          backlinks.map((b) => (
            <button
              key={b.source_path}
              type="button"
              className="w-full border-b border-border/50 px-3 py-2.5 text-left text-sm hover:bg-muted"
              onClick={() => {
                onOpen(b.source_path);
                onClose();
              }}
            >
              <div className="font-medium text-primary">{b.source_title}</div>
              <div className="text-xs text-muted-foreground">
                {b.source_path}
              </div>
              {b.context && (
                <div className="mt-1 line-clamp-2 text-xs text-muted-foreground/70">
                  {b.context}
                </div>
              )}
            </button>
          ))
        )}
      </ScrollArea>
    </SidePanel>
  );
}
