import { useEffect, useState } from "react";

import { Button } from "@/components/ui/button";
import { Input } from "@/components/ui/input";
import { ScrollArea } from "@/components/ui/scroll-area";
import { fileList } from "@/lib/ipc";
import type { FileListItem } from "@/types/ipc";

interface TagViewProps {
  open: boolean;
  onClose: () => void;
}

interface TagGroup {
  name: string;
  files: FileListItem[];
}

interface Stats {
  totalNotes: number;
  totalTags: number;
  totalLinks: number;
  totalWords: number;
}

export function TagView({ open, onClose }: TagViewProps) {
  const [tags, setTags] = useState<TagGroup[]>([]);
  const [stats, setStats] = useState<Stats | null>(null);
  const [expanded, setExpanded] = useState<string | null>(null);
  const [search, setSearch] = useState("");

  useEffect(() => {
    if (!open) return;
    void fileList().then((files) => {
      // Build tag groups from file titles and frontmatter tags
      const tagMap = new Map<string, FileListItem[]>();
      // Simple tag extraction from file titles (backend handles real indexing)
      for (const f of files) {
        const words = f.title.split(/\s+/);
        for (const w of words) {
          if (w.length > 1) {
            const existing = tagMap.get(w) ?? [];
            existing.push(f);
            tagMap.set(w, existing);
          }
        }
      }
      const groups: TagGroup[] = [...tagMap.entries()]
        .map(([name, tagFiles]) => ({ name, files: tagFiles }))
        .sort((a, b) => b.files.length - a.files.length);

      setTags(groups);
      setStats({
        totalNotes: files.length,
        totalTags: tagMap.size,
        totalLinks: 0, // Backend-driven; update via future IPC
        totalWords: 0,
      });
    });
  }, [open]);

  if (!open) return null;

  const filtered = tags.filter((t) =>
    t.name.toLowerCase().includes(search.toLowerCase()),
  );

  return (
    <div className="fixed inset-y-0 right-0 z-50 flex w-72 flex-col border-l border-border bg-panel shadow-xl">
      <div className="flex items-center justify-between border-b border-border px-3 py-2">
        <span className="text-sm font-medium">标签</span>
        <Button type="button" size="sm" variant="ghost" onClick={onClose}>
          Esc
        </Button>
      </div>

      {stats && (
        <div className="flex gap-3 border-b border-border px-3 py-2 text-xs text-muted-foreground">
          <span>{stats.totalNotes} 笔记</span>
          <span>{stats.totalTags} 标签</span>
        </div>
      )}

      <div className="border-b border-border px-2 py-1.5">
        <Input
          className="h-7 text-xs"
          placeholder="过滤标签…"
          value={search}
          onChange={(e) => setSearch(e.target.value)}
        />
      </div>

      <ScrollArea className="flex-1">
        {filtered.length === 0 ? (
          <p className="p-3 text-xs text-muted-foreground">无标签</p>
        ) : (
          filtered.map((t) => (
            <div key={t.name}>
              <button
                type="button"
                className="flex w-full items-center justify-between border-b border-border/50 px-3 py-2 text-left text-sm hover:bg-muted"
                onClick={() =>
                  setExpanded(expanded === t.name ? null : t.name)
                }
              >
                <span className="text-primary">#{t.name}</span>
                <span className="text-xs text-muted-foreground">
                  {t.files.length}
                </span>
              </button>
              {expanded === t.name && (
                <div className="bg-muted/30">
                  {t.files.map((f) => (
                    <div
                      key={f.path}
                      className="border-b border-border/30 px-5 py-1 text-xs text-muted-foreground"
                    >
                      {f.title}
                    </div>
                  ))}
                </div>
              )}
            </div>
          ))
        )}
      </ScrollArea>
    </div>
  );
}
